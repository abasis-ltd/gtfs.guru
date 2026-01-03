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

