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

        // Pre-build index: shape_id -> (max_dist, lat, lon) - O(shapes) instead of O(trips × shapes)
        // Parallelize shape index building?
        // It's a reduction. For now, keep sequential or use parallel iterator + reduce.
        // Given it's a HashMap insert, sequential might be fine or use DashMap if we really want parallel building.
        // Let's keep it sequential for now as shapes are usually fewer than trips, or just efficient enough.
        // Actually, let's just keep the index build sequential as it constructs a HashMap.
        let mut shape_max_dist: HashMap<gtfs_guru_model::StringId, (f64, f64, f64)> =
            HashMap::new();
        for shape in &shapes.rows {
            let shape_id = shape.shape_id;
            if shape_id.0 == 0 {
                continue;
            }
            let dist = shape.shape_dist_traveled.unwrap_or(0.0);
            let entry = shape_max_dist.entry(shape_id).or_insert((0.0, 0.0, 0.0));
            if dist > entry.0 {
                *entry = (dist, shape.shape_pt_lat, shape.shape_pt_lon);
            }
        }

        // We can use feed.stop_times_by_trip which is already built!
        // Oh wait, `trip_shape_distance` uses `stop_times_by_trip` derived locally in the original code.
        // But `feed.stop_times_by_trip` exists on `GtfsFeed` and is built in `feed.rs`.
        // We should use `feed.stop_times_by_trip`!
        // But we need `stop_times` objects. `feed.stop_times_by_trip` gives us indices `Vec<usize>`.
        // We can use that.

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        // Create a thread-safe context
        let context = ValidationContext {
            shape_max_dist,
            stops_by_id,
        };

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            // Iterate over trips in parallel
            let results: Vec<NoticeContainer> = feed
                .trips
                .rows
                .par_iter()
                .map(|trip| check_trip(trip, feed, &context))
                .collect();

            for result in results {
                notices.merge(result);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            for trip in &feed.trips.rows {
                let result = check_trip(trip, feed, &context);
                notices.merge(result);
            }
        }
    }
}

struct ValidationContext<'a> {
    shape_max_dist: HashMap<gtfs_guru_model::StringId, (f64, f64, f64)>,
    stops_by_id: HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Stop>,
}

unsafe impl<'a> Sync for ValidationContext<'a> {}
unsafe impl<'a> Send for ValidationContext<'a> {}

fn check_trip(
    trip: &gtfs_guru_model::Trip,
    feed: &GtfsFeed,
    context: &ValidationContext,
) -> NoticeContainer {
    let mut notices = NoticeContainer::new();
    let trip_id = trip.trip_id;
    if trip_id.0 == 0 {
        return notices;
    }
    let Some(shape_id) = trip.shape_id.filter(|id| id.0 != 0) else {
        return notices;
    };

    // Use the pre-calculated index from feed
    let stop_time_indices = match feed.stop_times_by_trip.get(&trip_id) {
        Some(indices) if !indices.is_empty() => indices,
        _ => return notices,
    };

    // We need the last stop time. Indices are sorted by stop_sequence in feed.rs
    let last_index = stop_time_indices[stop_time_indices.len() - 1];
    let last_stop_time = &feed.stop_times.rows[last_index];

    if last_stop_time.stop_id.0 == 0 {
        return notices;
    }
    let stop = match context.stops_by_id.get(&last_stop_time.stop_id) {
        Some(stop) => *stop,
        None => return notices,
    };
    let (Some(stop_lat), Some(stop_lon)) = (stop.stop_lat, stop.stop_lon) else {
        return notices;
    };

    let max_stop_time_dist = last_stop_time.shape_dist_traveled.unwrap_or(0.0);

    let (max_shape_dist, shape_lat, shape_lon) = match context.shape_max_dist.get(&shape_id) {
        Some(&data) if data.0 > 0.0 => data,
        _ => return notices,
    };

    let distance_meters = haversine_meters(shape_lat, shape_lon, stop_lat, stop_lon);

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
        let trip_id_value = feed.pool.resolve(trip_id);
        let shape_id_value = feed.pool.resolve(shape_id);
        let mut notice = ValidationNotice::new(code, severity, message);
        notice.insert_context_field("tripId", trip_id_value.as_str());
        notice.insert_context_field("shapeId", shape_id_value.as_str());
        notice.insert_context_field("maxTripDistanceTraveled", max_stop_time_dist);
        notice.insert_context_field("maxShapeDistanceTraveled", max_shape_dist);
        notice.insert_context_field("geoDistanceToShape", distance_meters);
        notice.field_order = vec![
            "tripId".into(),
            "shapeId".into(),
            "maxTripDistanceTraveled".into(),
            "maxShapeDistanceTraveled".into(),
            "geoDistanceToShape".into(),
        ];
        notices.push(notice);
    }
    notices
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
    use gtfs_guru_model::{Shape, Stop, StopTime, Trip};

    #[test]
    fn detects_trip_exceeds_shape_distance_above_threshold() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into(), "shape_dist_traveled".into()],
            rows: vec![Shape {
                shape_id: feed.pool.intern("SH1"),
                shape_dist_traveled: Some(10.0),
                shape_pt_lat: 40.7128,
                shape_pt_lon: -74.0060,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_lat: Some(40.7128),
                stop_lon: Some(-73.0060), // Far away (~100km)
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_id: feed.pool.intern("S1"),
                stop_sequence: 1,
                shape_dist_traveled: Some(20.0), // Greater than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE));
    }

    #[test]
    fn detects_trip_exceeds_shape_distance_below_threshold() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into(), "shape_dist_traveled".into()],
            rows: vec![Shape {
                shape_id: feed.pool.intern("SH1"),
                shape_dist_traveled: Some(10.0),
                shape_pt_lat: 40.7128,
                shape_pt_lon: -74.0060,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_lat: Some(40.7128),
                stop_lon: Some(-74.0060), // Exact same location
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_id: feed.pool.intern("S1"),
                stop_sequence: 1,
                shape_dist_traveled: Some(20.0), // Greater than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRIP_DISTANCE_EXCEEDS_SHAPE_DISTANCE_BELOW_THRESHOLD));
    }

    #[test]
    fn passes_valid_trip_shape_distance() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into(), "shape_dist_traveled".into()],
            rows: vec![Shape {
                shape_id: feed.pool.intern("SH1"),
                shape_dist_traveled: Some(20.0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_lat: Some(40.7128),
                stop_lon: Some(-74.0060),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_id".into(),
                "stop_sequence".into(),
                "shape_dist_traveled".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_id: feed.pool.intern("S1"),
                stop_sequence: 1,
                shape_dist_traveled: Some(10.0), // Less than shape distance
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        feed.rebuild_stop_times_index();
        TripAndShapeDistanceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
