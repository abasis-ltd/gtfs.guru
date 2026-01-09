use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_TRIP_EDGE: &str = "missing_trip_edge";

#[derive(Debug, Default)]
pub struct MissingTripEdgeValidator;

impl Validator for MissingTripEdgeValidator {
    fn name(&self) -> &'static str {
        "missing_trip_edge"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        #[cfg(feature = "parallel")]
        {
            #[cfg(feature = "parallel")]
            let results: Vec<NoticeContainer> = {
                use rayon::prelude::*;
                let ctx = crate::ValidationContextState::capture();
                feed.stop_times_by_trip
                    .par_iter()
                    .map(|(trip_id, stop_time_indices)| {
                        let _guards = ctx.apply();
                        Self::check_trip(feed, *trip_id, stop_time_indices)
                    })
                    .collect()
            };

            for result in results {
                notices.merge(result);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            // Use pre-built index - indices are already sorted by stop_sequence
            for (trip_id, indices) in &feed.stop_times_by_trip {
                let result = Self::check_trip(feed, *trip_id, indices);
                notices.merge(result);
            }
        }
    }
}

impl MissingTripEdgeValidator {
    fn check_trip(
        feed: &GtfsFeed,
        _trip_id: gtfs_guru_model::StringId,
        indices: &[usize],
    ) -> NoticeContainer {
        let mut notices = NoticeContainer::new();
        if indices.is_empty() {
            return notices;
        }
        let first_idx = indices[0];
        let last_idx = indices[indices.len() - 1];
        let first = &feed.stop_times.rows[first_idx];
        let last = &feed.stop_times.rows[last_idx];
        let first_row = feed.stop_times.row_number(first_idx);
        let last_row = feed.stop_times.row_number(last_idx);
        check_trip_edge(first, first_row, &mut notices, feed);
        check_trip_edge(last, last_row, &mut notices, feed);
        notices
    }
}

fn check_trip_edge(
    stop_time: &gtfs_guru_model::StopTime,
    row_number: u64,
    notices: &mut NoticeContainer,
    feed: &GtfsFeed,
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
            feed,
        ));
    }
    if stop_time.departure_time.is_none() {
        notices.push(missing_trip_edge_notice(
            stop_time,
            row_number,
            "departure_time",
            feed,
        ));
    }
}

fn missing_trip_edge_notice(
    stop_time: &gtfs_guru_model::StopTime,
    row_number: u64,
    field: &str,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_TRIP_EDGE,
        NoticeSeverity::Error,
        "missing arrival_time or departure_time at trip edge",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("specifiedField", field);
    notice.insert_context_field("stopSequence", stop_time.stop_sequence);
    notice.insert_context_field("tripId", feed.pool.resolve(stop_time.trip_id).as_str());
    notice.field_order = vec![
        "csvRowNumber".into(),
        "specifiedField".into(),
        "stopSequence".into(),
        "tripId".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, StopTime};

    #[test]
    fn detects_missing_arrival_at_start() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "departure_time".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    arrival_time: None,
                    departure_time: Some(GtfsTime::from_seconds(3600)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

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
                "trip_id".into(),
                "stop_sequence".into(),
                "arrival_time".into(),
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
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: None,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_MISSING_TRIP_EDGE));
    }

    #[test]
    fn passes_valid_trip_edges() {
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
                    arrival_time: Some(GtfsTime::from_seconds(4000)),
                    departure_time: Some(GtfsTime::from_seconds(4100)),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_flex_windows() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        MissingTripEdgeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
