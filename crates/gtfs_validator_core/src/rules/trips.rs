use std::collections::HashMap;

use crate::feed::{STOP_TIMES_FILE, TRIPS_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSABLE_TRIP: &str = "unusable_trip";

#[derive(Debug, Default)]
pub struct TripUsabilityValidator;

impl Validator for TripUsabilityValidator {
    fn name(&self) -> &'static str {
        "trip_usability"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.table_has_errors(TRIPS_FILE) || feed.table_has_errors(STOP_TIMES_FILE) {
            return;
        }

        let mut stop_counts: HashMap<gtfs_guru_model::StringId, usize> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            *stop_counts.entry(trip_id).or_insert(0) += 1;
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let trip_id = trip.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            let stop_count = stop_counts.get(&trip_id).copied().unwrap_or(0);
            if stop_count <= 1 {
                let trip_id_value = feed.pool.resolve(trip_id);
                let mut notice = ValidationNotice::new(
                    CODE_UNUSABLE_TRIP,
                    NoticeSeverity::Warning,
                    "trip must have at least two stop_times entries to be usable",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("tripId", trip_id_value.as_str());
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
    use crate::TableStatus;
    use gtfs_guru_model::{StopTime, Trip};

    #[test]
    fn detects_trip_with_no_stop_times() {
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
        TripUsabilityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_UNUSABLE_TRIP);
    }

    #[test]
    fn detects_trip_with_single_stop_time() {
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
                stop_sequence: 1,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripUsabilityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_UNUSABLE_TRIP);
    }

    #[test]
    fn passes_with_two_or_more_stop_times() {
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
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        TripUsabilityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_when_stop_times_missing() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.table_statuses
            .insert(STOP_TIMES_FILE, TableStatus::MissingFile);

        let mut notices = NoticeContainer::new();
        TripUsabilityValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
