use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_PATHWAY_LOOP: &str = "pathway_loop";

#[derive(Debug, Default)]
pub struct PathwayLoopValidator;

impl Validator for PathwayLoopValidator {
    fn name(&self) -> &'static str {
        "pathway_loop"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        for (index, pathway) in pathways.rows.iter().enumerate() {
            let row_number = pathways.row_number(index);
            let from_id = pathway.from_stop_id.trim();
            let to_id = pathway.to_stop_id.trim();
            if from_id.is_empty() || to_id.is_empty() {
                continue;
            }
            if from_id == to_id {
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_LOOP,
                    NoticeSeverity::Warning,
                    "pathway from_stop_id and to_stop_id must be different",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("pathwayId", pathway.pathway_id.as_str());
                notice.insert_context_field("stopId", from_id);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "pathwayId".to_string(),
                    "stopId".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

