use crate::feed::PATHWAYS_FILE;
use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_RECOMMENDED_FIELD: &str = "missing_recommended_field";
const CODE_NUMBER_OUT_OF_RANGE: &str = "number_out_of_range";

#[derive(Debug, Default)]
pub struct PathwaysValidator;

impl Validator for PathwaysValidator {
    fn name(&self) -> &'static str {
        "pathways_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(pathways) = &feed.pathways {
            for (index, pathway) in pathways.rows.iter().enumerate() {
                let row_number = pathways.row_number(index);
                if pathway.length.is_none()
                    && !matches!(pathway.pathway_mode, gtfs_guru_model::PathwayMode::ExitGate)
                    && thorough_mode_enabled()
                {
                    let mut notice = ValidationNotice::new(
                        CODE_MISSING_RECOMMENDED_FIELD,
                        NoticeSeverity::Warning,
                        "pathway length is missing",
                    );
                    notice.set_location(PATHWAYS_FILE, "length", row_number);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "fieldName".to_string(),
                        "filename".to_string(),
                    ];
                    notices.push(notice);
                }

                if let Some(traversal) = pathway.traversal_time {
                    if traversal == 0 {
                        notices.push(number_out_of_range_notice(
                            "traversal_time",
                            row_number,
                            "integer",
                            traversal,
                        ));
                    }
                }

                if matches!(pathway.pathway_mode, gtfs_guru_model::PathwayMode::Stairs)
                    && pathway.stair_count.is_none()
                    && thorough_mode_enabled()
                {
                    let mut notice = ValidationNotice::new(
                        CODE_MISSING_RECOMMENDED_FIELD,
                        NoticeSeverity::Warning,
                        "stair_count should be provided for stairs",
                    );
                    notice.set_location(PATHWAYS_FILE, "stair_count", row_number);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "fieldName".to_string(),
                        "filename".to_string(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

fn number_out_of_range_notice(
    field: &str,
    row_number: u64,
    field_type: &str,
    field_value: u32,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_NUMBER_OUT_OF_RANGE,
        NoticeSeverity::Error,
        "value out of range",
    );
    notice.set_location(PATHWAYS_FILE, field, row_number);
    notice.insert_context_field("fieldType", field_type);
    notice.insert_context_field("fieldValue", field_value);
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldType".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Pathway, PathwayMode};

    #[test]
    fn detects_missing_length_for_non_exit_gate() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec!["pathway_id".to_string(), "pathway_mode".to_string()],
            rows: vec![Pathway {
                pathway_id: "P1".to_string(),
                pathway_mode: PathwayMode::Walkway,
                length: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwaysValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_RECOMMENDED_FIELD && n.message.contains("length")));
    }

    #[test]
    fn detects_zero_traversal_time() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec!["pathway_id".to_string(), "traversal_time".to_string()],
            rows: vec![Pathway {
                pathway_id: "P1".to_string(),
                traversal_time: Some(0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwaysValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_NUMBER_OUT_OF_RANGE));
    }

    #[test]
    fn detects_missing_stair_count_for_stairs() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec!["pathway_id".to_string(), "pathway_mode".to_string()],
            rows: vec![Pathway {
                pathway_id: "P1".to_string(),
                pathway_mode: PathwayMode::Stairs,
                stair_count: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwaysValidator.validate(&feed, &mut notices);

        assert!(
            notices
                .iter()
                .any(|n| n.code == CODE_MISSING_RECOMMENDED_FIELD
                    && n.message.contains("stair_count"))
        );
    }

    #[test]
    fn passes_valid_pathway() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".to_string(),
                "pathway_mode".to_string(),
                "length".to_string(),
                "traversal_time".to_string(),
            ],
            rows: vec![Pathway {
                pathway_id: "P1".to_string(),
                pathway_mode: PathwayMode::Walkway,
                length: Some(10.0),
                traversal_time: Some(5),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwaysValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
