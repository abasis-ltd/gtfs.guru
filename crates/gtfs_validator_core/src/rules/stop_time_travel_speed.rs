use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::RouteType;

const CODE_FAST_TRAVEL_CONSECUTIVE: &str = "fast_travel_between_consecutive_stops";
const CODE_FAST_TRAVEL_FAR: &str = "fast_travel_between_far_stops";
const MAX_DISTANCE_OVER_MAX_SPEED_KM: f64 = 10.0;
const SECONDS_PER_MINUTE: i32 = 60;
const SECONDS_PER_HOUR: f64 = 3600.0;

#[derive(Debug, Default)]
pub struct StopTimeTravelSpeedValidator;

impl Validator for StopTimeTravelSpeedValidator {
    fn name(&self) -> &'static str {
        "stop_time_travel_speed"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        let mut routes_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Route> =
            HashMap::new();
        for route in &feed.routes.rows {
            let route_id = route.route_id;
            if route_id.0 == 0 {
                continue;
            }
            routes_by_id.insert(route_id, route);
        }

        // Build trips_by_id for fast lookup
        let mut trips_by_id: HashMap<gtfs_guru_model::StringId, (usize, &gtfs_guru_model::Trip)> =
            HashMap::new();
        for (trip_index, trip) in feed.trips.rows.iter().enumerate() {
            let trip_id = trip.trip_id;
            if trip_id.0 != 0 {
                trips_by_id.insert(trip_id, (trip_index, trip));
            }
        }

        let context = ValidationContext {
            stops_by_id,
            routes_by_id,
            trips_by_id,
        };

        let mut trips_by_pattern: HashMap<u64, Vec<gtfs_guru_model::StringId>> = HashMap::new();
        for (trip_id, indices) in &feed.stop_times_by_trip {
            let (_, trip) = match context.trips_by_id.get(trip_id) {
                Some(t) => *t,
                None => continue,
            };
            let route_id = trip.route_id;
            if route_id.0 == 0 || !context.routes_by_id.contains_key(&route_id) {
                continue;
            }
            let fingerprint = Self::trip_fingerprint(trip, indices, feed);
            trips_by_pattern
                .entry(fingerprint)
                .or_default()
                .push(*trip_id);
        }

        let pattern_groups: Vec<Vec<gtfs_guru_model::StringId>> =
            trips_by_pattern.into_values().collect();

        #[cfg(feature = "parallel")]
        {
            let results: Vec<NoticeContainer> = {
                use rayon::prelude::*;
                let ctx = crate::ValidationContextState::capture();
                pattern_groups
                    .par_iter()
                    .map(|trip_ids| {
                        let _guards = ctx.apply();
                        Self::check_pattern_group(feed, trip_ids, &context)
                    })
                    .collect()
            };

            for result in results {
                notices.merge(result);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            for trip_ids in &pattern_groups {
                let result = Self::check_pattern_group(feed, trip_ids, &context);
                notices.merge(result);
            }
        }
    }
}

struct ValidationContext<'a> {
    stops_by_id: HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Stop>,
    routes_by_id: HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Route>,
    trips_by_id: HashMap<gtfs_guru_model::StringId, (usize, &'a gtfs_guru_model::Trip)>,
}

// Sync implementation is required for sharing across threads,
// but HashMap with &str keys and &T values is Send + Sync if T is Sync.
// gtfs_guru_model types are Sync.
unsafe impl<'a> Sync for ValidationContext<'a> {}
unsafe impl<'a> Send for ValidationContext<'a> {}

