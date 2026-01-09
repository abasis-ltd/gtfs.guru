use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::RouteType;
use gtfs_guru_model::StringId;
use rstar::{RTree, RTreeObject, AABB};

const CODE_STOP_TOO_FAR_FROM_SHAPE: &str = "stop_too_far_from_shape";
const CODE_STOP_TOO_FAR_FROM_SHAPE_USER_DISTANCE: &str =
    "stop_too_far_from_shape_using_user_distance";
const CODE_STOP_HAS_TOO_MANY_MATCHES: &str = "stop_has_too_many_matches_for_shape";
const CODE_STOPS_MATCH_OUT_OF_ORDER: &str = "stops_match_shape_out_of_order";

#[derive(Debug, Default)]
pub struct ShapeToStopMatchingValidator;

impl Validator for ShapeToStopMatchingValidator {
    fn name(&self) -> &'static str {
        "shape_to_stop_matching"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let shapes = match &feed.shapes {
            Some(shapes) if !shapes.rows.is_empty() => shapes,
            _ => return,
        };
        if feed.stops.rows.is_empty()
            || feed.trips.rows.is_empty()
            || feed.stop_times.rows.is_empty()
        {
            return;
        }

        let mut stop_time_rows: HashMap<usize, u64> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            stop_time_rows.insert(
                stop_time as *const _ as usize,
                feed.stop_times.row_number(index),
            );
        }

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

        let mut trip_rows: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();
        let mut trips_by_shape: HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Trip>> =
            HashMap::new();
        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let trip_id = trip.trip_id;
            if trip_id.0 != 0 {
                trip_rows.insert(trip_id, feed.trips.row_number(index));
            }
            let Some(shape_id) = trip.shape_id.filter(|id| id.0 != 0) else {
                continue;
            };
            trips_by_shape.entry(shape_id).or_default().push(trip);
        }

        let mut shapes_by_id: HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Shape>> =
            HashMap::new();
        for shape in &shapes.rows {
            let shape_id = shape.shape_id;
            if shape_id.0 == 0 {
                continue;
            }
            shapes_by_id.entry(shape_id).or_default().push(shape);
        }

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            let ctx = crate::ValidationContextState::capture();
            let results: Vec<NoticeContainer> = shapes_by_id
                .into_par_iter()
                .filter_map(|(shape_id, shape_points_raw)| {
                    let _guards = ctx.apply();
                    let trips = trips_by_shape.get(&shape_id)?;
                    let shape_points = ShapePoints::from_shapes(shape_points_raw);
                    if shape_points.is_empty() {
                        return None;
                    }

                    let mut local_notices = NoticeContainer::new();
                    let mut processed_trip_hashes = HashSet::new();
                    let mut reported_stop_ids = HashSet::new();
                    let matcher = StopToShapeMatcher::default();

                    for trip in trips {
                        let trip_id = trip.trip_id;
                        if trip_id.0 == 0 {
                            continue;
                        }
                        let stop_time_indices = match feed.stop_times_by_trip.get(&trip_id) {
                            Some(indices) if !indices.is_empty() => indices,
                            _ => continue,
                        };
                        let stop_times: Vec<&gtfs_guru_model::StopTime> = stop_time_indices
                            .iter()
                            .map(|&i| &feed.stop_times.rows[i])
                            .collect();
                        let stop_times = stop_times.as_slice();
                        if !processed_trip_hashes.insert(trip_hash(stop_times)) {
                            continue;
                        }
                        let route_id = trip.route_id;
                        if route_id.0 == 0 {
                            continue;
                        }
                        let route = match routes_by_id.get(&route_id) {
                            Some(route) => *route,
                            None => continue,
                        };

                        let station_size = StopPoints::route_type_to_station_size(route.route_type);
                        let stop_points =
                            StopPoints::from_stop_times(stop_times, &stops_by_id, station_size);
                        let geo_result =
                            matcher.match_using_geo_distance(&stop_points, &shape_points);
                        let trip_row_number = trip_rows.get(&trip_id).copied().unwrap_or(2);
                        let shape_id = trip.shape_id.unwrap_or(StringId(0));
                        report_problems(
                            trip,
                            trip_row_number,
                            shape_id,
                            &stops_by_id,
                            &geo_result.problems,
                            MatchingDistance::Geo,
                            &mut reported_stop_ids,
                            &stop_time_rows,
                            &shape_points,
                            &mut local_notices,
                            feed,
                        );

                        if stop_points.has_user_distance() && shape_points.has_user_distance() {
                            let user_result =
                                matcher.match_using_user_distance(&stop_points, &shape_points);
                            report_problems(
                                trip,
                                trip_row_number,
                                shape_id,
                                &stops_by_id,
                                &user_result.problems,
                                MatchingDistance::User,
                                &mut reported_stop_ids,
                                &stop_time_rows,
                                &shape_points,
                                &mut local_notices,
                                feed,
                            );
                        }
                    }
                    Some(local_notices)
                })
                .collect();

            for local_notices in results {
                notices.merge(local_notices);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            let matcher = StopToShapeMatcher::default();
            for (shape_id, shape_points_raw) in shapes_by_id {
                let trips = match trips_by_shape.get(&shape_id) {
                    Some(trips) => trips,
                    None => continue,
                };
                let shape_points = ShapePoints::from_shapes(shape_points_raw);
                if shape_points.is_empty() {
                    continue;
                }

                let mut processed_trip_hashes = HashSet::new();
                let mut reported_stop_ids = HashSet::new();
                for trip in trips {
                    let trip_id = trip.trip_id;
                    if trip_id.0 == 0 {
                        continue;
                    }
                    let stop_time_indices = match feed.stop_times_by_trip.get(&trip_id) {
                        Some(indices) if !indices.is_empty() => indices,
                        _ => continue,
                    };
                    let stop_times: Vec<&gtfs_guru_model::StopTime> = stop_time_indices
                        .iter()
                        .map(|&i| &feed.stop_times.rows[i])
                        .collect();
                    let stop_times = stop_times.as_slice();
                    if !processed_trip_hashes.insert(trip_hash(stop_times)) {
                        continue;
                    }
                    let route_id = trip.route_id;
                    if route_id.0 == 0 {
                        continue;
                    }
                    let route = match routes_by_id.get(&route_id) {
                        Some(route) => *route,
                        None => continue,
                    };

                    let station_size = StopPoints::route_type_to_station_size(route.route_type);
                    let stop_points =
                        StopPoints::from_stop_times(stop_times, &stops_by_id, station_size);
                    let geo_result = matcher.match_using_geo_distance(&stop_points, &shape_points);
                    let trip_row_number = trip_rows.get(&trip_id).copied().unwrap_or(2);
                    let shape_id = trip.shape_id.unwrap_or(StringId(0));
                    report_problems(
                        trip,
                        trip_row_number,
                        shape_id,
                        &stops_by_id,
                        &geo_result.problems,
                        MatchingDistance::Geo,
                        &mut reported_stop_ids,
                        &stop_time_rows,
                        &shape_points,
                        notices,
                        feed,
                    );

                    if stop_points.has_user_distance() && shape_points.has_user_distance() {
                        let user_result =
                            matcher.match_using_user_distance(&stop_points, &shape_points);
                        report_problems(
                            trip,
                            trip_row_number,
                            shape_id,
                            &stops_by_id,
                            &user_result.problems,
                            MatchingDistance::User,
                            &mut reported_stop_ids,
                            &stop_time_rows,
                            &shape_points,
                            notices,
                            feed,
                        );
                    }
                }
            }
        }
    }
}

