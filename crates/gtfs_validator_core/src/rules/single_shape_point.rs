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

        let mut counts: HashMap<gtfs_guru_model::StringId, (usize, u64)> = HashMap::new();
        for (index, shape) in shapes.rows.iter().enumerate() {
            let shape_id = shape.shape_id;
            if shape_id.0 == 0 {
                continue;
            }
            let entry = counts
                .entry(shape_id)
                .or_insert((0, shapes.row_number(index)));
            entry.0 += 1;
        }

        // File order keeps the output deterministic.
        let mut single: Vec<(u64, gtfs_guru_model::StringId)> = counts
            .into_iter()
            .filter(|(_, (count, _))| *count == 1)
            .map(|(shape_id, (_, row_number))| (row_number, shape_id))
            .collect();
        single.sort_unstable();

        for (row_number, shape_id) in single {
            let shape_id_value = feed.pool.resolve(shape_id);
            let mut notice = ValidationNotice::new(
                CODE_SINGLE_SHAPE_POINT,
                NoticeSeverity::Warning,
                "shape has a single point",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("shapeId", shape_id_value.as_str());
            notice.field_order = vec!["csvRowNumber".into(), "shapeId".into()];
            notices.push(notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::Shape;

    #[test]
    fn detects_single_shape_point() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![Shape {
                shape_id: feed.pool.intern("SH1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        SingleShapePointValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_SINGLE_SHAPE_POINT));
    }

    #[test]
    fn reports_single_point_shapes_in_file_order() {
        let mut feed = GtfsFeed::default();
        let ids = ["zs", "as", "ms", "bs", "ks", "cs"];
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: ids
                .iter()
                .map(|id| Shape {
                    shape_id: feed.pool.intern(id),
                    ..Default::default()
                })
                .collect(),
            row_numbers: (2..2 + ids.len() as u64).collect(),
        });

        let mut notices = NoticeContainer::new();
        SingleShapePointValidator.validate(&feed, &mut notices);

        let rows: Vec<u64> = notices
            .iter()
            .map(|n| n.context["csvRowNumber"].as_u64().unwrap())
            .collect();
        assert_eq!(rows, vec![2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn passes_multiple_shape_points() {
        let mut feed = GtfsFeed::default();
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into()],
            rows: vec![
                Shape {
                    shape_id: feed.pool.intern("SH1"),
                    ..Default::default()
                },
                Shape {
                    shape_id: feed.pool.intern("SH1"),
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
