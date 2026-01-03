use std::collections::HashSet;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_TRIP: &str = "unused_trip";

#[derive(Debug, Default)]
pub struct TripUsageValidator;

impl Validator for TripUsageValidator {
    fn name(&self) -> &'static str {
        "trip_usage"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let stop_time_trips: HashSet<&str> = feed
            .stop_times
            .rows
            .iter()
            .map(|row| row.trip_id.trim())
            .filter(|value| !value.is_empty())
            .collect();
        let mut reported: HashSet<&str> = HashSet::new();

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            if reported.insert(trip_id) && !stop_time_trips.contains(trip_id) {
                let mut notice = ValidationNotice::new(
                    CODE_UNUSED_TRIP,
                    NoticeSeverity::Warning,
                    "trip is not referenced in stop_times",
                );
                notice.insert_context_field("tripId", trip_id);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".to_string(), "tripId".to_string()];
                notices.push(notice);
            }
        }
    }
}