fn report_problems(
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    problems: &[Problem<'_>],
    matching_distance: MatchingDistance,
    reported_stop_ids: &mut HashSet<StringId>,
    stop_time_rows: &HashMap<usize, u64>,
    shape_points: &ShapePoints,
    notices: &mut NoticeContainer,
    feed: &GtfsFeed,
) {
    for problem in problems {
        let stop_id = problem.stop_time.stop_id;
        if stop_id.0 == 0 {
            continue;
        }
        if problem.problem_type == ProblemType::StopTooFarFromShape
            && !reported_stop_ids.insert(stop_id)
        {
            continue;
        }
        if !thorough_mode_enabled()
            && problem.problem_type != ProblemType::StopTooFarFromShape
            && problem.problem_type != ProblemType::StopsMatchOutOfOrder
        {
            continue;
        }
        notices.push(problem_notice(
            problem,
            matching_distance,
            trip,
            trip_row_number,
            shape_id,
            stops_by_id,
            stop_time_rows,
            shape_points,
            feed,
        ));
    }
}

fn problem_notice(
    problem: &Problem<'_>,
    matching_distance: MatchingDistance,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    shape_points: &ShapePoints,
    feed: &GtfsFeed,
) -> ValidationNotice {
    match problem.problem_type {
        ProblemType::StopTooFarFromShape => match matching_distance {
            MatchingDistance::Geo => stop_too_far_from_shape_notice(
                problem,
                trip,
                trip_row_number,
                shape_id,
                stops_by_id,
                stop_time_rows,
                shape_points,
                feed,
            ),
            MatchingDistance::User => stop_too_far_from_shape_user_notice(
                problem,
                trip,
                trip_row_number,
                shape_id,
                stops_by_id,
                stop_time_rows,
                shape_points,
                feed,
            ),
        },
        ProblemType::StopHasTooManyMatches => stop_has_too_many_matches_notice(
            problem,
            trip,
            trip_row_number,
            shape_id,
            stops_by_id,
            stop_time_rows,
            feed,
        ),
        ProblemType::StopsMatchOutOfOrder => stops_match_out_of_order_notice(
            problem,
            trip,
            trip_row_number,
            shape_id,
            stops_by_id,
            stop_time_rows,
            feed,
        ),
    }
}

fn stop_too_far_from_shape_notice(
    problem: &Problem<'_>,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    shape_points: &ShapePoints,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOP_TOO_FAR_FROM_SHAPE,
        NoticeSeverity::Warning,
        "stop is too far from shape",
    );
    populate_stop_too_far_notice(
        &mut notice,
        problem,
        trip,
        trip_row_number,
        shape_id,
        stops_by_id,
        stop_time_rows,
        shape_points,
        feed,
    );
    notice
}

fn stop_too_far_from_shape_user_notice(
    problem: &Problem<'_>,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    shape_points: &ShapePoints,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOP_TOO_FAR_FROM_SHAPE_USER_DISTANCE,
        NoticeSeverity::Warning,
        "stop is too far from shape using user distance",
    );
    populate_stop_too_far_notice(
        &mut notice,
        problem,
        trip,
        trip_row_number,
        shape_id,
        stops_by_id,
        stop_time_rows,
        shape_points,
        feed,
    );
    notice
}

fn stop_has_too_many_matches_notice(
    problem: &Problem<'_>,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOP_HAS_TOO_MANY_MATCHES,
        NoticeSeverity::Warning,
        "stop has too many matches for shape",
    );
    let shape_id_value = feed.pool.resolve(shape_id);
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id_value.as_str());
    notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
    notice.insert_context_field(
        "stopTimeCsvRowNumber",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field(
        "stopId",
        feed.pool.resolve(problem.stop_time.stop_id).as_str(),
    );
    notice.insert_context_field(
        "stopName",
        stop_name_by_id(stops_by_id, problem.stop_time.stop_id),
    );
    notice.insert_context_field("match", lat_lng_array(problem.match_result.location));
    notice.insert_context_field("matchCount", problem.match_count);
    notice.field_order = vec![
        "tripCsvRowNumber".into(),
        "shapeId".into(),
        "tripId".into(),
        "stopTimeCsvRowNumber".into(),
        "stopId".into(),
        "stopName".into(),
        "match".into(),
        "matchCount".into(),
    ];
    notice
}

fn stops_match_out_of_order_notice(
    problem: &Problem<'_>,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOPS_MATCH_OUT_OF_ORDER,
        NoticeSeverity::Warning,
        "stops match shape out of order",
    );
    let shape_id_value = feed.pool.resolve(shape_id);
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id_value.as_str());
    notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
    let prev_stop_time = problem.prev_stop_time.unwrap_or(problem.stop_time);
    let prev_match = problem.prev_match.as_ref().unwrap_or(&problem.match_result);
    notice.insert_context_field(
        "stopTimeCsvRowNumber1",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field(
        "stopId1",
        feed.pool.resolve(problem.stop_time.stop_id).as_str(),
    );
    notice.insert_context_field(
        "stopName1",
        stop_name_by_id(stops_by_id, problem.stop_time.stop_id),
    );
    notice.insert_context_field("match1", lat_lng_array(problem.match_result.location));
    notice.insert_context_field(
        "stopTimeCsvRowNumber2",
        stop_time_row(stop_time_rows, prev_stop_time),
    );
    notice.insert_context_field(
        "stopId2",
        feed.pool.resolve(prev_stop_time.stop_id).as_str(),
    );
    notice.insert_context_field(
        "stopName2",
        stop_name_by_id(stops_by_id, prev_stop_time.stop_id),
    );
    notice.insert_context_field("match2", lat_lng_array(prev_match.location));
    notice.field_order = vec![
        "tripCsvRowNumber".into(),
        "shapeId".into(),
        "tripId".into(),
        "stopTimeCsvRowNumber1".into(),
        "stopId1".into(),
        "stopName1".into(),
        "match1".into(),
        "stopTimeCsvRowNumber2".into(),
        "stopId2".into(),
        "stopName2".into(),
        "match2".into(),
    ];
    notice
}

