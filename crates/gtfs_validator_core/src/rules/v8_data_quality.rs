use std::collections::HashMap;

use crate::feed::STOP_TIMES_FILE;
use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{ServiceAvailability, StringId};

#[derive(Debug, Default)]
pub struct ServiceHasNoActiveDayOfTheWeekValidator;

impl Validator for ServiceHasNoActiveDayOfTheWeekValidator {
    fn name(&self) -> &'static str {
        "service_has_no_active_day_of_the_week"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(calendar) = &feed.calendar else {
            return;
        };
        for (index, service) in calendar.rows.iter().enumerate() {
            if service.service_id.0 == 0
                || ![
                    service.monday,
                    service.tuesday,
                    service.wednesday,
                    service.thursday,
                    service.friday,
                    service.saturday,
                    service.sunday,
                ]
                .iter()
                .all(|day| *day == ServiceAvailability::Unavailable)
            {
                continue;
            }
            let mut notice = ValidationNotice::new(
                "service_has_no_active_day_of_the_week",
                NoticeSeverity::Warning,
                "service is not active on any day of the week",
            );
            notice.insert_context_field("csvRowNumber", calendar.row_number(index));
            notice
                .insert_context_field("serviceId", feed.pool.resolve(service.service_id).as_str());
            notice.field_order = vec!["csvRowNumber".into(), "serviceId".into()];
            notices.push(notice);
        }
    }
}

#[derive(Debug, Default)]
pub struct UnsortedStopTimesValidator;

impl Validator for UnsortedStopTimesValidator {
    fn name(&self) -> &'static str {
        "unsorted_stop_times"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Invalid rows are omitted from the typed table. Treating those gaps as
        // physical disorder creates a false `unsorted_stop_times` notice and a
        // sort cannot repair the underlying parse error.
        if feed.table_has_errors(STOP_TIMES_FILE) {
            return;
        }

        #[derive(Default)]
        struct Stats {
            count: u64,
            min_row: u64,
            max_row: u64,
            last_sequence: Option<u32>,
            unsorted: bool,
        }

        fn merge_group(
            stats_by_trip: &mut HashMap<StringId, Stats>,
            trip_id: StringId,
            group: Stats,
        ) {
            if let Some(existing) = stats_by_trip.get_mut(&trip_id) {
                existing.count += group.count;
                existing.min_row = existing.min_row.min(group.min_row);
                existing.max_row = existing.max_row.max(group.max_row);
                existing.unsorted = true;
            } else {
                stats_by_trip.insert(trip_id, group);
            }
        }

        let mut stats_by_trip = HashMap::new();
        let mut current_trip = StringId(0);
        let mut current_stats = Stats::default();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            if stop_time.trip_id.0 == 0 {
                if current_trip.0 != 0 {
                    merge_group(&mut stats_by_trip, current_trip, current_stats);
                    current_trip = StringId(0);
                    current_stats = Stats::default();
                }
                continue;
            }
            if stop_time.trip_id != current_trip {
                if current_trip.0 != 0 {
                    merge_group(&mut stats_by_trip, current_trip, current_stats);
                }
                current_trip = stop_time.trip_id;
                current_stats = Stats::default();
            }
            let row = feed.stop_times.row_number(index);
            current_stats.count += 1;
            if current_stats.count == 1 {
                current_stats.min_row = row;
                current_stats.max_row = row;
            } else {
                current_stats.min_row = current_stats.min_row.min(row);
                current_stats.max_row = current_stats.max_row.max(row);
            }
            if current_stats
                .last_sequence
                .is_some_and(|previous| stop_time.stop_sequence <= previous)
            {
                current_stats.unsorted = true;
            }
            current_stats.last_sequence = Some(stop_time.stop_sequence);
        }
        if current_trip.0 != 0 {
            merge_group(&mut stats_by_trip, current_trip, current_stats);
        }
        for (trip_id, stats) in stats_by_trip {
            let span = stats.max_row.saturating_sub(stats.min_row) + 1;
            if !stats.unsorted && span <= stats.count {
                continue;
            }
            let mut notice = ValidationNotice::new(
                "unsorted_stop_times",
                NoticeSeverity::Info,
                "stop times for a trip are unsorted or non-contiguous in the file",
            );
            notice.insert_context_field("tripId", feed.pool.resolve(trip_id).as_str());
            notice.insert_context_field("startCsvRowNumber", stats.min_row);
            notice.insert_context_field("endCsvRowNumber", stats.max_row);
            notice.field_order = vec![
                "tripId".into(),
                "startCsvRowNumber".into(),
                "endCsvRowNumber".into(),
            ];
            notices.push(notice);
        }
    }
}

