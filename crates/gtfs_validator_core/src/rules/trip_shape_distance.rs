use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE: &str = "trip_distance_exceeds_shape_distance";
const CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE_BELOW_THRESHOLD: &str =
    "trip_distance_exceeds_shape_distance_below_threshold";
const DISTANCE_THRESHOLD_METERS: f64 = 11.1;

#[derive(Debug, Default)]
pub struct TripAndShapeDistanceValidator;

impl Validator for TripAndShapeDistanceValidator {
    fn name(&self) -> &'static str {
        "trip_shape_distance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let shapes = match &feed.shapes {
            Some(shapes) => shapes,
            None => return,
        };

        let mut stop_times_by_trip: HashMap<&str, Vec<&gtfs_model::StopTime>> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
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

        let mut stops_by_id: HashMap<&str, &gtfs_model::Stop> = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        for trip in feed.trips.rows.iter() {
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let shape_id = match trip.shape_id.as_ref() {
                Some(shape_id) => shape_id.trim(),
                None => continue,
            };
            if shape_id.is_empty() {
                continue;
            }
            let stop_times = match stop_times_by_trip.get(trip_id) {
                Some(stop_times) if !stop_times.is_empty() => stop_times,
                _ => continue,
            };
            let last_stop_time = stop_times[stop_times.len() - 1];
            if last_stop_time.stop_id.trim().is_empty() {
                continue;
            }
            let stop = match stops_by_id.get(last_stop_time.stop_id.trim()) {
                Some(stop) => *stop,
                None => continue,
            };
            let (Some(stop_lat), Some(stop_lon)) = (stop.stop_lat, stop.stop_lon) else {
                continue;
            };

            let max_stop_time_dist = last_stop_time.shape_dist_traveled.unwrap_or(0.0);

            let mut max_shape: Option<&gtfs_model::Shape> = None;
            let mut max_shape_dist = 0.0;
            for shape in shapes
                .rows
                .iter()
                .filter(|shape| shape.shape_id.trim() == shape_id)
            {
                let dist = shape.shape_dist_traveled.unwrap_or(0.0);
                if max_shape.is_none() || dist > max_shape_dist {
                    max_shape = Some(shape);
                    max_shape_dist = dist;
                }
            }
            let max_shape = match max_shape {
                Some(shape) => shape,
                None => continue,
            };
            if max_shape_dist == 0.0 {
                continue;
            }

            let distance_meters = haversine_meters(
                max_shape.shape_pt_lat,
                max_shape.shape_pt_lon,
                stop_lat,
                stop_lon,
            );

            if max_stop_time_dist > max_shape_dist {
                let (code, severity, message) = if distance_meters > DISTANCE_THRESHOLD_METERS {
                    (
                        CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE,
                        NoticeSeverity::Error,
                        "trip distance exceeds shape distance",
                    )
                } else {
                    (
                        CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE_BELOW_THRESHOLD,
                        NoticeSeverity::Warning,
                        "trip distance exceeds shape distance (below threshold)",
                    )
                };
                let mut notice = ValidationNotice::new(code, severity, message);
                notice.insert_context_field("tripId", trip_id);
                notice.insert_context_field("shapeId", shape_id);
                notice.insert_context_field("maxTripDistanceTraveled", max_stop_time_dist);
                notice.insert_context_field("maxShapeDistanceTraveled", max_shape_dist);
                notice.insert_context_field("geoDistanceToShape", distance_meters);
                notice.field_order = vec![
                    "tripId".to_string(),
                    "shapeId".to_string(),
                    "maxTripDistanceTraveled".to_string(),
                    "maxShapeDistanceTraveled".to_string(),
                    "geoDistanceToShape".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_meters = 6_371_000.0;
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    radius_meters * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::{Shape, Stop, StopTime, Trip};

    #[test]
    fn detects_trip_exceeds_shape_distance_above_threshold() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "shape_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                shape_id: Some("SH1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".to_string(), "shape_dist_traveled".to_string()],
            rows: vec![Shape {
                shape_id: "SH1".to_string(),
                shape_dist_traveled: Some(10.0),
                shape_pt_lat: 40.7128,
                shape_pt_lon: -74.0060,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".to_string(),
                "stop_lat".to_string(),
                "stop_lon".to_string(),
            ],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_lat: Some(40.7128),
                stop_lon: Some(-73.0060), // Far away (~100km)
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                stop_sequence: 1,
                shape_dist_traveled: Some(20.0), // Greater than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE));
    }

    #[test]
    fn detects_trip_exceeds_shape_distance_below_threshold() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "shape_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                shape_id: Some("SH1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".to_string(), "shape_dist_traveled".to_string()],
            rows: vec![Shape {
                shape_id: "SH1".to_string(),
                shape_dist_traveled: Some(10.0),
                shape_pt_lat: 40.7128,
                shape_pt_lon: -74.0060,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".to_string(),
                "stop_lat".to_string(),
                "stop_lon".to_string(),
            ],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_lat: Some(40.7128),
                stop_lon: Some(-74.0060), // Exact same location
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                stop_sequence: 1,
                shape_dist_traveled: Some(20.0), // Greater than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE_BELOW_THRESHOLD));
    }

    #[test]
    fn passes_valid_trip_shape_distance() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "shape_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                shape_id: Some("SH1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".to_string(), "shape_dist_traveled".to_string()],
            rows: vec![Shape {
                shape_id: "SH1".to_string(),
                shape_dist_traveled: Some(20.0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".to_string(),
                "stop_lat".to_string(),
                "stop_lon".to_string(),
            ],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_lat: Some(40.7128),
                stop_lon: Some(-74.0060),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_id".to_string(),
                "stop_sequence".to_string(),
                "shape_dist_traveled".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                stop_sequence: 1,
                shape_dist_traveled: Some(10.0), // Less than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