impl StopTimeTravelSpeedValidator {
    fn check_pattern_group(
        feed: &GtfsFeed,
        trip_ids: &[gtfs_guru_model::StringId],
        context: &ValidationContext,
    ) -> NoticeContainer {
        let mut notices = NoticeContainer::new();
        let Some(first_trip_id) = trip_ids.first().copied() else {
            return notices;
        };
        let (_, first_trip) = match context.trips_by_id.get(&first_trip_id) {
            Some(t) => *t,
            None => return notices,
        };
        let first_route_id = first_trip.route_id;
        if first_route_id.0 == 0 {
            return notices;
        }
        let route = match context.routes_by_id.get(&first_route_id) {
            Some(route) => *route,
            None => return notices,
        };
        let max_speed_kph = max_speed_kph(route.route_type);

        let first_indices = match feed.stop_times_by_trip.get(&first_trip_id) {
            Some(indices) => indices,
            None => return notices,
        };
        if first_indices.len() <= 1 {
            return notices;
        }
        let distances_km = match Self::distances_for_pattern(first_indices, feed, context) {
            Some(distances) => distances,
            None => return notices,
        };

        let stop_times: Vec<&gtfs_guru_model::StopTime> = first_indices
            .iter()
            .map(|&i| &feed.stop_times.rows[i])
            .collect();

        let violations = collect_violations(&stop_times, &distances_km, max_speed_kph);

        if violations.is_empty() {
            return notices;
        }

        for trip_id in trip_ids {
            let (trip_index, trip) = match context.trips_by_id.get(trip_id) {
                Some(t) => *t,
                None => continue,
            };
            if trip.route_id != first_route_id {
                continue;
            }
            let indices = match feed.stop_times_by_trip.get(trip_id) {
                Some(indices) => indices,
                None => continue,
            };
            // Ensure indices match the pattern (they should if fingerprint is correct)
            if indices.len() != first_indices.len() {
                continue;
            }

            let trip_row_number = feed.trips.row_number(trip_index);

            for violation in &violations {
                let start_idx = violation.start_idx;
                let end_idx = violation.end_idx;
                let stop_time1 = &feed.stop_times.rows[indices[start_idx]];
                let stop_time2 = &feed.stop_times.rows[indices[end_idx]];
                let row_num1 = feed.stop_times.row_number(indices[start_idx]);
                let row_num2 = feed.stop_times.row_number(indices[end_idx]);

                let Some(stop1) = stop_by_id(&context.stops_by_id, stop_time1.stop_id) else {
                    continue;
                };
                let Some(stop2) = stop_by_id(&context.stops_by_id, stop_time2.stop_id) else {
                    continue;
                };

                let mut notice = match violation.kind {
                    ViolationType::Consecutive => ValidationNotice::new(
                        CODE_FAST_TRAVEL_CONSECUTIVE,
                        NoticeSeverity::Warning,
                        "fast travel between consecutive stops",
                    ),
                    ViolationType::Far => ValidationNotice::new(
                        CODE_FAST_TRAVEL_FAR,
                        NoticeSeverity::Warning,
                        "fast travel between far stops",
                    ),
                };

                populate_travel_speed_notice(
                    &mut notice,
                    trip,
                    trip_row_number,
                    stop_time1,
                    stop_time2,
                    stop1,
                    stop2,
                    violation.speed_kph,
                    violation.distance_km,
                    row_num1,
                    row_num2,
                    feed,
                );
                notices.push(notice);
            }
        }
        notices
    }

