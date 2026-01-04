use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::PickupDropOffType;

const CODE_MISSING_STOP_TIMES_RECORD: &str = "missing_stop_times_record";

#[derive(Debug, Default)]
pub struct StopTimesRecordValidator;

impl Validator for StopTimesRecordValidator {
    fn name(&self) -> &'static str {
        "stop_times_record"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
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

        let mut trip_counts: HashMap<&str, usize> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            *trip_counts.entry(trip_id).or_insert(0) += 1;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let has_windows = stop_time.start_pickup_drop_off_window.is_some()
                && stop_time.end_pickup_drop_off_window.is_some();
            let must_phone = stop_time.pickup_type == Some(PickupDropOffType::MustPhone)
                && stop_time.drop_off_type == Some(PickupDropOffType::MustPhone);
            if has_windows && must_phone {
                let count = trip_counts.get(trip_id).copied().unwrap_or(0);
                if count == 1 {
                    let mut notice = ValidationNotice::new(
                        CODE_MISSING_STOP_TIMES_RECORD,
                        NoticeSeverity::Error,
                        "only one stop_times record present for pickup/dropoff window",
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
                    notice.insert_context_field("tripId", trip_id);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "locationGroupId".to_string(),
                        "locationId".to_string(),
                        "tripId".to_string(),
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
    use gtfs_model::{GtfsTime, PickupDropOffType, StopTime};

    #[test]
    fn detects_single_record_with_window() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "start_pickup_drop_off_window".to_string(),
                "end_pickup_drop_off_window".to_string(),
                "pickup_type".to_string(),
                "drop_off_type".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
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
                "trip_id".to_string(),
                "start_pickup_drop_off_window".to_string(),
                "end_pickup_drop_off_window".to_string(),
                "pickup_type".to_string(),
                "drop_off_type".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                    pickup_type: Some(PickupDropOffType::MustPhone),
                    drop_off_type: Some(PickupDropOffType::MustPhone),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S1".to_string(),
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
