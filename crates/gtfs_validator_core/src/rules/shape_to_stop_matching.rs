use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::RouteType;

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

        let mut stops_by_id = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        let mut routes_by_id = HashMap::new();
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

        let mut trip_rows = HashMap::new();
        let mut trips_by_shape: HashMap<&str, Vec<&gtfs_model::Trip>> = HashMap::new();
        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let trip_id = trip.trip_id.trim();
            if !trip_id.is_empty() {
                trip_rows.insert(trip_id, feed.trips.row_number(index));
            }
            let Some(shape_id) = trip.shape_id.as_deref() else {
                continue;
            };
            let shape_id = shape_id.trim();
            if shape_id.is_empty() {
                continue;
            }
            trips_by_shape.entry(shape_id).or_default().push(trip);
        }

        let mut shapes_by_id: HashMap<&str, Vec<&gtfs_model::Shape>> = HashMap::new();
        for shape in &shapes.rows {
            let shape_id = shape.shape_id.trim();
            if shape_id.is_empty() {
                continue;
            }
            shapes_by_id.entry(shape_id).or_default().push(shape);
        }

        let matcher = StopToShapeMatcher::default();
        for (shape_id, shape_points_raw) in shapes_by_id {
            let trips = match trips_by_shape.get(shape_id) {
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
                let trip_id = trip.trip_id.trim();
                if trip_id.is_empty() {
                    continue;
                }
                let stop_times = match stop_times_by_trip.get(trip_id) {
                    Some(stop_times) if !stop_times.is_empty() => stop_times,
                    _ => continue,
                };
                if !processed_trip_hashes.insert(trip_hash(stop_times)) {
                    continue;
                }
                let route_id = trip.route_id.trim();
                if route_id.is_empty() {
                    continue;
                }
                let route = match routes_by_id.get(route_id) {
                    Some(route) => *route,
                    None => continue,
                };

                let station_size = StopPoints::route_type_to_station_size(route.route_type);
                let stop_points =
                    StopPoints::from_stop_times(stop_times, &stops_by_id, station_size);
                let geo_result = matcher.match_using_geo_distance(&stop_points, &shape_points);
                let trip_row_number = trip_rows.get(trip_id).copied().unwrap_or(2);
                let shape_id = trip.shape_id.as_deref().unwrap_or("").trim();
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
                    );
                }
            }
        }
    }
}

fn report_problems(
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    problems: &[Problem<'_>],
    matching_distance: MatchingDistance,
    reported_stop_ids: &mut HashSet<String>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    shape_points: &ShapePoints,
    notices: &mut NoticeContainer,
) {
    for problem in problems {
        let stop_id = problem.stop_time.stop_id.trim();
        if stop_id.is_empty() {
            continue;
        }
        if problem.problem_type == ProblemType::StopTooFarFromShape
            && !reported_stop_ids.insert(stop_id.to_string())
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
        ));
    }
}

fn problem_notice(
    problem: &Problem<'_>,
    matching_distance: MatchingDistance,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    shape_points: &ShapePoints,
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
            ),
            MatchingDistance::User => stop_too_far_from_shape_user_notice(
                problem,
                trip,
                trip_row_number,
                shape_id,
                stops_by_id,
                stop_time_rows,
                shape_points,
            ),
        },
        ProblemType::StopHasTooManyMatches => stop_has_too_many_matches_notice(
            problem,
            trip,
            trip_row_number,
            shape_id,
            stops_by_id,
            stop_time_rows,
        ),
        ProblemType::StopsMatchOutOfOrder => stops_match_out_of_order_notice(
            problem,
            trip,
            trip_row_number,
            shape_id,
            stops_by_id,
            stop_time_rows,
        ),
    }
}

fn stop_too_far_from_shape_notice(
    problem: &Problem<'_>,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    shape_points: &ShapePoints,
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
    );
    notice
}

fn stop_too_far_from_shape_user_notice(
    problem: &Problem<'_>,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    shape_points: &ShapePoints,
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
    );
    notice
}

