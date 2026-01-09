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
        for (trip_id, indices) in &feed.stop_times_by_trip {
            let mut previous_departure: Option<(gtfs_guru_model::GtfsTime, u64)> = None;
            for &index in indices {
                let stop_time = &feed.stop_times.rows[index];
                let row_number = feed.stop_times.row_number(index);
                let has_arrival = stop_time.arrival_time.is_some();
                let has_departure = stop_time.departure_time.is_some();
                if has_arrival != has_departure {
                    let trip_id = feed.pool.resolve(*trip_id);
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
                    notice.insert_context_field("tripId", trip_id.as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "specifiedField".into(),
                        "stopSequence".into(),
                        "tripId".into(),
                    ];
                    notices.push(notice);
                }

                if let (Some(arrival), Some((prev_departure, prev_row_number))) =
                    (stop_time.arrival_time, previous_departure)
                {
                    if arrival.total_seconds() < prev_departure.total_seconds() {
                        let trip_id = feed.pool.resolve(*trip_id);
                        let mut notice = ValidationNotice::new(
                            CODE_STOP_TIME_WITH_ARRIVAL_BEFORE_PREVIOUS_DEPARTURE_TIME,
                            NoticeSeverity::Error,
                            "arrival_time is before previous stop departure_time",
                        );
                        notice.insert_context_field("arrivalTime", arrival);
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("departureTime", prev_departure);
                        notice.insert_context_field("prevCsvRowNumber", prev_row_number);
                        notice.insert_context_field("tripId", trip_id.as_str());
                        notice.field_order = vec![
                            "arrivalTime".into(),
                            "csvRowNumber".into(),
                            "departureTime".into(),
                            "prevCsvRowNumber".into(),
                            "tripId".into(),
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
            let has_arrival = stop_time.arrival_time.is_some();
            let has_departure = stop_time.departure_time.is_some();
            let has_timepoint = stop_time.timepoint.is_some();

            if (has_arrival || has_departure) && !has_timepoint {
                let trip_id = feed.pool.resolve(stop_time.trip_id);
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_TIMEPOINT_VALUE,
                    NoticeSeverity::Warning,
                    "timepoint is required when arrival_time or departure_time is provided",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                notice.insert_context_field("tripId", trip_id.as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "stopSequence".into(),
                    "tripId".into(),
                ];
                notices.push(notice);
            }

            if matches!(stop_time.timepoint, Some(gtfs_guru_model::Timepoint::Exact)) {
                if !has_arrival {
                    let trip_id = feed.pool.resolve(stop_time.trip_id);
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES,
                        NoticeSeverity::Error,
                        "timepoint=1 requires arrival_time and departure_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("specifiedField", "arrival_time");
                    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                    notice.insert_context_field("tripId", trip_id.as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "specifiedField".into(),
                        "stopSequence".into(),
                        "tripId".into(),
                    ];
                    notices.push(notice);
                }
                if !has_departure {
                    let trip_id = feed.pool.resolve(stop_time.trip_id);
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TIME_TIMEPOINT_WITHOUT_TIMES,
                        NoticeSeverity::Error,
                        "timepoint=1 requires arrival_time and departure_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("specifiedField", "departure_time");
                    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                    notice.insert_context_field("tripId", trip_id.as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "specifiedField".into(),
                        "stopSequence".into(),
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
    use gtfs_guru_model::{GtfsTime, StopTime, Timepoint};

    #[test]
    fn detects_only_one_time_specified() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "arrival_time".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                arrival_time: Some(GtfsTime::from_seconds(3600)),
                departure_time: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.rebuild_stop_times_index();

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
                "trip_id".into(),
                "stop_sequence".into(),
                "arrival_time".into(),
                "departure_time".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    arrival_time: Some(GtfsTime::from_seconds(3600)),
                    departure_time: Some(GtfsTime::from_seconds(3700)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(3650)), // Before 3700
                    departure_time: Some(GtfsTime::from_seconds(3800)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

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
            headers: vec!["trip_id".into(), "stop_sequence".into(), "timepoint".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
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
                "trip_id".into(),
                "stop_sequence".into(),
                "arrival_time".into(),
                "timepoint".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
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
                "trip_id".into(),
                "stop_sequence".into(),
                "arrival_time".into(),
                "departure_time".into(),
                "timepoint".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    arrival_time: Some(GtfsTime::from_seconds(3600)),
                    departure_time: Some(GtfsTime::from_seconds(3700)),
                    timepoint: Some(Timepoint::Exact),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    timepoint: Some(Timepoint::Exact),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        StopTimeArrivalAndDepartureTimeValidator.validate(&feed, &mut notices);
        TimepointTimeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