fn populate_stop_too_far_notice(
    notice: &mut ValidationNotice,
    problem: &Problem<'_>,
    trip: &gtfs_guru_model::Trip,
    trip_row_number: u64,
    shape_id: StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_time_rows: &HashMap<usize, u64>,
    shape_points: &ShapePoints,
    feed: &GtfsFeed,
) {
    let shape_id_value = feed.pool.resolve(shape_id);
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id_value.as_str());
    notice.insert_context_field("tripId", feed.pool.resolve(trip.trip_id).as_str());
    notice.insert_context_field(
        "stopTimeCsvRowNumber",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field(
        "stopId",
        feed.pool.resolve(problem.stop_time.stop_id).as_str(),
    );
    notice.insert_context_field(
        "stopName",
        stop_name_by_id(stops_by_id, problem.stop_time.stop_id),
    );
    // Include the actual stop location for map visualization
    if let Some(stop_location) = stop_location_by_id(stops_by_id, problem.stop_time.stop_id) {
        notice.insert_context_field("stopLocation", lat_lng_array(stop_location));
    }
    notice.insert_context_field("match", lat_lng_array(problem.match_result.location));
    notice.insert_context_field(
        "geoDistanceToShape",
        problem.match_result.geo_distance_to_shape,
    );

    // Extract shape path segment around the error for visualization
    let shape_path = extract_shape_path_segment(shape_points, problem.match_result.index);
    if !shape_path.is_empty() {
        notice.insert_context_field("shapePath", shape_path);
        notice.insert_context_field("matchIndex", problem.match_result.index);
    }

    notice.field_order = vec![
        "tripCsvRowNumber".into(),
        "shapeId".into(),
        "tripId".into(),
        "stopTimeCsvRowNumber".into(),
        "stopId".into(),
        "stopName".into(),
        "stopLocation".into(),
        "match".into(),
        "geoDistanceToShape".into(),
        "shapePath".into(),
        "matchIndex".into(),
    ];
}

/// Extract a segment of the shape path around the given index (±10 points or ±500m)
fn extract_shape_path_segment(shape_points: &ShapePoints, match_index: usize) -> Vec<[f64; 2]> {
    const MAX_POINTS_EACH_SIDE: usize = 10;
    const MAX_DISTANCE_METERS: f64 = 500.0;

    if shape_points.points.is_empty() {
        return Vec::new();
    }

    let match_index = match_index.min(shape_points.points.len().saturating_sub(1));
    let match_geo_distance = shape_points
        .points
        .get(match_index)
        .map(|p| p.geo_distance)
        .unwrap_or(0.0);

    // Find start index (go back up to MAX_POINTS_EACH_SIDE or MAX_DISTANCE_METERS)
    let mut start_index = match_index;
    for i in (0..match_index).rev() {
        let dist_diff = match_geo_distance - shape_points.points[i].geo_distance;
        if dist_diff > MAX_DISTANCE_METERS || match_index - i > MAX_POINTS_EACH_SIDE {
            break;
        }
        start_index = i;
    }

    // Find end index (go forward up to MAX_POINTS_EACH_SIDE or MAX_DISTANCE_METERS)
    let mut end_index = match_index;
    for i in (match_index + 1)..shape_points.points.len() {
        let dist_diff = shape_points.points[i].geo_distance - match_geo_distance;
        if dist_diff > MAX_DISTANCE_METERS || i - match_index > MAX_POINTS_EACH_SIDE {
            break;
        }
        end_index = i;
    }

    // Extract coordinates
    shape_points.points[start_index..=end_index]
        .iter()
        .map(|p| [p.location.lat, p.location.lon])
        .collect()
}

fn stop_time_row(
    stop_time_rows: &HashMap<usize, u64>,
    stop_time: &gtfs_guru_model::StopTime,
) -> u64 {
    stop_time_rows
        .get(&(stop_time as *const _ as usize))
        .copied()
        .unwrap_or(2)
}

fn trip_hash(stop_times: &[&gtfs_guru_model::StopTime]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stop_times.len().hash(&mut hasher);
    for stop_time in stop_times {
        stop_time.stop_id.hash(&mut hasher);
        let distance = stop_time.shape_dist_traveled.unwrap_or(0.0);
        distance.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn lat_lng_array(lat_lng: LatLng) -> [f64; 2] {
    [lat_lng.lat, lat_lng.lon]
}

fn stop_name_by_id<'a>(
    stops_by_id: &'a HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Stop>,
    stop_id: gtfs_guru_model::StringId,
) -> &'a str {
    if stop_id.0 == 0 {
        return "";
    }
    stops_by_id
        .get(&stop_id)
        .and_then(|stop| stop.stop_name.as_deref())
        .unwrap_or("")
}

