use std::collections::HashMap;

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

        let mut by_trip: HashMap<&str, Vec<(u64, &gtfs_guru_model::StopTime)>> = HashMap::new();
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
            let mut prev: Option<(u64, &gtfs_guru_model::StopTime)> = None;
            for (row_number, curr) in stop_times {
                if !has_stop_id(curr)
                    || curr.location_group_id.is_some()
                    || curr.location_id.is_some()
                {
                    continue;
                }

                if let Some((prev_row_number, prev)) = prev {
                    if let (Some(prev_dist), Some(curr_dist)) =
                        (prev.shape_dist_traveled, curr.shape_dist_traveled)
                    {
                        if prev_dist >= curr_dist {
                            let mut notice = ValidationNotice::new(
                                CODE_DECREASING_OR_EQUAL_STOP_TIME_DISTANCE,
                                NoticeSeverity::Error,
                                "shape_dist_traveled must increase between stop_times",
                            );
                            notice.insert_context_field("csvRowNumber", *row_number);
                            notice.insert_context_field("prevCsvRowNumber", prev_row_number);
                            notice.insert_context_field("prevShapeDistTraveled", prev_dist);
                            notice.insert_context_field("prevStopSequence", prev.stop_sequence);
                            notice.insert_context_field("shapeDistTraveled", curr_dist);
                            notice.insert_context_field("stopId", curr.stop_id.as_str());
                            notice.insert_context_field("stopSequence", curr.stop_sequence);
                            notice.insert_context_field("tripId", curr.trip_id.as_str());
                            notice.field_order = vec![
                                "csvRowNumber".to_string(),
                                "prevCsvRowNumber".to_string(),
                                "prevShapeDistTraveled".to_string(),
                                "prevStopSequence".to_string(),
                                "shapeDistTraveled".to_string(),
                                "stopId".to_string(),
                                "stopSequence".to_string(),
                                "tripId".to_string(),
                            ];
                            notices.push(notice);
                        }
                    }
                }

                prev = Some((*row_number, *curr));
            }
        }
    }
}

fn has_stop_id(stop_time: &gtfs_guru_model::StopTime) -> bool {
    !stop_time.stop_id.trim().is_empty()
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
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S1".to_string(),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(5.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
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
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S1".to_string(),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
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
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S1".to_string(),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(15.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_without_shape_dist_traveled_header() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
            ],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S1".to_string(),
                    stop_sequence: 1,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
                    stop_sequence: 2,
                    shape_dist_traveled: Some(5.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        StopTimeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
