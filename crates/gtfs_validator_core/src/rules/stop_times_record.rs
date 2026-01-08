use std::collections::HashMap;

use crate::feed::STOP_TIMES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::PickupDropOffType;
use gtfs_guru_model::StringId;

const CODE_MISSING_STOP_TIMES_RECORD: &str = "missing_stop_times_record";

#[derive(Debug, Default)]
pub struct StopTimesRecordValidator;

impl Validator for StopTimesRecordValidator {
    fn name(&self) -> &'static str {
        "stop_times_record"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.table_has_errors(STOP_TIMES_FILE) {
            return;
        }

        let headers = &feed.stop_times.headers;
        let required_columns = [
            "start_pickup_drop_off_window",
            "end_pickup_drop_off_window",
            "pickup_type",
            "drop_off_type",
        ];
        if !required_columns.iter().all(|column| {
            headers
                .iter()
                .any(|header| header.eq_ignore_ascii_case(column))
        }) {
            return;
        }

        let mut trip_counts: HashMap<StringId, usize> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            *trip_counts.entry(trip_id).or_insert(0) += 1;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            let has_windows = stop_time.start_pickup_drop_off_window.is_some()
                && stop_time.end_pickup_drop_off_window.is_some();
            let must_phone = stop_time.pickup_type == Some(PickupDropOffType::MustPhone)
                && stop_time.drop_off_type == Some(PickupDropOffType::MustPhone);
            if has_windows && must_phone {
                let count = trip_counts.get(&trip_id).copied().unwrap_or(0);
                if count == 1 {
                    let location_group_value = feed
                        .pool
                        .resolve(stop_time.location_group_id.unwrap_or(StringId(0)));
                    let location_id_value = stop_time.location_id.map(|id| feed.pool.resolve(id));
                    let trip_id_value = feed.pool.resolve(trip_id);
                    let mut notice = ValidationNotice::new(
                        CODE_MISSING_STOP_TIMES_RECORD,
                        NoticeSeverity::Error,
                        "only one stop_times record present for pickup/dropoff window",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("locationGroupId", location_group_value.as_str());
                    notice.insert_context_field(
                        "locationId",
                        location_id_value.as_deref().unwrap_or(""),
                    );
                    notice.insert_context_field("tripId", trip_id_value.as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "locationGroupId".into(),
                        "locationId".into(),
                        "tripId".into(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, PickupDropOffType, StopTime};

    #[test]
    fn detects_single_record_with_window() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
                "pickup_type".into(),
                "drop_off_type".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                pickup_type: Some(PickupDropOffType::MustPhone),
                drop_off_type: Some(PickupDropOffType::MustPhone),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesRecordValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_STOP_TIMES_RECORD
        );
    }

    #[test]
    fn passes_multiple_records() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
                "pickup_type".into(),
                "drop_off_type".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                    pickup_type: Some(PickupDropOffType::MustPhone),
                    drop_off_type: Some(PickupDropOffType::MustPhone),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        StopTimesRecordValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