fn stop_location_by_id(
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_id: gtfs_guru_model::StringId,
) -> Option<LatLng> {
    if stop_id.0 == 0 {
        return None;
    }
    stops_by_id.get(&stop_id).and_then(|stop| {
        let lat = stop.stop_lat?;
        let lon = stop.stop_lon?;
        Some(LatLng { lat, lon })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchingDistance {
    Geo,
    User,
}

#[derive(Debug, Clone)]
struct StopToShapeMatcher {
    settings: StopToShapeMatcherSettings,
}

impl Default for StopToShapeMatcher {
    fn default() -> Self {
        Self {
            settings: StopToShapeMatcherSettings::default(),
        }
    }
}

impl StopToShapeMatcher {
    fn match_using_user_distance<'a>(
        &self,
        stop_points: &StopPoints<'a>,
        shape_points: &ShapePoints,
    ) -> MatchResult<'a> {
        let mut matches = Vec::new();
        let mut problems = Vec::new();
        if stop_points.is_empty() || shape_points.is_empty() {
            return MatchResult { matches, problems };
        }

        let mut potential_matches = Vec::with_capacity(stop_points.size());
        let mut search_from_index = 0;
        for stop_point in stop_points.points.iter() {
            let matches_for_stop = if !stop_point.has_user_distance() {
                let matches_for_stop = self.compute_potential_matches_using_geo_distance(
                    shape_points,
                    stop_point,
                    &mut problems,
                );
                if matches_for_stop.is_empty() {
                    return MatchResult { matches, problems };
                }
                matches_for_stop
            } else {
                let match_result = shape_points.match_from_user_dist(
                    stop_point.user_distance,
                    search_from_index,
                    stop_point.location,
                );
                search_from_index = match_result.index;
                vec![match_result]
            };
            potential_matches.push(matches_for_stop);
        }

        matches = find_best_matches(stop_points, &potential_matches, &mut problems);
        if !matches.is_empty()
            && !self.is_valid_stops_to_shape_match_from_user_distance(
                stop_points,
                &matches,
                &mut problems,
            )
        {
            matches.clear();
        }
        MatchResult { matches, problems }
    }

    fn match_using_geo_distance<'a>(
        &self,
        stop_points: &StopPoints<'a>,
        shape_points: &ShapePoints,
    ) -> MatchResult<'a> {
        let mut matches = Vec::new();
        let mut problems = Vec::new();
        if stop_points.is_empty() || shape_points.is_empty() {
            return MatchResult { matches, problems };
        }

        let mut potential_matches = Vec::with_capacity(stop_points.size());
        let mut ok = true;
        for stop_point in stop_points.points.iter() {
            let matches_for_stop = self.compute_potential_matches_using_geo_distance(
                shape_points,
                stop_point,
                &mut problems,
            );
            ok &= !matches_for_stop.is_empty();
            potential_matches.push(matches_for_stop);
        }
        if !ok {
            return MatchResult { matches, problems };
        }

        matches = find_best_matches(stop_points, &potential_matches, &mut problems);
        MatchResult { matches, problems }
    }

    fn compute_potential_matches_using_geo_distance<'a>(
        &self,
        shape_points: &ShapePoints,
        stop_point: &StopPoint<'a>,
        problems: &mut Vec<Problem<'a>>,
    ) -> Vec<StopToShapeMatch> {
        let max_distance = self.settings.max_distance_from_stop_to_shape_meters
            * if stop_point.is_large_station {
                self.settings.large_station_distance_multiplier
            } else {
                1.0
            };
        let matches_for_stop =
            shape_points.matches_from_location(stop_point.location, max_distance);
        if matches_for_stop.is_empty() {
            let match_result = shape_points.match_from_location(stop_point.location);
            if match_result.geo_distance_to_shape > max_distance {
                problems.push(Problem::stop_too_far_from_shape(
                    stop_point.stop_time,
                    match_result,
                ));
            }
            return matches_for_stop;
        }
        if matches_for_stop.len() > self.settings.potential_matches_for_stop_problem_threshold {
            let closest = matches_for_stop
                .iter()
                .cloned()
                .min_by(|a, b| cmp_f64(a.geo_distance_to_shape, b.geo_distance_to_shape));
            if let Some(match_result) = closest {
                problems.push(Problem::stop_has_too_many_matches(
                    stop_point.stop_time,
                    match_result,
                    matches_for_stop.len(),
                ));
            }
        }
        matches_for_stop
    }

    fn is_valid_stops_to_shape_match_from_user_distance<'a>(
        &self,
        stop_points: &StopPoints<'a>,
        matches: &[StopToShapeMatch],
        problems: &mut Vec<Problem<'a>>,
    ) -> bool {
        let mut valid = true;
        for (idx, match_result) in matches.iter().enumerate() {
            if match_result.geo_distance_to_shape
                > self.settings.max_distance_from_stop_to_shape_meters
            {
                problems.push(Problem::stop_too_far_from_shape(
                    stop_points.points[idx].stop_time,
                    match_result.clone(),
                ));
                valid = false;
            }
        }
        valid
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MatchResult<'a> {
    matches: Vec<StopToShapeMatch>,
    problems: Vec<Problem<'a>>,
}

#[derive(Debug, Clone)]
struct StopToShapeMatcherSettings {
    max_distance_from_stop_to_shape_meters: f64,
    large_station_distance_multiplier: f64,
    potential_matches_for_stop_problem_threshold: usize,
}

impl Default for StopToShapeMatcherSettings {
    fn default() -> Self {
        Self {
            max_distance_from_stop_to_shape_meters: 100.0,
            large_station_distance_multiplier: 4.0,
            potential_matches_for_stop_problem_threshold: 20,
        }
    }
}

#[derive(Debug, Clone)]
struct StopPoints<'a> {
    points: Vec<StopPoint<'a>>,
}

impl<'a> StopPoints<'a> {
    fn from_stop_times(
        stop_times: &[&'a gtfs_guru_model::StopTime],
        stops_by_id: &HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Stop>,
        station_size: StationSize,
    ) -> Self {
        let mut points = Vec::with_capacity(stop_times.len());
        for stop_time in stop_times.iter() {
            let stop_id = stop_time.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            let location = match stop_or_parent_location(stops_by_id, stop_id) {
                Some(location) => location,
                None => continue,
            };
            points.push(StopPoint {
                location,
                user_distance: stop_time.shape_dist_traveled.unwrap_or(0.0),
                stop_time,
                is_large_station: false,
            });
        }
        if station_size == StationSize::Large && !points.is_empty() {
            points[0].is_large_station = true;
            if let Some(last) = points.last_mut() {
                last.is_large_station = true;
            }
        }
        Self { points }
    }

    fn route_type_to_station_size(route_type: RouteType) -> StationSize {
        if route_type == RouteType::Rail {
            StationSize::Large
        } else {
            StationSize::Small
        }
    }

    fn has_user_distance(&self) -> bool {
        self.points
            .last()
            .map(|point| point.has_user_distance())
            .unwrap_or(false)
    }

    fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn size(&self) -> usize {
        self.points.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationSize {
    Small,
    Large,
}

#[derive(Debug, Clone)]
struct StopPoint<'a> {
    location: LatLng,
    user_distance: f64,
    stop_time: &'a gtfs_guru_model::StopTime,
    is_large_station: bool,
}

impl StopPoint<'_> {
    fn has_user_distance(&self) -> bool {
        self.user_distance > 0.0
    }
}

#[derive(Debug, Clone)]
struct ShapePoints {
    points: Vec<ShapePoint>,
    rtree: RTree<ShapeSegment>,
}

#[derive(Debug, Clone, Copy)]
struct ShapeSegment {
    index: usize,
    p1: LatLng,
    p1_vec: Vec3,
    p2: LatLng,
    p2_vec: Vec3,
}

impl RTreeObject for ShapeSegment {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let min_lat = self.p1.lat.min(self.p2.lat);
        let max_lat = self.p1.lat.max(self.p2.lat);
        let min_lon = self.p1.lon.min(self.p2.lon);
        let max_lon = self.p1.lon.max(self.p2.lon);
        AABB::from_corners([min_lat, min_lon], [max_lat, max_lon])
    }
}

impl rstar::PointDistance for ShapeSegment {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let p_lat = point[0];
        let p_lon = point[1];
        let p1_lat = self.p1.lat;
        let p1_lon = self.p1.lon;
        let p2_lat = self.p2.lat;
        let p2_lon = self.p2.lon;

