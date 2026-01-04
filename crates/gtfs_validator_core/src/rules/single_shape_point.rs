use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_SINGLE_SHAPE_POINT: &str = "single_shape_point";

#[derive(Debug, Default)]
pub struct SingleShapePointValidator;

impl Validator for SingleShapePointValidator {
    fn name(&self) -> &'static str {
        "single_shape_point"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(shapes) = &feed.shapes else {
            return;
        };

        let mut counts: HashMap<&str, (usize, u64)> = HashMap::new();
        for (index, shape) in shapes.rows.iter().enumerate() {
            let shape_id = shape.shape_id.trim();
            if shape_id.is_empty() {
                continue;
            }
            let entry = counts
                .entry(shape_id)
                .or_insert((0, shapes.row_number(index)));
            entry.0 += 1;
        }

        for (shape_id, (count, row_number)) in counts {
            if count == 1 {
                let mut notice = ValidationNotice::new(
                    CODE_SINGLE_SHAPE_POINT,
                    NoticeSeverity::Warning,
                    "shape has a single point",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("shapeId", shape_id);
                notice.field_order = vec!["csvRowNumber".to_string(), "shapeId".to_string()];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::Shape;

    #[test]
    fn detects_single_shape_point() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".to_string()],
            rows: vec![Shape {
                shape_id: "SH1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        SingleShapePointValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_SINGLE_SHAPE_POINT));
    }

    #[test]
    fn passes_multiple_shape_points() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".to_string()],
            rows: vec![
                Shape {
                    shape_id: "SH1".to_string(),
                    ..Default::default()
                },
                Shape {
                    shape_id: "SH1".to_string(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        SingleShapePointValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
