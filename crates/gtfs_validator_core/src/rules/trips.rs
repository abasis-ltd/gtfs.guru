use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSABLE_TRIP: &str = "unusable_trip";

#[derive(Debug, Default)]
pub struct TripUsabilityValidator;

impl Validator for TripUsabilityValidator {
    fn name(&self) -> &'static str {
        "trip_usability"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut stop_counts: HashMap<&str, usize> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            *stop_counts.entry(trip_id).or_insert(0) += 1;
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let stop_count = stop_counts.get(trip_id).copied().unwrap_or(0);
            if stop_count == 0 {
                let mut notice = ValidationNotice::new(
                    "missing_stop_times_record",
                    NoticeSeverity::Error,
                    "trip must have at least one stop_times entry",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("tripId", trip_id);
                notice.field_order = vec!["csvRowNumber".to_string(), "tripId".to_string()];
                notices.push(notice);
            } else if stop_count == 1 {
                let mut notice = ValidationNotice::new(
                    CODE_UNUSABLE_TRIP,
                    NoticeSeverity::Warning,
                    "trip must have at least two stop_times entries to be usable",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("tripId", trip_id);
                notice.field_order = vec!["csvRowNumber".to_string(), "tripId".to_string()];
                notices.push(notice);
            }
        }
    }
}