        let l2 = (p1_lat - p2_lat).powi(2) + (p1_lon - p2_lon).powi(2);
        if l2 == 0.0 {
            return (p_lat - p1_lat).powi(2) + (p_lon - p1_lon).powi(2);
        }

        let t = ((p_lat - p1_lat) * (p2_lat - p1_lat) + (p_lon - p1_lon) * (p2_lon - p1_lon)) / l2;
        let t = t.max(0.0).min(1.0);

        let proj_lat = p1_lat + t * (p2_lat - p1_lat);
        let proj_lon = p1_lon + t * (p2_lon - p1_lon);

        (p_lat - proj_lat).powi(2) + (p_lon - proj_lon).powi(2)
    }
}

impl ShapePoints {
    fn from_shapes(mut shapes: Vec<&gtfs_guru_model::Shape>) -> Self {
        shapes.sort_by_key(|shape| shape.shape_pt_sequence);
        let mut points = Vec::with_capacity(shapes.len());
        let mut segments = Vec::with_capacity(shapes.len().saturating_sub(1));
        let mut geo_distance = 0.0_f64;
        let mut user_distance = 0.0_f64;
        for (idx, shape) in shapes.iter().enumerate() {
            if idx > 0 {
                let prev = shapes[idx - 1];
                let prev_loc = lat_lng(prev);
                let curr_loc = lat_lng(shape);
                let prev_vec = lat_lng_to_vec(prev_loc);
                let curr_vec = lat_lng_to_vec(curr_loc);
                geo_distance += distance_meters_vec(prev_vec, curr_vec).max(0.0);

                segments.push(ShapeSegment {
                    index: idx - 1,
                    p1: prev_loc,
                    p1_vec: prev_vec,
                    p2: curr_loc,
                    p2_vec: curr_vec,
                });
            }
            user_distance = user_distance.max(shape.shape_dist_traveled.unwrap_or(0.0));
            let loc = lat_lng(shape);
            points.push(ShapePoint {
                geo_distance,
                user_distance,
                location: loc,
                location_vec: lat_lng_to_vec(loc),
            });
        }

        let rtree = RTree::bulk_load(segments);

        Self { points, rtree }
    }

    fn has_user_distance(&self) -> bool {
        self.points
            .last()
            .map(|point| point.has_user_distance())
            .unwrap_or(false)
    }

    fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn match_from_location(&self, location: LatLng) -> StopToShapeMatch {
        if self.points.is_empty() {
            return StopToShapeMatch::new();
        }
        if self.points.len() == 1 {
            let mut best_match = StopToShapeMatch::new();
            let closest = self.points[0].location;
            best_match.keep_best_match(closest, haversine_meters(location, closest), 0);
            if best_match.has_best_match() {
                self.fill_location_match(&mut best_match);
            }
            return best_match;
        }

        // Use RTree nearest neighbor to find the closest segment quickly
        // Note: The distance_2 impl uses Euclidean degrees, so it's an approximation.
        // We might want to check a few neighbors or verify, but usually it's good enough for "closest".
        // however, to be strictly correct with haversine, nearest neighbor in lat/lon space
        // is very likely the nearest in haversine unless distortion is extreme.
        let closest_segment = self.rtree.nearest_neighbor(&[location.lat, location.lon]);
        let location_vec = lat_lng_to_vec(location);

        if let Some(segment) = closest_segment {
            let mut best_match = StopToShapeMatch::new();
            let (closest, _closest_vec, distance) = closest_point_on_segment_vec(
                location,
                location_vec,
                segment.p1,
                segment.p1_vec,
                segment.p2,
                segment.p2_vec,
            );
            best_match.keep_best_match(closest, distance, segment.index);

            // Check neighbors in RTree just in case (optional, but safer)
            // Or simply iterate a few candidates if we wanted to be perfectly sure.
            // For now, let's rely on the nearest provided by RTree as a good candidate
            // but strictly we should probably query nearest N if we want to be 100% sure about projection.
            // Given the context of "stop matching", strict nearest is good.

            if best_match.has_best_match() {
                self.fill_location_match(&mut best_match);
            }
            return best_match;
        }

        // Fallback (shouldn't happen if not empty)
        StopToShapeMatch::new()
    }

    fn match_from_user_dist(
        &self,
        user_dist: f64,
        start_index: usize,
        stop_location: LatLng,
    ) -> StopToShapeMatch {
        self.interpolate(
            self.vertex_dist_from_user_dist(user_dist, start_index),
            stop_location,
        )
    }

    fn matches_from_location(
        &self,
        location: LatLng,
        max_distance_from_shape: f64,
    ) -> Vec<StopToShapeMatch> {
        let mut matches = Vec::new();
        let mut local_match = StopToShapeMatch::new();
        let mut distance_to_end_previous_segment = f64::INFINITY;
        let mut previous_segment_getting_further_away = false;

        // Calculate search envelope in degrees
        // 1 degree ~ 111,111 meters
        const METERS_PER_DEGREE: f64 = 111_111.0;
        let lat_delta = max_distance_from_shape / METERS_PER_DEGREE;
        // Adjust lon delta based on latitude (cos(lat))
        // Clamp cos_lat to avoid division by zero near poles, though unlikely for transit
        let cos_lat = location.lat.to_radians().cos().abs().max(0.01);
        let lon_delta = lat_delta / cos_lat;

        // Add a safety margin (e.g. 50%) to account for Haversine vs Euclidean difference
        let safety_factor = 1.5;
        let search_lat_delta = lat_delta * safety_factor;
        let search_lon_delta = lon_delta * safety_factor;

        let envelope = AABB::from_corners(
            [
                location.lat - search_lat_delta,
                location.lon - search_lon_delta,
            ],
            [
                location.lat + search_lat_delta,
                location.lon + search_lon_delta,
            ],
        );

        let location_vec = lat_lng_to_vec(location);

        // Query RTree for candidate segments
        let candidates = self.rtree.locate_in_envelope_intersecting(&envelope);

        // Process ONLY candidate segments, but we must sort them by index to maintain logic
        // "previous_segment_getting_further_away" relies on sequential processing.
        // So we collect candidates and sort by index.
        let mut sorted_candidates: Vec<&ShapeSegment> = candidates.collect();
        sorted_candidates.sort_by_key(|s| s.index);

        // Optimization: If candidates list is sparse/discontinuous, the logic
        // "distance_to_end_previous_segment" is tricky because it assumes continuity.
        // However, the original algorithm iterates ALL segments to detect local minima.
        // If we skip segments that are too far away, we just effectively ignore them.
        // But we need to be careful about "distance_to_end_previous_segment" logic.
        //
        // If we are skipping segments, we treat them as "infinite distance".
        // The logic relies on `previous_segment_getting_further_away` to group matches.

        let mut last_index = usize::MAX;

        for segment in sorted_candidates {
            // Skip duplicates if any
            if last_index != usize::MAX && segment.index == last_index {
                continue;
            }

            // Handle gap between segments (reset state if non-consecutive)
            if last_index != usize::MAX && segment.index > last_index + 1 {
                // Gap detected, reset local match state as if we started fresh or finished a group
                if local_match.has_best_match() {
                    matches.push(local_match.clone());
                    local_match.clear_best_match();
                }
                distance_to_end_previous_segment = f64::INFINITY;
                previous_segment_getting_further_away = false;
            }
            last_index = segment.index;

            let left = segment.p1;
            let left_vec = segment.p1_vec;
            let right = segment.p2;
            let right_vec = segment.p2_vec;
            let (closest, _closest_vec, geo_distance_to_shape) = closest_point_on_segment_vec(
                location,
                location_vec,
                left,
                left_vec,
                right,
                right_vec,
            );

            if geo_distance_to_shape <= max_distance_from_shape {
                if local_match.has_best_match()
                    && previous_segment_getting_further_away
                    && geo_distance_to_shape < distance_to_end_previous_segment
                {
                    matches.push(local_match.clone());
                    local_match.clear_best_match();
                }
                local_match.keep_best_match(closest, geo_distance_to_shape, segment.index);
            } else if local_match.has_best_match() {
                matches.push(local_match.clone());
                local_match.clear_best_match();
            }

            distance_to_end_previous_segment = distance_meters_vec(location_vec, right_vec);
            previous_segment_getting_further_away =
                distance_to_end_previous_segment > geo_distance_to_shape;
        }

        if local_match.has_best_match() {
            matches.push(local_match);
        }

        for match_result in matches.iter_mut() {
            self.fill_location_match(match_result);
        }
        matches
    }

