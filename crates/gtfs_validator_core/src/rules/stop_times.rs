use std::collections::HashSet;

use crate::feed::{STOP_TIMES_FILE, TRIPS_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_TRIP: &str = "unused_trip";

#[derive(Debug, Default)]
pub struct TripUsageValidator;

impl Validator for TripUsageValidator {
    fn name(&self) -> &'static str {
        "trip_usage"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Only run in thorough mode to match Java default behavior
        if feed.table_has_errors(TRIPS_FILE) || feed.table_has_errors(STOP_TIMES_FILE) {
            return;
        }

        let stop_time_trips: HashSet<gtfs_guru_model::StringId> = feed
            .stop_times
            .rows
            .iter()
            .map(|row| row.trip_id)
            .filter(|id| id.0 != 0)
            .collect();
        let mut reported: HashSet<gtfs_guru_model::StringId> = HashSet::new();

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let trip_id = trip.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            if reported.insert(trip_id) && !stop_time_trips.contains(&trip_id) {
                let trip_id_value = feed.pool.resolve(trip_id);
                let mut notice = ValidationNotice::new(
                    CODE_UNUSED_TRIP,
                    NoticeSeverity::Warning,
                    "trip is not referenced in stop_times",
                );
                notice.insert_context_field("tripId", trip_id_value.as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into(), "tripId".into()];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{StopTime, Trip};

    #[test]
    fn detects_unused_trip() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![],
            row_numbers: vec![],
        };

        let mut notices = NoticeContainer::new();
        TripUsageValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_UNUSED_TRIP);
        assert_eq!(
            notice.context.get("tripId").unwrap().as_str().unwrap(),
            "T1"
        );
    }

    #[test]
    fn passes_when_trip_is_used() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripUsageValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn reports_each_unused_trip_once() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![
                Trip {
                    trip_id: feed.pool.intern("T1"),
                    ..Default::default()
                },
                Trip {
                    trip_id: feed.pool.intern("T1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![],
            row_numbers: vec![],
        };

        let mut notices = NoticeContainer::new();
        TripUsageValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
    }
}
