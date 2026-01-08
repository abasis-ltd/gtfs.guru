use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_DECREASING_SHAPE_DISTANCE: &str = "decreasing_shape_distance";
const CODE_EQUAL_SHAPE_DISTANCE_SAME_COORDS: &str = "equal_shape_distance_same_coordinates";
const CODE_EQUAL_SHAPE_DISTANCE_DIFF_COORDS: &str = "equal_shape_distance_diff_coordinates";
const CODE_EQUAL_SHAPE_DISTANCE_DIFF_COORDS_BELOW_THRESHOLD: &str =
    "equal_shape_distance_diff_coordinates_distance_below_threshold";
const DISTANCE_THRESHOLD_METERS: f64 = 20.0;

#[derive(Debug, Default)]
pub struct ShapeIncreasingDistanceValidator;

impl Validator for ShapeIncreasingDistanceValidator {
    fn name(&self) -> &'static str {
        "shape_increasing_distance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(shapes) = &feed.shapes {
            let mut by_shape: HashMap<
                gtfs_guru_model::StringId,
                Vec<(u64, &gtfs_guru_model::Shape)>,
            > = HashMap::new();
            for (index, shape) in shapes.rows.iter().enumerate() {
                let row_number = shapes.row_number(index);
                let shape_id = shape.shape_id;
                if shape_id.0 == 0 {
                    continue;
                }
                by_shape
                    .entry(shape_id)
                    .or_default()
                    .push((row_number, shape));
            }
            for shape_points in by_shape.values_mut() {
                shape_points.sort_by_key(|(_, shape)| shape.shape_pt_sequence);
            }

            for shape_points in by_shape.values() {
                for window in shape_points.windows(2) {
                    let (prev_row, prev) = window[0];
                    let (curr_row, curr) = window[1];
                    let (Some(prev_dist), Some(curr_dist)) =
                        (prev.shape_dist_traveled, curr.shape_dist_traveled)
                    else {
                        continue;
                    };

                    if prev_dist > curr_dist {
                        notices.push(shape_notice(
                            CODE_DECREASING_SHAPE_DISTANCE,
                            NoticeSeverity::Error,
                            "shape_dist_traveled decreases within shape_id",
                            prev_row,
                            prev,
                            curr_row,
                            curr,
                            prev_dist,
                            curr_dist,
                            None,
                            feed,
                        ));
                        continue;
                    }

                    if prev_dist != curr_dist {
                        continue;
                    }

                    if prev.shape_pt_lat == curr.shape_pt_lat
                        && prev.shape_pt_lon == curr.shape_pt_lon
                    {
                        notices.push(shape_notice(
                            CODE_EQUAL_SHAPE_DISTANCE_SAME_COORDS,
                            NoticeSeverity::Warning,
                            "equal shape_dist_traveled with identical coordinates",
                            prev_row,
                            prev,
                            curr_row,
                            curr,
                            prev_dist,
                            curr_dist,
                            None,
                            feed,
                        ));
                        continue;
                    }

                    let distance = haversine_meters(
                        prev.shape_pt_lat,
                        prev.shape_pt_lon,
                        curr.shape_pt_lat,
                        curr.shape_pt_lon,
                    );
                    if distance >= DISTANCE_THRESHOLD_METERS {
                        notices.push(shape_notice(
                            CODE_EQUAL_SHAPE_DISTANCE_DIFF_COORDS,
                            NoticeSeverity::Error,
                            "equal shape_dist_traveled with different coordinates",
                            prev_row,
                            prev,
                            curr_row,
                            curr,
                            prev_dist,
                            curr_dist,
                            Some(distance),
                            feed,
                        ));
                    } else if distance > 0.0 {
                        notices.push(shape_notice(
                            CODE_EQUAL_SHAPE_DISTANCE_DIFF_COORDS_BELOW_THRESHOLD,
                            NoticeSeverity::Warning,
                            "equal shape_dist_traveled with near-identical coordinates",
                            prev_row,
                            prev,
                            curr_row,
                            curr,
                            prev_dist,
                            curr_dist,
                            Some(distance),
                            feed,
                        ));
                    }
                }
            }
        }
    }
}

fn shape_notice(
    code: &str,
    severity: NoticeSeverity,
    message: &str,
    prev_row: u64,
    prev: &gtfs_guru_model::Shape,
    row_number: u64,
    curr: &gtfs_guru_model::Shape,
    prev_dist: f64,
    curr_dist: f64,
    distance: Option<f64>,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let shape_id_value = feed.pool.resolve(curr.shape_id);
    let mut notice = ValidationNotice::new(code, severity, message);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("prevCsvRowNumber", prev_row);
    notice.insert_context_field("prevShapeDistTraveled", prev_dist);
    notice.insert_context_field("prevShapePtSequence", prev.shape_pt_sequence);
    notice.insert_context_field("shapeDistTraveled", curr_dist);
    notice.insert_context_field("shapeId", shape_id_value.as_str());
    notice.insert_context_field("shapePtSequence", curr.shape_pt_sequence);
    if let Some(distance) = distance {
        notice.insert_context_field("actualDistanceBetweenShapePoints", distance);
        notice.field_order = vec![
            "actualDistanceBetweenShapePoints".into(),
            "csvRowNumber".into(),
            "prevCsvRowNumber".into(),
            "prevShapeDistTraveled".into(),
            "prevShapePtSequence".into(),
            "shapeDistTraveled".into(),
            "shapeId".into(),
            "shapePtSequence".into(),
        ];
    } else {
        notice.field_order = vec![
            "csvRowNumber".into(),
            "prevCsvRowNumber".into(),
            "prevShapeDistTraveled".into(),
            "prevShapePtSequence".into(),
            "shapeDistTraveled".into(),
            "shapeId".into(),
            "shapePtSequence".into(),
        ];
    }
    notice
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
    use gtfs_guru_model::Shape;

    #[test]
    fn detects_decreasing_shape_distance() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.0,
                    shape_pt_lon: -74.0,
                    shape_pt_sequence: 1,
                    shape_dist_traveled: Some(100.0),
                },
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.01,
                    shape_pt_lon: -74.01,
                    shape_pt_sequence: 2,
                    shape_dist_traveled: Some(50.0),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        ShapeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_DECREASING_SHAPE_DISTANCE
        );
    }

    #[test]
    fn detects_equal_distance_same_coords() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.0,
                    shape_pt_lon: -74.0,
                    shape_pt_sequence: 1,
                    shape_dist_traveled: Some(100.0),
                },
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.0,
                    shape_pt_lon: -74.0,
                    shape_pt_sequence: 2,
                    shape_dist_traveled: Some(100.0),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        ShapeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_EQUAL_SHAPE_DISTANCE_SAME_COORDS
        );
    }

    #[test]
    fn passes_with_increasing_distance() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.0,
                    shape_pt_lon: -74.0,
                    shape_pt_sequence: 1,
                    shape_dist_traveled: Some(0.0),
                },
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.01,
                    shape_pt_lon: -74.01,
                    shape_pt_sequence: 2,
                    shape_dist_traveled: Some(100.0),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        ShapeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn passes_without_shape_dist_traveled() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.0,
                    shape_pt_lon: -74.0,
                    shape_pt_sequence: 1,
                    shape_dist_traveled: None,
                },
                Shape {
                    shape_id: feed.pool.intern("S1"),
                    shape_pt_lat: 40.01,
                    shape_pt_lon: -74.01,
                    shape_pt_sequence: 2,
                    shape_dist_traveled: None,
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        ShapeIncreasingDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
