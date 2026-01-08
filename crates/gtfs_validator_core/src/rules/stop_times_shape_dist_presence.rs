use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;

const CODE_FORBIDDEN_SHAPE_DIST_TRAVELED: &str = "forbidden_shape_dist_traveled";

#[derive(Debug, Default)]
pub struct StopTimesShapeDistTraveledPresenceValidator;

impl Validator for StopTimesShapeDistTraveledPresenceValidator {
    fn name(&self) -> &'static str {
        "stop_times_shape_dist_traveled_presence"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let headers = &feed.stop_times.headers;
        let has_shape_dist = headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("shape_dist_traveled"));
        let has_location_id = headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("location_id"));
        let has_location_group_id = headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("location_group_id"));
        let has_flex_window = headers.iter().any(|header| {
            header.eq_ignore_ascii_case("start_pickup_drop_off_window")
                || header.eq_ignore_ascii_case("end_pickup_drop_off_window")
        });

        if !has_shape_dist {
            return;
        }
        if !(has_location_id || has_location_group_id || has_flex_window) {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);

            let has_flex_window = stop_time.start_pickup_drop_off_window.is_some()
                || stop_time.end_pickup_drop_off_window.is_some();
            let has_shape_dist = stop_time.shape_dist_traveled.is_some();

            if has_shape_dist && has_flex_window {
                let mut notice = ValidationNotice::new(
                    CODE_FORBIDDEN_SHAPE_DIST_TRAVELED,
                    NoticeSeverity::Error,
                    "shape_dist_traveled is forbidden when pickup/drop-off windows are defined",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                if let Some(shape_dist) = stop_time.shape_dist_traveled {
                    notice.insert_context_field("shapeDistTraveled", shape_dist);
                }
                notice
                    .insert_context_field("tripId", feed.pool.resolve(stop_time.trip_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "shapeDistTraveled".into(),
                    "tripId".into(),
                ];
                notices.push(notice);
                continue;
            }

            if has_stop_id(stop_time, feed) {
                continue;
            }
            if (stop_time.location_group_id.is_some() || stop_time.location_id.is_some())
                && stop_time.shape_dist_traveled.is_some()
            {
                let location_group_value = feed
                    .pool
                    .resolve(stop_time.location_group_id.unwrap_or(StringId(0)));
                let location_id_value = stop_time.location_id.map(|id| feed.pool.resolve(id));
                let mut notice = ValidationNotice::new(
                    CODE_FORBIDDEN_SHAPE_DIST_TRAVELED,
                    NoticeSeverity::Error,
                    "shape_dist_traveled is forbidden without stop_id",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("locationGroupId", location_group_value.as_str());
                notice
                    .insert_context_field("locationId", location_id_value.as_deref().unwrap_or(""));
                if let Some(shape_dist) = stop_time.shape_dist_traveled {
                    notice.insert_context_field("shapeDistTraveled", shape_dist);
                }
                notice
                    .insert_context_field("tripId", feed.pool.resolve(stop_time.trip_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "locationGroupId".into(),
                    "locationId".into(),
                    "shapeDistTraveled".into(),
                    "tripId".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn has_stop_id(stop_time: &gtfs_guru_model::StopTime, _feed: &GtfsFeed) -> bool {
    stop_time.stop_id.0 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, StopTime};

    #[test]
    fn detects_shape_dist_with_flex_window() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "shape_dist_traveled".into(),
                "start_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                shape_dist_traveled: Some(10.0),
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesShapeDistTraveledPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_SHAPE_DIST_TRAVELED
        );
        assert!(notices
            .iter()
            .next()
            .unwrap()
            .message
            .contains("forbidden when pickup/drop-off windows are defined"));
    }

    #[test]
    fn detects_shape_dist_without_stop_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "shape_dist_traveled".into(),
                "location_id".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                shape_dist_traveled: Some(10.0),
                location_id: Some(feed.pool.intern("L1")),
                stop_id: StringId(0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesShapeDistTraveledPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_SHAPE_DIST_TRAVELED
        );
        assert!(notices
            .iter()
            .next()
            .unwrap()
            .message
            .contains("forbidden without stop_id"));
    }

    #[test]
    fn passes_valid_shape_dist() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "shape_dist_traveled".into(),
                "stop_id".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                shape_dist_traveled: Some(10.0),
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesShapeDistTraveledPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
