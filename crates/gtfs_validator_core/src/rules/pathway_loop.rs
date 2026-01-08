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
            let from_id = pathway.from_stop_id;
            let to_id = pathway.to_stop_id;
            if from_id.0 == 0 || to_id.0 == 0 {
                continue;
            }
            if from_id == to_id {
                let pathway_id = feed.pool.resolve(pathway.pathway_id);
                let stop_id = feed.pool.resolve(from_id);
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_LOOP,
                    NoticeSeverity::Warning,
                    "pathway from_stop_id and to_stop_id must be different",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("pathwayId", pathway_id.as_str());
                notice.insert_context_field("stopId", stop_id.as_str());
                notice.field_order =
                    vec!["csvRowNumber".into(), "pathwayId".into(), "stopId".into()];
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
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("S1"),
                to_stop_id: feed.pool.intern("S1"),
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
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("S1"),
                to_stop_id: feed.pool.intern("S2"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayLoopValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
