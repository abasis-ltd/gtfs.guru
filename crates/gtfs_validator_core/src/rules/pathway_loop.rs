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
                    "csvRowNumber".into(),
                    "pathwayId".into(),
                    "stopId".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::Pathway;

    #[test]
    fn detects_pathway_loop() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![Pathway {
                pathway_id: "P1".into(),
                from_stop_id: "S1".into(),
                to_stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayLoopValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_PATHWAY_LOOP);
    }

    #[test]
    fn passes_normal_pathway() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![Pathway {
                pathway_id: "P1".into(),
                from_stop_id: "S1".into(),
                to_stop_id: "S2".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayLoopValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
