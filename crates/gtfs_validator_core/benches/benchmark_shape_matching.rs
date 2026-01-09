use criterion::{criterion_group, criterion_main, Criterion};
use gtfs_guru_core::rules::shape_to_stop_matching::ShapeToStopMatchingValidator;
// Note: We need to access internal logic or just benchmark the public API.
// Since the validator struct itself is public, we can use it, but `validate` takes a full feed.
// However, the rule logic might be private or not easily exposed.
// Just checking the existing file, `ShapeToStopMatchingValidator` implements `Validator`.
// To benchmark specifically the matching logic which is private (StopToShapeMatcher),
// we might need to expose it or just benchmark the validator on a constructed feed.
// Given the constraints, let's benchmark the `validate` method on a feed with many shapes and stops.
use gtfs_guru_core::{CsvTable, GtfsFeed, NoticeContainer, Validator};
use gtfs_guru_model::{Route, RouteType, Shape, Stop, StopTime, Trip};
use std::collections::HashMap;

fn generate_complex_feed(
    num_shapes: usize,
    points_per_shape: usize,
    trips_per_shape: usize,
) -> GtfsFeed {
    let mut feed = GtfsFeed::default();
    let pool = feed.pool.clone();
    let mut shapes = Vec::new();
    let mut stops = Vec::new();
    let mut trips = Vec::new();
    let mut stop_times = Vec::new();
    let mut routes = Vec::new();
    let mut shape_id_gen = 0;

    // Create one route
    let route_id = pool.intern("R1");
    let service_id = pool.intern("SVC1");
    routes.push(Route {
        route_id,
        agency_id: None,
        route_short_name: Some("R1".into()),
        route_long_name: None,
        route_desc: None,
        route_type: RouteType::Bus,
        route_url: None,
        route_color: None,
        route_text_color: None,
        route_sort_order: None,
        continuous_pickup: None,
        continuous_drop_off: None,
        network_id: None,
        route_branding_url: None,
        ..Default::default()
    });

    for _ in 0..num_shapes {
        shape_id_gen += 1;
        let shape_id_str = format!("S{}", shape_id_gen);
        let shape_id = pool.intern(&shape_id_str);

        // Generate zigzag shape
        for i in 0..points_per_shape {
            let lat = (i as f64) * 0.001;
            let lon = if i % 2 == 0 { 0.0 } else { 0.001 };
            shapes.push(Shape {
                shape_id,
                shape_pt_lat: lat,
                shape_pt_lon: lon,
                shape_pt_sequence: i as u32,
                shape_dist_traveled: Some(i as f64 * 100.0),
            });
        }

        // Generate stops along the shape
        // For simplicity, every 10th point has a stop near it
        for i in (0..points_per_shape).step_by(10) {
            let stop_id_str = format!("STOP_{}_{}", shape_id_str, i);
            let stop_id = pool.intern(&stop_id_str);
            stops.push(Stop {
                stop_id,
                stop_code: None,
                stop_name: Some(stop_id_str.clone().into()),
                tts_stop_name: None,
                stop_desc: None,
                stop_lat: Some((i as f64) * 0.001),
                stop_lon: Some(if i % 2 == 0 { 0.0001 } else { 0.0009 }), // Slightly off
                zone_id: None,
                stop_url: None,
                location_type: None,
                parent_station: None,
                stop_timezone: None,
                wheelchair_boarding: None,
                level_id: None,
                platform_code: None,
                stop_address: None,
                stop_city: None,
                stop_region: None,
                stop_postcode: None,
                stop_country: None,
                stop_phone: None,
                ..Default::default()
            });
        }

        for t in 0..trips_per_shape {
            let trip_id_str = format!("T_{}_{}", shape_id_str, t);
            let trip_id = pool.intern(&trip_id_str);
            trips.push(Trip {
                route_id,
                service_id,
                trip_id,
                trip_headsign: None,
                trip_short_name: None,
                direction_id: None,
                block_id: None,
                shape_id: Some(shape_id),
                wheelchair_accessible: None,
                bikes_allowed: None,
                continuous_pickup: None,
                continuous_drop_off: None,
            });

            // Add stop times for stops on this shape
            for i in (0..points_per_shape).step_by(10) {
                let stop_id_str = format!("STOP_{}_{}", shape_id_str, i);
                let stop_id = pool.intern(&stop_id_str);
                stop_times.push(StopTime {
                    trip_id,
                    arrival_time: None,
                    departure_time: None,
                    stop_id,
                    location_group_id: None,
                    location_id: None,
                    stop_sequence: (i / 10) as u32,
                    stop_headsign: None,
                    pickup_type: None,
                    drop_off_type: None,
                    pickup_booking_rule_id: None,
                    drop_off_booking_rule_id: None,
                    continuous_pickup: None,
                    continuous_drop_off: None,
                    shape_dist_traveled: None,
                    timepoint: None,
                    start_pickup_drop_off_window: None,
                    end_pickup_drop_off_window: None,
                    ..Default::default()
                });
            }
        }
    }

    feed.agency = CsvTable {
        headers: vec![],
        rows: vec![],
        row_numbers: vec![],
    }; // minimal dummy
    feed.stops = CsvTable {
        headers: vec![],
        rows: stops,
        row_numbers: vec![],
    };
    feed.routes = CsvTable {
        headers: vec![],
        rows: routes,
        row_numbers: vec![],
    };
    feed.trips = CsvTable {
        headers: vec![],
        rows: trips,
        row_numbers: vec![],
    };
    feed.stop_times = CsvTable {
        headers: vec![],
        rows: stop_times,
        row_numbers: vec![],
    };
    feed.shapes = Some(CsvTable {
        headers: vec![],
        rows: shapes,
        row_numbers: vec![],
    });
    feed.stop_times_by_trip = HashMap::new();
    feed.rebuild_stop_times_index();
    feed
}

fn benchmark_validation(c: &mut Criterion) {
    // 5 shapes, 1000 points each, 5 trips each
    let feed = generate_complex_feed(5, 1000, 5);
    let validator = ShapeToStopMatchingValidator;

    c.bench_function("shape_matching_5_shapes_1000_pts", |b| {
        b.iter(|| {
            let mut notices = NoticeContainer::new();
            validator.validate(&feed, &mut notices);
        })
    });
}

criterion_group!(benches, benchmark_validation);
criterion_main!(benches);