#[derive(Debug, Default)]
pub struct TripHeadsignMatchesIntermediateStopValidator;

impl Validator for TripHeadsignMatchesIntermediateStopValidator {
    fn name(&self) -> &'static str {
        "trip_headsign_matches_intermediate_stop"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // The canonical validator (v8.0.1 TripHeadsignValidator) returns from
        // the whole check on the first circular trip instead of skipping just
        // that trip, so it stops reporting after one. Default runs reproduce
        // that for output parity; `--thorough` checks every trip.
        let scan_every_trip = thorough_mode_enabled();
        let stops: HashMap<_, _> = feed
            .stops
            .rows
            .iter()
            .map(|stop| (stop.stop_id, stop))
            .collect();
        for (trip_index, trip) in feed.trips.rows.iter().enumerate() {
            let Some(headsign) = trip
                .trip_headsign
                .as_deref()
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(stop_time_indices) = feed.stop_times_by_trip.get(&trip.trip_id) else {
                continue;
            };
            if stop_time_indices.len() < 2 {
                continue;
            }
            let first = &feed.stop_times.rows[stop_time_indices[0]];
            let last = &feed.stop_times.rows[*stop_time_indices.last().unwrap()];
            if first.stop_id == last.stop_id {
                if scan_every_trip {
                    continue;
                }
                return;
            }
            for &stop_time_index in &stop_time_indices[..stop_time_indices.len() - 1] {
                let stop_time = &feed.stop_times.rows[stop_time_index];
                let Some(stop_name) = stops
                    .get(&stop_time.stop_id)
                    .and_then(|stop| stop.stop_name.as_deref())
                else {
                    continue;
                };
                if !stop_name.eq_ignore_ascii_case(headsign) {
                    continue;
                }
                let mut notice = ValidationNotice::new(
                    "trip_headsign_matches_intermediate_stop",
                    NoticeSeverity::Info,
                    "trip_headsign matches an intermediate stop rather than the final stop",
                );
                notice.insert_context_field("csvRowNumber", feed.trips.row_number(trip_index));
                notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
                notice.insert_context_field("tripHeadsign", headsign);
                notice
                    .insert_context_field("stopId1", feed.pool.resolve(stop_time.stop_id).as_str());
                notice.insert_context_field("stopSequence", stop_time.stop_sequence);
                notice.insert_context_field("stopId2", feed.pool.resolve(last.stop_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "tripId".into(),
                    "tripHeadsign".into(),
                    "stopId1".into(),
                    "stopSequence".into(),
                    "stopId2".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct TripWithShapeDistTraveledButNoShapeDistancesValidator;

impl Validator for TripWithShapeDistTraveledButNoShapeDistancesValidator {
    fn name(&self) -> &'static str {
        "trip_with_shape_dist_traveled_but_no_shape_distances"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("shape_dist_traveled"))
        {
            return;
        }
        let Some(shapes) = &feed.shapes else {
            return;
        };
        let mut shape_completeness: HashMap<_, (usize, usize)> = HashMap::new();
        for shape in &shapes.rows {
            let entry = shape_completeness.entry(shape.shape_id).or_default();
            entry.0 += 1;
            if shape.shape_dist_traveled.is_some() {
                entry.1 += 1;
            }
        }
        let trips: HashMap<_, _> = feed
            .trips
            .rows
            .iter()
            .enumerate()
            .map(|(index, trip)| (trip.trip_id, (index, trip)))
            .collect();
        let mut first_distance_by_trip = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            if stop_time.shape_dist_traveled.is_some() {
                first_distance_by_trip
                    .entry(stop_time.trip_id)
                    .or_insert(index);
            }
        }
        for (trip_id, first_with_distance) in first_distance_by_trip {
            let Some((trip_index, trip)) = trips.get(&trip_id).copied() else {
                continue;
            };
            let Some(shape_id) = trip.shape_id.filter(|id| id.0 != 0) else {
                continue;
            };
            let Some((total, with_distance)) = shape_completeness.get(&shape_id).copied() else {
                continue;
            };
            if total == with_distance {
                continue;
            }
            let mut notice = ValidationNotice::new(
                "trip_with_shape_dist_traveled_but_no_shape_distances",
                NoticeSeverity::Info,
                "stop times contain shape distances but the referenced shape is incomplete",
            );
            notice.insert_context_field("tripCsvRowNumber", feed.trips.row_number(trip_index));
            notice.insert_context_field("tripId", feed.pool.resolve(trip_id).as_str());
            notice.insert_context_field("shapeId", feed.pool.resolve(shape_id).as_str());
            notice.insert_context_field(
                "stopTimeCsvRowNumber",
                feed.stop_times.row_number(first_with_distance),
            );
            notice.field_order = vec![
                "tripCsvRowNumber".into(),
                "tripId".into(),
                "shapeId".into(),
                "stopTimeCsvRowNumber".into(),
            ];
            notices.push(notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Calendar, GtfsDate, Shape, Stop, StopTime, Trip};

    #[test]
    fn detects_service_without_active_weekday() {
        let mut feed = GtfsFeed::default();
        feed.calendar = Some(CsvTable {
            headers: vec!["service_id".into()],
            rows: vec![Calendar {
                service_id: feed.pool.intern("SVC"),
                start_date: GtfsDate::parse("20260101").unwrap(),
                end_date: GtfsDate::parse("20260131").unwrap(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        let mut notices = NoticeContainer::new();
        ServiceHasNoActiveDayOfTheWeekValidator.validate(&feed, &mut notices);
        assert_eq!(
            notices.iter().next().unwrap().code,
            "service_has_no_active_day_of_the_week"
        );
    }

    #[test]
    fn detects_unsorted_stop_times() {
        let mut feed = GtfsFeed::default();
        let trip_id = feed.pool.intern("T1");
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id,
                    stop_sequence: 2,
                    ..Default::default()
                },
                StopTime {
                    trip_id,
                    stop_sequence: 1,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        let mut notices = NoticeContainer::new();
        UnsortedStopTimesValidator.validate(&feed, &mut notices);
        assert_eq!(notices.iter().next().unwrap().code, "unsorted_stop_times");
    }

    #[test]
    fn detects_non_contiguous_stop_times_without_per_row_hashing() {
        let mut feed = GtfsFeed::default();
        let trip_1 = feed.pool.intern("T1");
        let trip_2 = feed.pool.intern("T2");
        feed.stop_times.rows = vec![
            StopTime {
                trip_id: trip_1,
                stop_sequence: 1,
                ..Default::default()
            },
            StopTime {
                trip_id: trip_2,
                stop_sequence: 1,
                ..Default::default()
            },
            StopTime {
                trip_id: trip_1,
                stop_sequence: 2,
                ..Default::default()
            },
        ];
        feed.stop_times.row_numbers = vec![2, 3, 4];

        let mut notices = NoticeContainer::new();
        UnsortedStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "unsorted_stop_times");
    }

    #[test]
    fn detects_headsign_and_incomplete_shape_distances() {
        let mut feed = GtfsFeed::default();
        let trip_id = feed.pool.intern("T1");
        let shape_id = feed.pool.intern("SH1");
        let stop_1 = feed.pool.intern("S1");
        let stop_2 = feed.pool.intern("S2");
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "trip_headsign".into(), "shape_id".into()],
            rows: vec![Trip {
                trip_id,
                trip_headsign: Some("Downtown".into()),
                shape_id: Some(shape_id),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops.rows = vec![
            Stop {
                stop_id: stop_1,
                stop_name: Some("Downtown".into()),
                ..Default::default()
            },
            Stop {
                stop_id: stop_2,
                stop_name: Some("Terminus".into()),
                ..Default::default()
            },
        ];
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id,
                    stop_id: stop_1,
                    stop_sequence: 1,
                    shape_dist_traveled: Some(0.0),
                    ..Default::default()
                },
                StopTime {
                    trip_id,
                    stop_id: stop_2,
                    stop_sequence: 2,
                    shape_dist_traveled: Some(10.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into(), "shape_dist_traveled".into()],
            rows: vec![Shape {
                shape_id,
                shape_pt_sequence: 1,
                shape_dist_traveled: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        TripHeadsignMatchesIntermediateStopValidator.validate(&feed, &mut notices);
        TripWithShapeDistTraveledButNoShapeDistancesValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == "trip_headsign_matches_intermediate_stop"));
        assert!(notices.iter().any(|notice| {
            notice.code == "trip_with_shape_dist_traveled_but_no_shape_distances"
        }));
    }
}