fn stop_has_too_many_matches_notice(
    problem: &Problem<'_>,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOP_HAS_TOO_MANY_MATCHES,
        NoticeSeverity::Warning,
        "stop has too many matches for shape",
    );
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id);
    notice.insert_context_field("tripId", trip.trip_id.as_str());
    notice.insert_context_field(
        "stopTimeCsvRowNumber",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field("stopId", problem.stop_time.stop_id.as_str());
    notice.insert_context_field(
        "stopName",
        stop_name_by_id(stops_by_id, &problem.stop_time.stop_id),
    );
    notice.insert_context_field("match", lat_lng_array(problem.match_result.location));
    notice.insert_context_field("matchCount", problem.match_count);
    notice.field_order = vec![
        "tripCsvRowNumber".to_string(),
        "shapeId".to_string(),
        "tripId".to_string(),
        "stopTimeCsvRowNumber".to_string(),
        "stopId".to_string(),
        "stopName".to_string(),
        "match".to_string(),
        "matchCount".to_string(),
    ];
    notice
}

fn stops_match_out_of_order_notice(
    problem: &Problem<'_>,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STOPS_MATCH_OUT_OF_ORDER,
        NoticeSeverity::Warning,
        "stops match shape out of order",
    );
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id);
    notice.insert_context_field("tripId", trip.trip_id.as_str());
    let prev_stop_time = problem.prev_stop_time.unwrap_or(problem.stop_time);
    let prev_match = problem.prev_match.as_ref().unwrap_or(&problem.match_result);
    notice.insert_context_field(
        "stopTimeCsvRowNumber1",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field("stopId1", problem.stop_time.stop_id.as_str());
    notice.insert_context_field(
        "stopName1",
        stop_name_by_id(stops_by_id, &problem.stop_time.stop_id),
    );
    notice.insert_context_field("match1", lat_lng_array(problem.match_result.location));
    notice.insert_context_field(
        "stopTimeCsvRowNumber2",
        stop_time_row(stop_time_rows, prev_stop_time),
    );
    notice.insert_context_field("stopId2", prev_stop_time.stop_id.as_str());
    notice.insert_context_field(
        "stopName2",
        stop_name_by_id(stops_by_id, &prev_stop_time.stop_id),
    );
    notice.insert_context_field("match2", lat_lng_array(prev_match.location));
    notice.field_order = vec![
        "tripCsvRowNumber".to_string(),
        "shapeId".to_string(),
        "tripId".to_string(),
        "stopTimeCsvRowNumber1".to_string(),
        "stopId1".to_string(),
        "stopName1".to_string(),
        "match1".to_string(),
        "stopTimeCsvRowNumber2".to_string(),
        "stopId2".to_string(),
        "stopName2".to_string(),
        "match2".to_string(),
    ];
    notice
}

fn populate_stop_too_far_notice(
    notice: &mut ValidationNotice,
    problem: &Problem<'_>,
    trip: &gtfs_model::Trip,
    trip_row_number: u64,
    shape_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    shape_points: &ShapePoints,
) {
    notice.insert_context_field("tripCsvRowNumber", trip_row_number);
    notice.insert_context_field("shapeId", shape_id);
    notice.insert_context_field("tripId", trip.trip_id.as_str());
    notice.insert_context_field(
        "stopTimeCsvRowNumber",
        stop_time_row(stop_time_rows, problem.stop_time),
    );
    notice.insert_context_field("stopId", problem.stop_time.stop_id.as_str());
    notice.insert_context_field(
        "stopName",
        stop_name_by_id(stops_by_id, &problem.stop_time.stop_id),
    );
    // Include the actual stop location for map visualization
    if let Some(stop_location) = stop_location_by_id(stops_by_id, &problem.stop_time.stop_id) {
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
        "tripCsvRowNumber".to_string(),
        "shapeId".to_string(),
        "tripId".to_string(),
        "stopTimeCsvRowNumber".to_string(),
        "stopId".to_string(),
        "stopName".to_string(),
        "stopLocation".to_string(),
        "match".to_string(),
        "geoDistanceToShape".to_string(),
        "shapePath".to_string(),
        "matchIndex".to_string(),
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
    stop_time_rows: &HashMap<*const gtfs_model::StopTime, u64>,
    stop_time: &gtfs_model::StopTime,
) -> u64 {
    stop_time_rows
        .get(&(stop_time as *const _))
        .copied()
        .unwrap_or(2)
}

fn trip_hash(stop_times: &[&gtfs_model::StopTime]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stop_times.len().hash(&mut hasher);
    for stop_time in stop_times {
        stop_time.stop_id.len().hash(&mut hasher);
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
    stops_by_id: &'a HashMap<&str, &'a gtfs_model::Stop>,
    stop_id: &str,
) -> &'a str {
    let key = stop_id.trim();
    if key.is_empty() {
        return "";
    }
    stops_by_id
        .get(key)
        .and_then(|stop| stop.stop_name.as_deref())
        .unwrap_or("")
}

fn stop_location_by_id(
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_id: &str,
) -> Option<LatLng> {
    let key = stop_id.trim();
    if key.is_empty() {
        return None;
    }
    stops_by_id.get(key).and_then(|stop| {
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
            potential_matches_for_stop_problem_threshold: 1,
        }
    }
}

#[derive(Debug, Clone)]
struct StopPoints<'a> {
    points: Vec<StopPoint<'a>>,
}

