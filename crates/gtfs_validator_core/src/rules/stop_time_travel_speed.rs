use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::RouteType;

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
        let mut stops_by_id: HashMap<&str, &gtfs_model::Stop> = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        let mut routes_by_id: HashMap<&str, &gtfs_model::Route> = HashMap::new();
        for route in &feed.routes.rows {
            let route_id = route.route_id.trim();
            if route_id.is_empty() {
                continue;
            }
            routes_by_id.insert(route_id, route);
        }

        let mut stop_times_by_trip: HashMap<&str, Vec<&gtfs_model::StopTime>> = HashMap::new();
        let mut stop_time_rows: HashMap<*const gtfs_model::StopTime, u64> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            stop_time_rows.insert(stop_time as *const _, feed.stop_times.row_number(index));
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .push(stop_time);
        }
        for stop_times in stop_times_by_trip.values_mut() {
            stop_times.sort_by_key(|stop_time| stop_time.stop_sequence);
        }

        for (trip_index, trip) in feed.trips.rows.iter().enumerate() {
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let stop_times = match stop_times_by_trip.get(trip_id) {
                Some(times) if times.len() > 1 => times,
                _ => continue,
            };
            let route_id = trip.route_id.trim();
            if route_id.is_empty() {
                continue;
            }
            let route = match routes_by_id.get(route_id) {
                Some(route) => *route,
                None => continue,
            };
            let max_speed_kph = max_speed_kph(route.route_type);
            let trip_row_number = feed.trips.row_number(trip_index);

            let mut distances_km = Vec::with_capacity(stop_times.len() - 1);
            let mut coords = Vec::with_capacity(stop_times.len());
            let mut missing_coords = false;
            for stop_time in stop_times.iter() {
                match stop_coords(stop_time, &stops_by_id) {
                    Some(coords_for_stop) => coords.push(coords_for_stop),
                    None => {
                        missing_coords = true;
                        break;
                    }
                }
            }
            if missing_coords {
                continue;
            }
            for i in 0..coords.len() - 1 {
                let (lat1, lon1) = coords[i];
                let (lat2, lon2) = coords[i + 1];
                distances_km.push(haversine_km(lat1, lon1, lat2, lon2));
            }

            validate_consecutive_stops(
                notices,
                trip,
                trip_row_number,
                stop_times,
                &distances_km,
                max_speed_kph,
                &stops_by_id,
                &stop_time_rows,
            );
            validate_far_stops(
                notices,
                trip,
                trip_row_number,
                stop_times,
                &distances_km,
                max_speed_kph,
                &stops_by_id,
                &stop_time_rows,
            );
        }
    }
}

fn validate_consecutive_stops(
    notices: &mut NoticeContainer,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    stop_times: &[&gtfs_model::StopTime],
    distances_km: &[f64],
    max_speed_kph: f64,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
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
            let Some(stop1) = stop_by_id(stops_by_id, &first.stop_id) else {
                return;
            };
            let Some(stop2) = stop_by_id(stops_by_id, &second.stop_id) else {
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
            ));
        }
    }
}

fn validate_far_stops(
    notices: &mut NoticeContainer,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    stop_times: &[&gtfs_model::StopTime],
    distances_km: &[f64],
    max_speed_kph: f64,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
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
                let Some(stop1) = stop_by_id(stops_by_id, &start.stop_id) else {
                    return;
                };
                let Some(stop2) = stop_by_id(stops_by_id, &end.stop_id) else {
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
                ));
                return;
            }
        }
    }
}

fn fast_travel_between_consecutive_notice(
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_model::StopTime,
    stop_time2: &gtfs_model::StopTime,
    stop1: &gtfs_model::Stop,
    stop2: &gtfs_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
) -> ValidationNotice {
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
    );
    notice
}

fn fast_travel_between_far_notice(
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_model::StopTime,
    stop_time2: &gtfs_model::StopTime,
    stop1: &gtfs_model::Stop,
    stop2: &gtfs_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
) -> ValidationNotice {
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
    );
    notice
}

fn populate_travel_speed_notice(
    notice: &mut ValidationNotice,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    stop_time1: &gtfs_model::StopTime,
    stop_time2: &gtfs_model::StopTime,
    stop1: &gtfs_model::Stop,
    stop2: &gtfs_model::Stop,
    speed_kph: f64,
    distance_km: f64,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
) {
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("tripId", trip.trip_id.as_str());
    notice.insert_context_field("routeId", trip.route_id.as_str());
    notice.insert_context_field("speedKph", speed_kph);
    notice.insert_context_field("distanceKm", distance_km);
    notice.insert_context_field("csvRowNumber1", stop_time_row(stop_time_rows, stop_time1));
    notice.insert_context_field("stopSequence1", stop_time1.stop_sequence);
    notice.insert_context_field("stopId1", stop_time1.stop_id.as_str());
    notice.insert_context_field("stopName1", stop1.stop_name.as_deref().unwrap_or(""));
    if let Some(departure) = stop_time1.departure_time {
        notice.insert_context_field("departureTime1", departure);
    }
    notice.insert_context_field("csvRowNumber2", stop_time_row(stop_time_rows, stop_time2));
    notice.insert_context_field("stopSequence2", stop_time2.stop_sequence);
    notice.insert_context_field("stopId2", stop_time2.stop_id.as_str());
    notice.insert_context_field("stopName2", stop2.stop_name.as_deref().unwrap_or(""));
    if let Some(arrival) = stop_time2.arrival_time {
        notice.insert_context_field("arrivalTime2", arrival);
    }
    notice.field_order = vec![
        "tripCsvRowNumber".to_string(),
        "tripId".to_string(),
        "routeId".to_string(),
        "speedKph".to_string(),
        "distanceKm".to_string(),
        "csvRowNumber1".to_string(),
        "stopSequence1".to_string(),
        "stopId1".to_string(),
        "stopName1".to_string(),
        "departureTime1".to_string(),
        "csvRowNumber2".to_string(),
        "stopSequence2".to_string(),
        "stopId2".to_string(),
        "stopName2".to_string(),
        "arrivalTime2".to_string(),
    ];
}

fn stop_time_row(
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    stop_time: &gtfs_model::StopTime,
) -> u64 {
    stop_time_rows
        .get(&(stop_time as *const _))
        .copied()
        .unwrap_or(2)
}

fn stop_coords(
    stop_time: &gtfs_model::StopTime,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
) -> Option<(f64, f64)> {
    let mut current_id = stop_time.stop_id.trim();
    if current_id.is_empty() {
        return None;
    }
    for _ in 0..3 {
        let stop = match stops_by_id.get(current_id) {
            Some(stop) => *stop,
            None => break,
        };
        if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
            return Some((lat, lon));
        }
        let Some(parent) = stop
            .parent_station
            .as_deref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            break;
        };
        current_id = parent;
    }
    None
}

fn stop_by_id<'a>(
    stops_by_id: &'a HashMap<&str, &gtfs_model::Stop>,
    stop_id: &str,
) -> Option<&'a gtfs_model::Stop> {
    let key = stop_id.trim();
    if key.is_empty() {
        return None;
    }
    stops_by_id.get(key).copied()
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