    fn vertex_dist_from_user_dist(&self, user_dist: f64, start_index: usize) -> VertexDist {
        let mut previous_index = start_index;
        let mut next_index = start_index;
        while next_index < self.points.len() && user_dist >= self.points[next_index].user_distance {
            previous_index = next_index;
            next_index += 1;
        }
        if next_index == 0 || previous_index + 1 >= self.points.len() {
            return VertexDist {
                index: previous_index,
                fraction: 0.0,
            };
        }
        let prev_distance = self.points[previous_index].user_distance;
        let next_distance = self.points[next_index].user_distance;
        if near_by_fraction_or_margin(prev_distance, next_distance) {
            return VertexDist {
                index: previous_index,
                fraction: 0.0,
            };
        }
        VertexDist {
            index: previous_index,
            fraction: (user_dist - prev_distance) / (next_distance - prev_distance),
        }
    }

    fn interpolate(&self, vertex_dist: VertexDist, stop_location: LatLng) -> StopToShapeMatch {
        let prev_index = vertex_dist.index;
        let prev_point = self.points[prev_index];
        let next_point = if prev_index + 1 == self.points.len() {
            prev_point
        } else {
            self.points[prev_index + 1]
        };
        let fraction = vertex_dist.fraction;
        let location = if approx_equals(prev_point.location, next_point.location) {
            prev_point.location
        } else {
            slerp_lat_lng(prev_point.location, next_point.location, fraction)
        };
        StopToShapeMatch::from_parts(
            prev_index,
            prev_point.user_distance
                + fraction * (next_point.user_distance - prev_point.user_distance),
            prev_point.geo_distance
                + fraction * (next_point.geo_distance - prev_point.geo_distance),
            haversine_meters(stop_location, location),
            location,
        )
    }

