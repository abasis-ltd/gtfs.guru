use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_STOP_TIME_WITH_ONLY_ARRIVAL_OR_DEPARTURE_TIME: &str =
    "stop_time_with_only_arrival_or_departure_time";
const CODE_STOP_TIME_WITH_ARRIVAL_BEFORE_PREVIOUS_DEPARTURE_TIME: &str =
    "stop_time_with_arrival_before_previous_departure_time";
const CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES: &str = "stop_time_timepoint_without_times";
const CODE_MISSING_TIMEPOINT_VALUE: &str = "missing_timepoint_value";

#[derive(Debug, Default)]
pub struct StopTimeArrivalAndDepartureTimeValidator;

impl Validator for StopTimeArrivalAndDepartureTimeValidator {
    fn name(&self) -> &'static str {
        "stop_time_arrival_departure_time"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut by_trip: HashMap<&str, Vec<(usize, &gtfs_model::StopTime)>> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            by_trip.entry(trip_id).or_default().push((index, stop_time));
        }
        for stop_times in by_trip.values_mut() {
            stop_times.sort_by_key(|(_, stop_time)| stop_time.stop_sequence);
        }

        for stop_times in by_trip.values() {
            let mut previous_departure: Option<(gtfs_model::GtfsTime, u64)> = None;
            for (index, stop_time) in stop_times {
                let row_number = feed.stop_times.row_number(*index);
                let trip_id = stop_time.trip_id.trim();
                let has_arrival = stop_time.arrival_time.is_some();
                let has_departure = stop_time.departure_time.is_some();
                if has_arrival != has_departure {
                    let specified_field = if has_arrival {
                        "arrival_time"
                    } else {
                        "departure_time"
                    };
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TIME_WITH_ONLY_ARRIVAL_OR_DEPARTURE_TIME,
                        NoticeSeverity::Error,
                        "arrival_time and departure_time must both be set or both empty",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("specifiedField", specified_field);
                    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                    notice.insert_context_field("tripId", trip_id);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "specifiedField".to_string(),
                        "stopSequence".to_string(),
                        "tripId".to_string(),
                    ];
                    notices.push(notice);
                }

                if let (Some(arrival), Some((prev_departure, prev_row_number))) =
                    (stop_time.arrival_time, previous_departure)
                {
                    if arrival.total_seconds() < prev_departure.total_seconds() {
                        let mut notice = ValidationNotice::new(
                            CODE_STOP_TIME_WITH_ARRIVAL_BEFORE_PREVIOUS_DEPARTURE_TIME,
                            NoticeSeverity::Error,
                            "arrival_time is before previous stop departure_time",
                        );
                        notice.insert_context_field("arrivalTime", arrival);
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("departureTime", prev_departure);
                        notice.insert_context_field("prevCsvRowNumber", prev_row_number);
                        notice.insert_context_field("tripId", trip_id);
                        notice.field_order = vec![
                            "arrivalTime".to_string(),
                            "csvRowNumber".to_string(),
                            "departureTime".to_string(),
                            "prevCsvRowNumber".to_string(),
                            "tripId".to_string(),
                        ];
                        notices.push(notice);
                    }
                }

                if let Some(departure) = stop_time.departure_time {
                    previous_departure = Some((departure, row_number));
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct TimepointTimeValidator;

impl Validator for TimepointTimeValidator {
    fn name(&self) -> &'static str {
        "timepoint_time"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let has_timepoint_column = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("timepoint"));
        if !has_timepoint_column {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            let has_arrival = stop_time.arrival_time.is_some();
            let has_departure = stop_time.departure_time.is_some();
            let has_timepoint = stop_time.timepoint.is_some();

            if (has_arrival || has_departure) && !has_timepoint {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_TIMEPOINT_VALUE,
                    NoticeSeverity::Warning,
                    "timepoint is required when arrival_time or departure_time is provided",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                notice.insert_context_field("tripId", trip_id);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "stopSequence".to_string(),
                    "tripId".to_string(),
                ];
                notices.push(notice);
            }

            if matches!(stop_time.timepoint, Some(gtfs_model::Timepoint::Exact)) {
                if !has_arrival {
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES,
                        NoticeSeverity::Error,
                        "timepoint=1 requires arrival_time and departure_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("specifiedField", "arrival_time");
                    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                    notice.insert_context_field("tripId", trip_id);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "specifiedField".to_string(),
                        "stopSequence".to_string(),
                        "tripId".to_string(),
                    ];
                    notices.push(notice);
                }
                if !has_departure {
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES,
                        NoticeSeverity::Error,
                        "timepoint=1 requires arrival_time and departure_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("specifiedField", "departure_time");
                    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                    notice.insert_context_field("tripId", trip_id);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "specifiedField".to_string(),
                        "stopSequence".to_string(),
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
    use gtfs_model::{GtfsTime, StopTime, Timepoint};

    #[test]
    fn detects_only_one_time_specified() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "arrival_time".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_sequence: 1,
                arrival_time: Some(GtfsTime::from_seconds(3600)),
                departure_time: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimeArrivalAndDepartureTimeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STOP_TIME_WITH_ONLY_ARRIVAL_OR_DEPARTURE_TIME));
    }

    #[test]
    fn detects_arrival_before_previous_departure() {
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
                    arrival_time: Some(GtfsTime::from_seconds(3650)), // Before 3700
                    departure_time: Some(GtfsTime::from_seconds(3800)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        StopTimeArrivalAndDepartureTimeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STOP_TIME_WITH_ARRIVAL_BEFORE_PREVIOUS_DEPARTURE_TIME));
    }

    #[test]
    fn detects_timepoint_without_times() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "timepoint".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_sequence: 1,
                timepoint: Some(Timepoint::Exact),
                arrival_time: None,
                departure_time: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TimepointTimeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES));
    }

    #[test]
    fn detects_missing_timepoint_value() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "arrival_time".to_string(),
                "timepoint".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_sequence: 1,
                arrival_time: Some(GtfsTime::from_seconds(3600)),
                departure_time: Some(GtfsTime::from_seconds(3600)),
                timepoint: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TimepointTimeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_TIMEPOINT_VALUE));
    }

    #[test]
    fn passes_valid_times_and_timepoints() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_sequence".to_string(),
                "arrival_time".to_string(),
                "departure_time".to_string(),
                "timepoint".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    arrival_time: Some(GtfsTime::from_seconds(3600)),
                    departure_time: Some(GtfsTime::from_seconds(3700)),
                    timepoint: Some(Timepoint::Exact),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    timepoint: Some(Timepoint::Exact),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        StopTimeArrivalAndDepartureTimeValidator.validate(&feed, &mut notices);
        TimepointTimeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
