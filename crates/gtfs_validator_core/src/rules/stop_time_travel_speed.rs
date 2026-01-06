use std::collections::HashMap;

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

        #[cfg(feature = "parallel")]
        {
            let results: Vec<NoticeContainer> = {
                use rayon::prelude::*;
                let ctx = crate::ValidationContextState::capture();
                feed.stop_times_by_trip
                    .par_iter()
                    .map(|(trip_id, indices)| {
                        let _guards = ctx.apply();
                        Self::check_trip(feed, *trip_id, indices, &context)
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
                let result = Self::check_trip(feed, *trip_id, indices, &context);
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
    fn check_trip(
        feed: &GtfsFeed,
        trip_id: gtfs_guru_model::StringId,
        indices: &[usize],
        context: &ValidationContext,
    ) -> NoticeContainer {
        let mut notices = NoticeContainer::new();
        if indices.len() <= 1 {
            return notices;
        }
        let (trip_index, trip) = match context.trips_by_id.get(&trip_id) {
            Some(t) => *t,
            None => return notices,
        };
        let route_id = trip.route_id;
        if route_id.0 == 0 {
            return notices;
        }
        let route = match context.routes_by_id.get(&route_id) {
            Some(route) => *route,
            None => return notices,
        };
        let max_speed_kph = max_speed_kph(route.route_type);
        let trip_row_number = feed.trips.row_number(trip_index);

        // Collect stop_times as references for validation functions
        let stop_times: Vec<&gtfs_guru_model::StopTime> =
            indices.iter().map(|&i| &feed.stop_times.rows[i]).collect();

        // Build row number lookup
        let stop_time_rows: HashMap<*const gtfs_guru_model::StopTime, u64> = indices
            .iter()
            .map(|&i| {
                (
                    &feed.stop_times.rows[i] as *const _,
                    feed.stop_times.row_number(i),
                )
            })
            .collect();

        let mut distances_km = Vec::with_capacity(stop_times.len() - 1);
        let mut coords = Vec::with_capacity(stop_times.len());
        let mut missing_coords = false;
        for stop_time in stop_times.iter() {
            match stop_coords(stop_time, &context.stops_by_id) {
                Some(coords_for_stop) => coords.push(coords_for_stop),
                None => {
                    missing_coords = true;
                    break;
                }
            }
        }
        if missing_coords {
            return notices;
        }
        for i in 0..coords.len() - 1 {
            let (lat1, lon1) = coords[i];
            let (lat2, lon2) = coords[i + 1];
            distances_km.push(haversine_km(lat1, lon1, lat2, lon2));
        }

        validate_consecutive_stops(
            &mut notices,
            trip,
            trip_row_number,
            &stop_times,
            &distances_km,
            max_speed_kph,
            &context.stops_by_id,
            &stop_time_rows,
            feed,
        );
        validate_far_stops(
            &mut notices,
            trip,
            trip_row_number,
            &stop_times,
            &distances_km,
            max_speed_kph,
            &context.stops_by_id,
            &stop_time_rows,
            feed,
        );
        notices
    }
}

fn validate_consecutive_stops(
    notices: &mut NoticeContainer,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    stop_times: &[&gtfs_guru_model::StopTime],
    distances_km: &[f64],
    max_speed_kph: f64,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
    feed: &GtfsFeed,
) {
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
            let Some(stop1) = stop_by_id(stops_by_id, first.stop_id) else {
                return;
            };
            let Some(stop2) = stop_by_id(stops_by_id, second.stop_id) else {
                return;
            };
            notices.push(fast_travel_between_consecutive_notice(
                trip,
                trip_row_number,
                first,
                second,
                stop1,
                stop2,
                speed_kph,
                distances_km[i],
                stop_time_rows,
             feed));
        }
    }
}

fn validate_far_stops(
    notices: &mut NoticeContainer,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    stop_times: &[&gtfs_guru_model::StopTime],
    distances_km: &[f64],
    max_speed_kph: f64,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
    feed: &GtfsFeed,
) {
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
                let Some(stop1) = stop_by_id(stops_by_id, start.stop_id) else {
                    return;
                };
                let Some(stop2) = stop_by_id(stops_by_id, end.stop_id) else {
                    return;
                };
                notices.push(fast_travel_between_far_notice(
                    trip,
                    trip_row_number,
                    start,
                    end,
                    stop1,
                    stop2,
                    speed_kph,
                    distance_to_end,
                    stop_time_rows,
                    feed,
                ));
                return;
            }
        }
    }
}

fn fast_travel_between_consecutive_notice(
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_guru_model::StopTime,
    stop_time2: &gtfs_guru_model::StopTime,
    stop1: &gtfs_guru_model::Stop,
    stop2: &gtfs_guru_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
 feed: &GtfsFeed) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FAST_TRAVEL_CONSECUTIVE,
        NoticeSeverity::Warning,
        "fast travel between consecutive stops",
    );
    populate_travel_speed_notice(
        &mut notice,
        trip,
        trip_row_number,
        stop_time1,
        stop_time2,
        stop1,
        stop2,
        speed_kph,
        distance_km,
        stop_time_rows,
     feed);
    notice
}

fn fast_travel_between_far_notice(
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_guru_model::StopTime,
    stop_time2: &gtfs_guru_model::StopTime,
    stop1: &gtfs_guru_model::Stop,
    stop2: &gtfs_guru_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
 feed: &GtfsFeed) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FAST_TRAVEL_FAR,
        NoticeSeverity::Warning,
        "fast travel between far stops",
    );
    populate_travel_speed_notice(
        &mut notice,
        trip,
        trip_row_number,
        stop_time1,
        stop_time2,
        stop1,
        stop2,
        speed_kph,
        distance_km,
        stop_time_rows,
     feed);
    notice
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
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
 feed: &GtfsFeed) {
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
    notice.insert_context_field("routeId", feed.pool.resolve(trip.route_id).as_str());
    notice.insert_context_field("speedKph", speed_kph);
    notice.insert_context_field("distanceKm", distance_km);
    notice.insert_context_field("csvRowNumber1", stop_time_row(stop_time_rows, stop_time1));
    notice.insert_context_field("stopSequence1", stop_time1.stop_sequence);
    notice.insert_context_field("stopId1", feed.pool.resolve(stop_time1.stop_id).as_str());
    notice.insert_context_field("stopName1", stop1.stop_name.as_deref().unwrap_or(""));
    if let Some(departure) = stop_time1.departure_time {
        notice.insert_context_field("departureTime1", departure);
    }
    notice.insert_context_field("csvRowNumber2", stop_time_row(stop_time_rows, stop_time2));
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

fn stop_time_row(
    stop_time_rows: &HashMap<*const gtfs_guru_model::StopTime, u64>,
    stop_time: &gtfs_guru_model::StopTime,
) -> u64 {
    stop_time_rows
        .get(&(stop_time as *const _))
        .copied()
        .unwrap_or(2)
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