    fn fill_location_match(&self, match_result: &mut StopToShapeMatch) {
        let shape_point = self.points[match_result.index];
        match_result.geo_distance = shape_point.geo_distance
            + haversine_meters(match_result.location, shape_point.location);
        match_result.user_distance = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct ShapePoint {
    geo_distance: f64,
    user_distance: f64,
    location: LatLng,
    #[allow(dead_code)]
    location_vec: Vec3,
}

impl ShapePoint {
    fn has_user_distance(&self) -> bool {
        self.user_distance > 0.0
    }
}

#[derive(Debug, Clone, Copy)]
struct VertexDist {
    index: usize,
    fraction: f64,
}

#[derive(Debug, Clone)]
struct StopToShapeMatch {
    index: usize,
    user_distance: f64,
    geo_distance: f64,
    geo_distance_to_shape: f64,
    location: LatLng,
}

impl StopToShapeMatch {
    fn new() -> Self {
        Self {
            index: 0,
            user_distance: 0.0,
            geo_distance: 0.0,
            geo_distance_to_shape: f64::INFINITY,
            location: LatLng { lat: 0.0, lon: 0.0 },
        }
    }

    fn from_parts(
        index: usize,
        user_distance: f64,
        geo_distance: f64,
        geo_distance_to_shape: f64,
        location: LatLng,
    ) -> Self {
        Self {
            index,
            user_distance,
            geo_distance,
            geo_distance_to_shape,
            location,
        }
    }

    fn clear_best_match(&mut self) {
        self.geo_distance_to_shape = f64::INFINITY;
    }

    fn keep_best_match(&mut self, location: LatLng, geo_distance_to_shape: f64, index: usize) {
        if geo_distance_to_shape < self.geo_distance_to_shape {
            self.geo_distance_to_shape = geo_distance_to_shape;
            self.location = location;
            self.index = index;
        }
    }

    fn has_best_match(&self) -> bool {
        self.geo_distance_to_shape.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProblemType {
    StopTooFarFromShape,
    StopHasTooManyMatches,
    StopsMatchOutOfOrder,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Problem<'a> {
    problem_type: ProblemType,
    stop_time: &'a gtfs_guru_model::StopTime,
    match_result: StopToShapeMatch,
    match_count: usize,
    prev_stop_time: Option<&'a gtfs_guru_model::StopTime>,
    prev_match: Option<StopToShapeMatch>,
}

impl<'a> Problem<'a> {
    fn stop_too_far_from_shape(
        stop_time: &'a gtfs_guru_model::StopTime,
        match_result: StopToShapeMatch,
    ) -> Self {
        Self {
            problem_type: ProblemType::StopTooFarFromShape,
            stop_time,
            match_result,
            match_count: 0,
            prev_stop_time: None,
            prev_match: None,
        }
    }

    fn stop_has_too_many_matches(
        stop_time: &'a gtfs_guru_model::StopTime,
        match_result: StopToShapeMatch,
        match_count: usize,
    ) -> Self {
        Self {
            problem_type: ProblemType::StopHasTooManyMatches,
            stop_time,
            match_result,
            match_count,
            prev_stop_time: None,
            prev_match: None,
        }
    }

    fn stop_match_out_of_order(
        stop_time: &'a gtfs_guru_model::StopTime,
        match_result: StopToShapeMatch,
        prev_stop_time: &'a gtfs_guru_model::StopTime,
        prev_match: StopToShapeMatch,
    ) -> Self {
        Self {
            problem_type: ProblemType::StopsMatchOutOfOrder,
            stop_time,
            match_result,
            match_count: 0,
            prev_stop_time: Some(prev_stop_time),
            prev_match: Some(prev_match),
        }
    }
}

fn find_best_matches<'a>(
    stop_points: &StopPoints<'a>,
    potential_matches: &[Vec<StopToShapeMatch>],
    problems: &mut Vec<Problem<'a>>,
) -> Vec<StopToShapeMatch> {
    let mut assignments = vec![Assignment::new()];
    let mut matches = Vec::new();

    for index in 0..potential_matches.len() {
        let next_assignments =
            construct_best_incremental_assignments(&potential_matches[index], &assignments);
        if next_assignments.is_empty() {
            if index > 0 {
                problems.push(construct_out_of_order_error(
                    stop_points,
                    potential_matches,
                    index,
                    &assignments,
                ));
            }
            return matches;
        }
        assignments = next_assignments;
    }

    let best_assignment = assignments
        .iter()
        .min_by(|a, b| cmp_f64(a.score, b.score))
        .map(|assignment| assignment.assignment.clone())
        .unwrap_or_default();

    for (index, match_idx) in best_assignment.into_iter().enumerate() {
        if let Some(match_result) = potential_matches
            .get(index)
            .and_then(|matches| matches.get(match_idx))
        {
            matches.push(match_result.clone());
        }
    }
    matches
}

fn construct_out_of_order_error<'a>(
    stop_points: &StopPoints<'a>,
    potential_matches: &[Vec<StopToShapeMatch>],
    index: usize,
    prev_assignments: &[Assignment],
) -> Problem<'a> {
    let match_result = potential_matches[index]
        .iter()
        .cloned()
        .min_by(|a, b| cmp_f64(a.geo_distance_to_shape, b.geo_distance_to_shape))
        .unwrap_or_else(StopToShapeMatch::new);
    let prev_assignment = prev_assignments
        .iter()
        .min_by(|a, b| cmp_f64(a.score, b.score))
        .map(|assignment| assignment.assignment.clone())
        .unwrap_or_default();
    let prev_match_index = prev_assignment.last().copied().unwrap_or(0);
    let prev_match = potential_matches[index - 1]
        .get(prev_match_index)
        .cloned()
        .unwrap_or_else(StopToShapeMatch::new);
    Problem::stop_match_out_of_order(
        stop_points.points[index].stop_time,
        match_result,
        stop_points.points[index - 1].stop_time,
        prev_match,
    )
}

fn construct_best_incremental_assignments(
    potential_matches: &[StopToShapeMatch],
    prev_assignments: &[Assignment],
) -> Vec<Assignment> {
    let mut next_assignments = Vec::new();
    for (idx, match_result) in potential_matches.iter().enumerate() {
        let mut best_index = None;
        let mut best_score = f64::INFINITY;
        for (prev_idx, prev) in prev_assignments.iter().enumerate() {
            if prev.max_geo_distance > match_result.geo_distance {
                continue;
            }
            if prev.score < best_score {
                best_index = Some(prev_idx);
                best_score = prev.score;
            }
        }
        if let Some(best_index) = best_index {
            let prev = &prev_assignments[best_index];
            let mut assignment = prev.assignment.clone();
            assignment.push(idx);
            next_assignments.push(Assignment {
                assignment,
                score: prev.score + match_result.geo_distance_to_shape,
                max_geo_distance: match_result.geo_distance,
            });
        }
    }
    next_assignments
}

#[derive(Debug, Clone)]
struct Assignment {
    assignment: Vec<usize>,
    score: f64,
    max_geo_distance: f64,
}

impl Assignment {
    fn new() -> Self {
        Self {
            assignment: Vec::new(),
            score: 0.0,
            max_geo_distance: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LatLng {
    lat: f64,
    lon: f64,
}

const EARTH_RADIUS_METERS: f64 = 6_371_010.0;

#[derive(Debug, Clone, Copy)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    fn normalize(self) -> Self {
        let norm = self.norm();
        if norm == 0.0 {
            return self;
        }
        Self {
            x: self.x / norm,
            y: self.y / norm,
            z: self.z / norm,
        }
    }

    fn scale(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

fn cmp_f64(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (true, true) => Ordering::Equal,
    }
}

fn stop_or_parent_location(
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_id: gtfs_guru_model::StringId,
) -> Option<LatLng> {
    let mut current_id = stop_id;
    for _ in 0..3 {
        let stop = match stops_by_id.get(&current_id) {
            Some(stop) => *stop,
            None => break,
        };
        if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
            return Some(LatLng { lat, lon });
        }
        let Some(parent_id) = stop.parent_station.filter(|id| id.0 != 0) else {
            break;
        };
        current_id = parent_id;
    }
    None
}

fn lat_lng(shape: &gtfs_guru_model::Shape) -> LatLng {
    LatLng {
        lat: shape.shape_pt_lat,
        lon: shape.shape_pt_lon,
    }
}

fn approx_equals(a: LatLng, b: LatLng) -> bool {
    (a.lat - b.lat).abs() < 1e-9 && (a.lon - b.lon).abs() < 1e-9
}

fn lat_lng_to_vec(point: LatLng) -> Vec3 {
    let lat = point.lat.to_radians();
    let lon = point.lon.to_radians();
    let cos_lat = lat.cos();
    Vec3 {
        x: cos_lat * lon.cos(),
        y: cos_lat * lon.sin(),
        z: lat.sin(),
    }
}

fn vec_to_lat_lng(point: Vec3) -> LatLng {
    let normalized = point.normalize();
    let lat = normalized.z.asin();
    let lon = normalized.y.atan2(normalized.x);
    LatLng {
        lat: lat.to_degrees(),
        lon: lon.to_degrees(),
    }
}

fn angular_distance(a: Vec3, b: Vec3) -> f64 {
    let cross = a.cross(b);
    let sin = cross.norm();
    let cos = a.dot(b);
    sin.atan2(cos)
}

fn distance_meters_vec(a: Vec3, b: Vec3) -> f64 {
    angular_distance(a, b) * EARTH_RADIUS_METERS
}

#[allow(dead_code)]
fn closest_point_on_segment(point: LatLng, left: LatLng, right: LatLng) -> (LatLng, f64) {
    let p = lat_lng_to_vec(point);
    let a = lat_lng_to_vec(left);
    let b = lat_lng_to_vec(right);
    let (res_latlng, _res_vec, dist) = closest_point_on_segment_vec(point, p, left, a, right, b);
    (res_latlng, dist)
}

fn closest_point_on_segment_vec(
    _point: LatLng,
    p: Vec3,
    left: LatLng,
    a: Vec3,
    right: LatLng,
    b: Vec3,
) -> (LatLng, Vec3, f64) {
    let n = a.cross(b);
    let n_norm = n.norm();
    if n_norm == 0.0 {
        let distance = distance_meters_vec(p, a);
        return (left, a, distance);
    }
    let n_unit = n.scale(1.0 / n_norm);
    let m = n_unit.cross(p);
    let m_norm = m.norm();
    if m_norm == 0.0 {
        let dist_a = distance_meters_vec(p, a);
        let dist_b = distance_meters_vec(p, b);
        return if dist_a <= dist_b {
            (left, a, dist_a)
        } else {
            (right, b, dist_b)
        };
    }
    let mut q = m.cross(n_unit).normalize();
    if q.dot(p) < 0.0 {
        q = q.neg();
    }

    let angle_ab = angular_distance(a, b);
    let angle_aq = angular_distance(a, q);
    let angle_qb = angular_distance(q, b);
    let on_segment = angle_aq + angle_qb <= angle_ab + 1e-12;
    let closest = if on_segment {
        q
    } else if angular_distance(a, p) <= angular_distance(b, p) {
        a
    } else {
        b
    };

    let matched = if closest.x == a.x && closest.y == a.y && closest.z == a.z {
        left
    } else if closest.x == b.x && closest.y == b.y && closest.z == b.z {
        right
    } else {
        vec_to_lat_lng(closest)
    };
    let distance = distance_meters_vec(p, closest);
    (matched, closest, distance)
}

fn haversine_meters(a: LatLng, b: LatLng) -> f64 {
    distance_meters_vec(lat_lng_to_vec(a), lat_lng_to_vec(b))
}

fn slerp_lat_lng(a: LatLng, b: LatLng, fraction: f64) -> LatLng {
    let a_vec = lat_lng_to_vec(a);
    let b_vec = lat_lng_to_vec(b);
    let mut dot = a_vec.dot(b_vec);
    if dot > 1.0 {
        dot = 1.0;
    } else if dot < -1.0 {
        dot = -1.0;
    }
    let theta = dot.acos();
    if theta.abs() < 1e-12 {
        return a;
    }
    let sin_theta = theta.sin();
    let w1 = ((1.0 - fraction) * theta).sin() / sin_theta;
    let w2 = (fraction * theta).sin() / sin_theta;
    vec_to_lat_lng(a_vec.scale(w1).add(b_vec.scale(w2)))
}

fn near_by_fraction_or_margin(x: f64, y: f64) -> bool {
    if x.is_infinite() || y.is_infinite() {
        return false;
    }
    let margin = 1e-9 * 32.0;
    let relative_margin = margin * x.abs().max(y.abs());
    (x - y).abs() <= margin.max(relative_margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Route, Shape, Stop, StopTime, Trip};

    #[test]
    fn detects_stop_too_far_from_shape() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".into(),
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![Default::default()],
            ..Default::default()
        };
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_type".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "stop_name".into(),
                "stop_lat".into(),
                "stop_lon".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_name: Some("Stop 1".into()),
                    stop_lat: Some(37.7749),
                    stop_lon: Some(-122.4194),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"), // Far from shape
                    stop_name: Some("Stop 2".into()),
                    stop_lat: Some(38.0),
                    stop_lon: Some(-122.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec![
                "shape_id".into(),
                "shape_pt_lat".into(),
                "shape_pt_lon".into(),
                "shape_pt_sequence".into(),
            ],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7749,
                    shape_pt_lon: -122.4194,
                    shape_pt_sequence: 1,
                    ..Default::default()
                },
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7750,
                    shape_pt_lon: -122.4195,
                    shape_pt_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        ShapeToStopMatchingValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STOP_TOO_FAR_FROM_SHAPE));
    }

    #[test]
    fn passes_when_stops_match_shape() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_type".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "stop_name".into(),
                "stop_lat".into(),
                "stop_lon".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_name: Some("Stop 1".into()),
                    stop_lat: Some(37.7749),
                    stop_lon: Some(-122.4194),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_name: Some("Stop 2".into()),
                    stop_lat: Some(37.7750),
                    stop_lon: Some(-122.4195),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec![
                "shape_id".into(),
                "shape_pt_lat".into(),
                "shape_pt_lon".into(),
                "shape_pt_sequence".into(),
            ],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7749,
                    shape_pt_lon: -122.4194,
                    shape_pt_sequence: 1,
                    ..Default::default()
                },
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7750,
                    shape_pt_lon: -122.4195,
                    shape_pt_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        ShapeToStopMatchingValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
    #[test]
    fn detects_out_of_order_stops() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_type".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // Stops are physically ordered: S1 (start), S2 (end)
        // But trip visits them in order: S2 -> S1 (reverse of shape)
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "stop_name".into(),
                "stop_lat".into(),
                "stop_lon".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_name: Some("Stop 1".into()),
                    stop_lat: Some(37.7749),
                    stop_lon: Some(-122.4194),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_name: Some("Stop 2".into()),
                    stop_lat: Some(37.7750),
                    stop_lon: Some(-122.4195),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec![
                "shape_id".into(),
                "shape_pt_lat".into(),
                "shape_pt_lon".into(),
                "shape_pt_sequence".into(),
            ],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7749,
                    shape_pt_lon: -122.4194, // Matches S1
                    shape_pt_sequence: 1,
                    ..Default::default()
                },
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    shape_pt_lat: 37.7750,
                    shape_pt_lon: -122.4195, // Matches S2
                    shape_pt_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                shape_id: Some(feed.pool.intern("SH1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // Trip is S2 (idx=2) -> S1 (idx=1)
        // Shape is S1 -> S2.
        // So S2 matches point 2, S1 matches point 1.
        // Sequence: distance(point 2) > distance(point 1) => DECREASING distance.
        // We expect out-of-order notice.
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        ShapeToStopMatchingValidator.validate(&feed, &mut notices);

        let notice = notices
            .iter()
            .find(|n| n.code == CODE_STOPS_MATCH_OUT_OF_ORDER);
        assert!(
            notice.is_some(),
            "Expected stops_match_shape_out_of_order notice"
        );
    }
}