impl<'a> StopPoints<'a> {
    fn from_stop_times(
        stop_times: &[&'a gtfs_model::StopTime],
        stops_by_id: &HashMap<&str, &'a gtfs_model::Stop>,
        station_size: StationSize,
    ) -> Self {
        let mut points = Vec::with_capacity(stop_times.len());
        for stop_time in stop_times.iter() {
            let stop_id = stop_time.stop_id.trim();
            if stop_id.is_empty() {
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
    stop_time: &'a gtfs_model::StopTime,
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
}

impl ShapePoints {
    fn from_shapes(mut shapes: Vec<&gtfs_model::Shape>) -> Self {
        shapes.sort_by_key(|shape| shape.shape_pt_sequence);
        let mut points = Vec::with_capacity(shapes.len());
        let mut geo_distance = 0.0_f64;
        let mut user_distance = 0.0_f64;
        for (idx, shape) in shapes.iter().enumerate() {
            if idx > 0 {
                let prev = shapes[idx - 1];
                geo_distance += haversine_meters(lat_lng(prev), lat_lng(shape)).max(0.0);
            }
            user_distance = user_distance.max(shape.shape_dist_traveled.unwrap_or(0.0));
            points.push(ShapePoint {
                geo_distance,
                user_distance,
                location: lat_lng(shape),
            });
        }
        Self { points }
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
        let mut best_match = StopToShapeMatch::new();
        if self.points.len() == 1 {
            let closest = self.points[0].location;
            best_match.keep_best_match(closest, haversine_meters(location, closest), 0);
        }
        for index in 0..self.points.len().saturating_sub(1) {
            let left = self.points[index].location;
            let right = self.points[index + 1].location;
            let (closest, distance) = closest_point_on_segment(location, left, right);
            best_match.keep_best_match(closest, distance, index);
        }
        if best_match.has_best_match() {
            self.fill_location_match(&mut best_match);
        }
        best_match
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

        for index in 0..self.points.len().saturating_sub(1) {
            let left = self.points[index].location;
            let right = self.points[index + 1].location;
            let (closest, geo_distance_to_shape) = closest_point_on_segment(location, left, right);

            if geo_distance_to_shape <= max_distance_from_shape {
                if local_match.has_best_match()
                    && (previous_segment_getting_further_away
                        || (geo_distance_to_shape == 0.0
                            && distance_to_end_previous_segment == 0.0))
                {
                    matches.push(local_match.clone());
                    local_match.clear_best_match();
                }
                local_match.keep_best_match(closest, geo_distance_to_shape, index);
            } else if local_match.has_best_match() {
                matches.push(local_match.clone());
                local_match.clear_best_match();
            }

            distance_to_end_previous_segment = haversine_meters(location, right);
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
    stop_time: &'a gtfs_model::StopTime,
    match_result: StopToShapeMatch,
    match_count: usize,
    prev_stop_time: Option<&'a gtfs_model::StopTime>,
    prev_match: Option<StopToShapeMatch>,
}

impl<'a> Problem<'a> {
    fn stop_too_far_from_shape(
        stop_time: &'a gtfs_model::StopTime,
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
        stop_time: &'a gtfs_model::StopTime,
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
        stop_time: &'a gtfs_model::StopTime,
        match_result: StopToShapeMatch,
        prev_stop_time: &'a gtfs_model::StopTime,
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
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_id: &str,
) -> Option<LatLng> {
    let mut current_id = stop_id;
    for _ in 0..3 {
        let stop = match stops_by_id.get(current_id) {
            Some(stop) => *stop,
            None => break,
        };
        if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
            return Some(LatLng { lat, lon });
        }
        let Some(parent_id) = stop
            .parent_station
            .as_deref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            break;
        };
        current_id = parent_id;
    }
    None
}

fn lat_lng(shape: &gtfs_model::Shape) -> LatLng {
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

fn closest_point_on_segment(point: LatLng, left: LatLng, right: LatLng) -> (LatLng, f64) {
    let p = lat_lng_to_vec(point);
    let a = lat_lng_to_vec(left);
    let b = lat_lng_to_vec(right);
    let n = a.cross(b);
    let n_norm = n.norm();
    if n_norm == 0.0 {
        let distance = distance_meters_vec(p, a);
        return (left, distance);
    }
    let n_unit = n.scale(1.0 / n_norm);
    let m = n_unit.cross(p);
    let m_norm = m.norm();
    if m_norm == 0.0 {
        let dist_a = distance_meters_vec(p, a);
        let dist_b = distance_meters_vec(p, b);
        return if dist_a <= dist_b {
            (left, dist_a)
        } else {
            (right, dist_b)
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

    let matched = vec_to_lat_lng(closest);
    let distance = distance_meters_vec(p, closest);
    (matched, distance)
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
    use gtfs_model::{Route, Shape, Stop, StopTime, Trip};

    #[test]
    fn detects_stop_too_far_from_shape() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".to_string(),
                "agency_name".to_string(),
                "agency_url".to_string(),
                "agency_timezone".to_string(),
            ],
            rows: vec![Default::default()],
            ..Default::default()
        };
        feed.routes = CsvTable {
            headers: vec![
                "route_id".to_string(),
                "route_short_name".to_string(),
                "route_type".to_string(),
            ],
            rows: vec![Route {
                route_id: "R1".to_string(),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".to_string(),
                "stop_name".to_string(),
                "stop_lat".to_string(),
                "stop_lon".to_string(),
            ],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    stop_name: Some("Stop 1".to_string()),
                    stop_lat: Some(37.7749),
                    stop_lon: Some(-122.4194),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(), // Far from shape
                    stop_name: Some("Stop 2".to_string()),
                    stop_lat: Some(38.0),
                    stop_lon: Some(-122.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec![
                "shape_id".to_string(),
                "shape_pt_lat".to_string(),
                "shape_pt_lon".to_string(),
                "shape_pt_sequence".to_string(),
            ],
            rows: vec![
                Shape {
                    shape_id: "SH1".to_string(),
                    shape_pt_lat: 37.7749,
                    shape_pt_lon: -122.4194,
                    shape_pt_sequence: 1,
                    ..Default::default()
                },
                Shape {
                    shape_id: "SH1".to_string(),
                    shape_pt_lat: 37.7750,
                    shape_pt_lon: -122.4195,
                    shape_pt_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.trips = CsvTable {
            headers: vec![
                "route_id".to_string(),
                "trip_id".to_string(),
                "shape_id".to_string(),
            ],
            rows: vec![Trip {
                route_id: "R1".to_string(),
                trip_id: "T1".to_string(),
                shape_id: Some("SH1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
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
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

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
                "route_id".to_string(),
                "route_short_name".to_string(),
                "route_type".to_string(),
            ],
            rows: vec![Route {
                route_id: "R1".to_string(),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".to_string(),
                "stop_name".to_string(),
                "stop_lat".to_string(),
                "stop_lon".to_string(),
            ],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    stop_name: Some("Stop 1".to_string()),
                    stop_lat: Some(37.7749),
                    stop_lon: Some(-122.4194),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(),
                    stop_name: Some("Stop 2".to_string()),
                    stop_lat: Some(37.7750),
                    stop_lon: Some(-122.4195),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.shapes = Some(CsvTable {
            headers: vec![
                "shape_id".to_string(),
                "shape_pt_lat".to_string(),
                "shape_pt_lon".to_string(),
                "shape_pt_sequence".to_string(),
            ],
            rows: vec![
                Shape {
                    shape_id: "SH1".to_string(),
                    shape_pt_lat: 37.7749,
                    shape_pt_lon: -122.4194,
                    shape_pt_sequence: 1,
                    ..Default::default()
                },
                Shape {
                    shape_id: "SH1".to_string(),
                    shape_pt_lat: 37.7750,
                    shape_pt_lon: -122.4195,
                    shape_pt_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.trips = CsvTable {
            headers: vec![
                "route_id".to_string(),
                "trip_id".to_string(),
                "shape_id".to_string(),
            ],
            rows: vec![Trip {
                route_id: "R1".to_string(),
                trip_id: "T1".to_string(),
                shape_id: Some("SH1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
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
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_id: "S2".to_string(),
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
}