    fn trip_fingerprint(trip: &gtfs_guru_model::Trip, indices: &[usize], feed: &GtfsFeed) -> u64 {
        let mut hasher = DefaultHasher::new();
        trip.route_id.hash(&mut hasher);
        indices.len().hash(&mut hasher);
        for &index in indices {
            let stop_time = &feed.stop_times.rows[index];
            stop_time.stop_id.hash(&mut hasher);
            stop_time
                .arrival_time
                .map(|time| time.total_seconds())
                .unwrap_or(-1)
                .hash(&mut hasher);
            stop_time
                .departure_time
                .map(|time| time.total_seconds())
                .unwrap_or(-1)
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    fn distances_for_pattern(
        indices: &[usize],
        feed: &GtfsFeed,
        context: &ValidationContext,
    ) -> Option<Vec<f64>> {
        if indices.len() <= 1 {
            return Some(Vec::new());
        }
        let mut coords = Vec::with_capacity(indices.len());
        for &index in indices {
            let stop_time = &feed.stop_times.rows[index];
            let coords_for_stop = stop_coords(stop_time, &context.stops_by_id)?;
            coords.push(coords_for_stop);
        }
        let mut distances_km = Vec::with_capacity(coords.len() - 1);
        for i in 0..coords.len() - 1 {
            let (lat1, lon1) = coords[i];
            let (lat2, lon2) = coords[i + 1];
            distances_km.push(haversine_km(lat1, lon1, lat2, lon2));
        }
        Some(distances_km)
    }
}

enum ViolationType {
    Consecutive,
    Far,
}

struct SpeedViolation {
    kind: ViolationType,
    start_idx: usize,
    end_idx: usize,
    speed_kph: f64,
    distance_km: f64,
}

fn collect_violations(
    stop_times: &[&gtfs_guru_model::StopTime],
    distances_km: &[f64],
    max_speed_kph: f64,
) -> Vec<SpeedViolation> {
    let mut violations = Vec::new();

    // Check consecutive stops
    for i in 0..distances_km.len() {
        let first = stop_times[i];
        let second = stop_times[i + 1];
        let (Some(departure), Some(arrival)) = (first.departure_time, second.arrival_time) else {
            continue;
        };
        let speed_kph = speed_kph(
            distances_km[i],
            departure.total_seconds(),
            arrival.total_seconds(),
        );
        if speed_kph > max_speed_kph {
            violations.push(SpeedViolation {
                kind: ViolationType::Consecutive,
                start_idx: i,
                end_idx: i + 1,
                speed_kph,
                distance_km: distances_km[i],
            });
        }
    }

    // Check far stops
    for end_idx in 0..stop_times.len() {
        let end = stop_times[end_idx];
        let Some(arrival) = end.arrival_time else {
            continue;
        };
        let mut distance_to_end = 0.0;
        for start_idx in (0..end_idx).rev() {
            let start = stop_times[start_idx];
            distance_to_end += distances_km[start_idx];
            let Some(departure) = start.departure_time else {
                continue;
            };
            let speed_kph = speed_kph(
                distance_to_end,
                departure.total_seconds(),
                arrival.total_seconds(),
            );
            if speed_kph > max_speed_kph && distance_to_end > MAX_DISTANCE_OVER_MAX_SPEED_KM {
                violations.push(SpeedViolation {
                    kind: ViolationType::Far,
                    start_idx,
                    end_idx,
                    speed_kph,
                    distance_km: distance_to_end,
                });
                // Optimization: if we found a far warning ending at 'end', we might not need to check other starts?
                // The original code had `return;` inside the inner loop, breaking the inner loop for this 'end_idx'.
                // Yes, "return" in original `validate_far_stops` inside the inner loop breaks the inner loop (function return? No, validate_far_stops returns void).
                // Wait, original has `return;` inside the loop. It returns from the FUNCTION `validate_far_stops`.
                // That means for a given trip, it reports at most ONE "far" notice and then stops checking ENTIRELY?
                // No, it's `validate_far_stops`. If it returns, it stops checking ANY more far stops for that trip.
                // THAT seems like a bug or a specific design choice to avoid spam.
                // "limit to one far notice per trip" makes sense to avoid thousands of notices.
                // Let's replicate that behavior.
                break;
            }
        }
        // If we broke out of the inner loop due to finding a violation, we should probably stop the outer loop too if the original behavior was "return".
        // Original: `return;` (line 348) -> Returns from `validate_far_stops`.
        // So yes, one violation per trip.
        // So if `violations` has a Far violation, we should stop?
        // But `collect_violations` does consecutive too.
        // Actually, `validate_far_stops` and `validate_consecutive` are separate calls in original check_pattern_group.
        // So we can have consecutive notices AND one far notice.
        // So here: if we find a violation in inner loop, break inner loop.
        // But wait, if we break inner loop, we just go to next end_idx.
        // The original code returned from the whole function, skipping remaining `end_idx` check.
        // So we should break the outer loop too?
        // Let's check line 348: `return;`
        // Yes.
        if !violations.is_empty() && matches!(violations.last().unwrap().kind, ViolationType::Far) {
            // If we just added a Far violation, stop scanning for more Far violations.
            break;
        }
    }

    violations
}

fn populate_travel_speed_notice(
    notice: &mut ValidationNotice,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_guru_model::StopTime,
    stop_time2: &gtfs_guru_model::StopTime,
    stop1: &gtfs_guru_model::Stop,
    stop2: &gtfs_guru_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    row_num1: u64,
    row_num2: u64,
    feed: &GtfsFeed,
) {
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
    notice.insert_context_field("routeId", feed.pool.resolve(trip.route_id).as_str());
    notice.insert_context_field("speedKph", speed_kph);
    notice.insert_context_field("distanceKm", distance_km);
    notice.insert_context_field("csvRowNumber1", row_num1);
    notice.insert_context_field("stopSequence1", stop_time1.stop_sequence);
    notice.insert_context_field("stopId1", feed.pool.resolve(stop_time1.stop_id).as_str());
    notice.insert_context_field("stopName1", stop1.stop_name.as_deref().unwrap_or(""));
    if let Some(departure) = stop_time1.departure_time {
        notice.insert_context_field("departureTime1", departure);
    }
    notice.insert_context_field("csvRowNumber2", row_num2);
    notice.insert_context_field("stopSequence2", stop_time2.stop_sequence);
    notice.insert_context_field("stopId2", feed.pool.resolve(stop_time2.stop_id).as_str());
    notice.insert_context_field("stopName2", stop2.stop_name.as_deref().unwrap_or(""));
    if let Some(arrival) = stop_time2.arrival_time {
        notice.insert_context_field("arrivalTime2", arrival);
    }
    notice.field_order = vec![
        "tripCsvRowNumber".into(),
        "tripId".into(),
        "routeId".into(),
        "speedKph".into(),
        "distanceKm".into(),
        "csvRowNumber1".into(),
        "stopSequence1".into(),
        "stopId1".into(),
        "stopName1".into(),
        "departureTime1".into(),
        "csvRowNumber2".into(),
        "stopSequence2".into(),
        "stopId2".into(),
        "stopName2".into(),
        "arrivalTime2".into(),
    ];
}

fn stop_coords(
    stop_time: &gtfs_guru_model::StopTime,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
) -> Option<(f64, f64)> {
    let mut current_id = stop_time.stop_id;
    if current_id.0 == 0 {
        return None;
    }
    for _ in 0..3 {
        let stop = match stops_by_id.get(&current_id) {
            Some(stop) => *stop,
            None => break,
        };
        if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
            return Some((lat, lon));
        }
        let Some(parent) = stop.parent_station.filter(|id| id.0 != 0) else {
            break;
        };
        current_id = parent;
    }
    None
}

fn stop_by_id<'a>(
    stops_by_id: &'a HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_id: gtfs_guru_model::StringId,
) -> Option<&'a gtfs_guru_model::Stop> {
    if stop_id.0 == 0 {
        return None;
    }
    stops_by_id.get(&stop_id).copied()
}

