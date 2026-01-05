use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_SHAPE: &str = "unused_shape";

#[derive(Debug, Default)]
pub struct ShapeUsageValidator;

impl Validator for ShapeUsageValidator {
    fn name(&self) -> &'static str {
        "shape_usage"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(shapes) = &feed.shapes else {
            return;
        };

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            use std::collections::HashMap;

            // 1. Collect used shapes
            let used_shapes: HashSet<&str> = feed
                .trips
                .rows
                .par_iter()
                .filter_map(|trip| trip.shape_id.as_deref())
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect();

            // 2. Map-Reduce to find first row for each shape
            let shapes_map: HashMap<&str, u64> = shapes
                .rows
                .par_iter()
                .enumerate()
                .fold(HashMap::<&str, u64>::new, |mut acc, (index, shape)| {
                    let shape_id = shape.shape_id.trim();
                    if !shape_id.is_empty() {
                        let row = shapes.row_number(index);
                        acc.entry(shape_id)
                            .and_modify(|r| *r = (*r).min(row))
                            .or_insert(row);
                    }
                    acc
                })
                .reduce(HashMap::<&str, u64>::new, |mut a, b| {
                    for (k, v) in b {
                        a.entry(k).and_modify(|r| *r = (*r).min(v)).or_insert(v);
                    }
                    a
                });

            // 3. Generate notices
            let results: Vec<ValidationNotice> = shapes_map
                .into_par_iter()
                .filter(|(shape_id, _)| !used_shapes.contains(shape_id))
                .map(|(shape_id, row_number)| {
                    let mut notice = ValidationNotice::new(
                        CODE_UNUSED_SHAPE,
                        NoticeSeverity::Warning,
                        "shape is not referenced in trips",
                    );
                    notice.insert_context_field("shapeId", shape_id);
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.field_order = vec!["csvRowNumber".into(), "shapeId".into()];
                    notice
                })
                .collect();

            for notice in results {
                notices.push(notice);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            let used_shapes: HashSet<&str> = feed
                .trips
                .rows
                .iter()
                .filter_map(|trip| trip.shape_id.as_deref())
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect();

            let mut reported: HashSet<&str> = HashSet::new();
            for (index, shape) in shapes.rows.iter().enumerate() {
                let row_number = shapes.row_number(index);
                let shape_id = shape.shape_id.trim();
                if shape_id.is_empty() {
                    continue;
                }
                if reported.insert(shape_id) && !used_shapes.contains(shape_id) {
                    let mut notice = ValidationNotice::new(
                        CODE_UNUSED_SHAPE,
                        NoticeSeverity::Warning,
                        "shape is not referenced in trips",
                    );
                    notice.insert_context_field("shapeId", shape_id);
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.field_order = vec!["csvRowNumber".into(), "shapeId".into()];
                    notices.push(notice);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Shape, Trip};

    #[test]
    fn detects_unused_shape() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![Shape {
                shape_id: "SH1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: "T1".into(),
                shape_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ShapeUsageValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_UNUSED_SHAPE));
    }

    #[test]
    fn passes_used_shape() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![Shape {
                shape_id: "SH1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "shape_id".into()],
            rows: vec![Trip {
                trip_id: "T1".into(),
                shape_id: Some("SH1".into()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ShapeUsageValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
