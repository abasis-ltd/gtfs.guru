use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

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
        let has_flex_window = headers
            .iter()
            .any(|header| {
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
            
            let has_flex_window = stop_time.start_pickup_drop_off_window.is_some() || stop_time.end_pickup_drop_off_window.is_some();
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
                notice.insert_context_field("tripId", stop_time.trip_id.as_str());
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "shapeDistTraveled".to_string(),
                    "tripId".to_string(),
                ];
                notices.push(notice);
                continue;
            }

            if has_stop_id(stop_time) {
                continue;
            }
            if (stop_time.location_group_id.is_some() || stop_time.location_id.is_some())
                && stop_time.shape_dist_traveled.is_some()
            {
                let mut notice = ValidationNotice::new(
                    CODE_FORBIDDEN_SHAPE_DIST_TRAVELED,
                    NoticeSeverity::Error,
                    "shape_dist_traveled is forbidden without stop_id",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field(
                    "locationGroupId",
                    stop_time.location_group_id.as_deref().unwrap_or(""),
                );
                notice.insert_context_field(
                    "locationId",
                    stop_time.location_id.as_deref().unwrap_or(""),
                );
                if let Some(shape_dist) = stop_time.shape_dist_traveled {
                    notice.insert_context_field("shapeDistTraveled", shape_dist);
                }
                notice.insert_context_field("tripId", stop_time.trip_id.as_str());
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "locationGroupId".to_string(),
                    "locationId".to_string(),
                    "shapeDistTraveled".to_string(),
                    "tripId".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn has_stop_id(stop_time: &gtfs_model::StopTime) -> bool {
    !stop_time.stop_id.trim().is_empty()
}