fn speed_kph(distance_km: f64, departure_sec: i32, arrival_sec: i32) -> f64 {
    let time_sec = time_between_stops(arrival_sec, departure_sec) as f64;
    distance_km * SECONDS_PER_HOUR / time_sec
}

fn time_between_stops(arrival_sec: i32, departure_sec: i32) -> i32 {
    let mut delta = arrival_sec - departure_sec;
    if delta <= 0 {
        return SECONDS_PER_MINUTE;
    }
    if arrival_sec % SECONDS_PER_MINUTE == 0 && departure_sec % SECONDS_PER_MINUTE == 0 {
        delta += SECONDS_PER_MINUTE;
    }
    delta
}

fn max_speed_kph(route_type: RouteType) -> f64 {
    match route_type {
        RouteType::Tram => 100.0,
        RouteType::Rail => 500.0,
        RouteType::Subway | RouteType::Monorail | RouteType::Bus | RouteType::Trolleybus => 150.0,
        RouteType::Ferry => 80.0,
        RouteType::CableCar => 30.0,
        RouteType::Gondola | RouteType::Funicular => 50.0,
        RouteType::Extended(_) | RouteType::Unknown => 200.0,
    }
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_km = 6371.0;
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    radius_km * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, Route, RouteType, Stop, StopTime, Trip};

    #[test]
    fn detects_fast_travel_consecutive() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(0.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(0.1),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_type".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "departure_time".into(),
                "arrival_time".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    departure_time: Some(GtfsTime::from_seconds(0)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(120)), // 2 minutes, very fast
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        StopTimeTravelSpeedValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_FAST_TRAVEL_CONSECUTIVE));
    }

    #[test]
    fn detects_fast_travel_far() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(0.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(0.05),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S3"),
                    stop_lat: Some(0.1),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_type".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "departure_time".into(),
                "arrival_time".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    departure_time: Some(GtfsTime::from_seconds(0)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(300)),
                    departure_time: Some(GtfsTime::from_seconds(300)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S3"),
                    stop_sequence: 3,
                    arrival_time: Some(GtfsTime::from_seconds(200)), // < 266s is fast (> 150 kph) for 11.1km
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        StopTimeTravelSpeedValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_FAST_TRAVEL_FAR));
    }

    #[test]
    fn passes_normal_speed() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(0.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(0.1),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_type".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "departure_time".into(),
                "arrival_time".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    departure_time: Some(GtfsTime::from_seconds(0)),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    arrival_time: Some(GtfsTime::from_seconds(600)), // 10 minutes, normal speed
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        StopTimeTravelSpeedValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
