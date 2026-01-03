use crate::feed::PATHWAYS_FILE;
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
                    && !matches!(pathway.pathway_mode, gtfs_model::PathwayMode::ExitGate)
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

                if matches!(pathway.pathway_mode, gtfs_model::PathwayMode::Stairs)
                    && pathway.stair_count.is_none()
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

