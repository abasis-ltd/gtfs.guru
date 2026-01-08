use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_DECREASING_OR_EQUAL_STOP_TIME_DISTANCE: &str = "decreasing_or_equal_stop_time_distance";

#[derive(Debug, Default)]
pub struct StopTimeIncreasingDistanceValidator;

impl Validator for StopTimeIncreasingDistanceValidator {
    fn name(&self) -> &'static str {
        "stop_time_increasing_distance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let headers = &feed.stop_times.headers;
        if !headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("stop_id"))
            || !headers
                .iter()
                .any(|header| header.eq_ignore_ascii_case("shape_dist_traveled"))
        {
            return;
        }

        // We can leverage `feed.stop_times_by_trip` but we need `stop_times` objects loaded.
        // `StopTimes` are loaded.
        // The original code groups by trip manually:
        // `let mut by_trip: HashMap<&str, Vec<(u64, &gtfs_guru_model::StopTime)>> = HashMap::new();`
        // We can skip this manual grouping if we use `feed.stop_times_by_trip`.
        // However, `feed.stop_times_by_trip` doesn't store row numbers directly (we can compute them).
        // And it stores indices.

        // Let's use `feed.stop_times_by_trip` which is efficient.

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

                        check_trip(*trip_id, stop_time_indices, feed)
                    })
                    .collect()
            };

            for result in results {
                notices.merge(result);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            for (trip_id, indices) in &feed.stop_times_by_trip {
                let result = check_trip(*trip_id, indices, feed);
                notices.merge(result);
            }
        }
    }
}

fn check_trip(
    trip_id: gtfs_guru_model::StringId,
    indices: &[usize],
    feed: &GtfsFeed,
) -> NoticeContainer {
    let mut notices = NoticeContainer::new();
    let mut prev: Option<(u64, &gtfs_guru_model::StopTime)> = None;

    // Indicies are sorted by stop_sequence
    for &index in indices {
        let curr = &feed.stop_times.rows[index];
        let row_number = feed.stop_times.row_number(index);

        if !has_stop_id(curr, feed)
            || curr.location_group_id.is_some()
            || curr.location_id.is_some()
        {
            continue;
        }

        if let Some((prev_row_number, prev_stop_time)) = prev {
            if let (Some(prev_dist), Some(curr_dist)) =
                (prev_stop_time.shape_dist_traveled, curr.shape_dist_traveled)
            {
                if prev_dist >= curr_dist {
                    let mut notice = ValidationNotice::new(
                        CODE_DECREASING_OR_EQUAL_STOP_TIME_DISTANCE,
                        NoticeSeverity::Error,
                        "shape_dist_traveled must increase between stop_times",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("prevCsvRowNumber", prev_row_number);
                    notice.insert_context_field("prevShapeDistTraveled", prev_dist);
                    notice.insert_context_field("prevStopSequence", prev_stop_time.stop_sequence);
                    notice.insert_context_field("shapeDistTraveled", curr_dist);
                    notice.insert_context_field("stopId", feed.pool.resolve(curr.stop_id).as_str());
                    notice.insert_context_field("stopSequence", curr.stop_sequence);
                    notice.insert_context_field("tripId", feed.pool.resolve(trip_id).as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "prevCsvRowNumber".into(),
                        "prevShapeDistTraveled".into(),
                        "prevStopSequence".into(),
                        "shapeDistTraveled".into(),
                        "stopId".into(),
                        "stopSequence".into(),
                        "tripId".into(),
                    ];
                    notices.push(notice);
                }
            }
        }

        prev = Some((row_number, curr));
    }
    notices
}

fn has_stop_id(stop_time: &gtfs_guru_model::StopTime, _feed: &GtfsFeed) -> bool {
    stop_time.stop_id.0 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::StopTime;

    #[test]
    fn detects_decreasing_distance() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(5.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_DECREASING_OR_EQUAL_STOP_TIME_DISTANCE
        );
    }

    #[test]
    fn detects_equal_distance() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_DECREASING_OR_EQUAL_STOP_TIME_DISTANCE
        );
    }

    #[test]
    fn passes_increasing_distance() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(15.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_without_shape_dist_traveled_header() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(5.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
