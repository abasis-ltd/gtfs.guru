use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_TRIP_EDGE: &str = "missing_trip_edge";

#[derive(Debug, Default)]
pub struct MissingTripEdgeValidator;

impl Validator for MissingTripEdgeValidator {
    fn name(&self) -> &'static str {
        "missing_trip_edge"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut by_trip: HashMap<&str, Vec<(u64, &gtfs_model::StopTime)>> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            by_trip
                .entry(trip_id)
                .or_default()
                .push((row_number, stop_time));
        }
        for stop_times in by_trip.values_mut() {
            stop_times.sort_by_key(|(_, stop_time)| stop_time.stop_sequence);
        }

        for stop_times in by_trip.values() {
            if stop_times.is_empty() {
                continue;
            }
            let (first_row, first) = stop_times[0];
            let (last_row, last) = stop_times[stop_times.len() - 1];
            check_trip_edge(first, first_row, notices);
            check_trip_edge(last, last_row, notices);
        }
    }
}

fn check_trip_edge(
    stop_time: &gtfs_model::StopTime,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    if stop_time.start_pickup_drop_off_window.is_some()
        || stop_time.end_pickup_drop_off_window.is_some()
    {
        return;
    }

    if stop_time.arrival_time.is_none() {
        notices.push(missing_trip_edge_notice(
            stop_time,
            row_number,
            "arrival_time",
        ));
    }
    if stop_time.departure_time.is_none() {
        notices.push(missing_trip_edge_notice(
            stop_time,
            row_number,
            "departure_time",
        ));
    }
}

fn missing_trip_edge_notice(
    stop_time: &gtfs_model::StopTime,
    row_number: u64,
    field: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_TRIP_EDGE,
        NoticeSeverity::Error,
        "missing arrival_time or departure_time at trip edge",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("specifiedField", field);
    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
    notice.insert_context_field("tripId", stop_time.trip_id.as_str());
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "specifiedField".to_string(),
        "stopSequence".to_string(),
        "tripId".to_string(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::{GtfsTime, StopTime};

    #[test]
    fn detects_missing_arrival_at_start() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "departure_time".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    arrival_time: None,
                    departure_time: Some(GtfsTime::from_seconds(3600)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_MISSING_TRIP_EDGE
            && n.message.contains("missing arrival_time or departure_time")));
    }

    #[test]
    fn detects_missing_departure_at_end() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "arrival_time".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    arrival_time: Some(GtfsTime::from_seconds(3600)),
                    departure_time: Some(GtfsTime::from_seconds(3700)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: None,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_MISSING_TRIP_EDGE));
    }

    #[test]
    fn passes_valid_trip_edges() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "arrival_time".to_string(),
                "departure_time".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    arrival_time: Some(GtfsTime::from_seconds(3600)),
                    departure_time: Some(GtfsTime::from_seconds(3700)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_flex_windows() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "start_pickup_drop_off_window".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
