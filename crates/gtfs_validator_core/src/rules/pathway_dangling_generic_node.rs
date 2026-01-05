use std::collections::HashSet;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_PATHWAY_DANGLING_GENERIC_NODE: &str = "pathway_dangling_generic_node";

#[derive(Debug, Default)]
pub struct PathwayDanglingGenericNodeValidator;

impl Validator for PathwayDanglingGenericNodeValidator {
    fn name(&self) -> &'static str {
        "pathway_dangling_generic_node"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            if stop.location_type != Some(LocationType::GenericNode) {
                continue;
            }
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }

            let mut incident_ids: HashSet<&str> = HashSet::new();
            for pathway in &pathways.rows {
                if pathway.from_stop_id.trim() == stop_id {
                    let to_id = pathway.to_stop_id.trim();
                    if !to_id.is_empty() {
                        incident_ids.insert(to_id);
                    }
                }
                if pathway.to_stop_id.trim() == stop_id {
                    let from_id = pathway.from_stop_id.trim();
                    if !from_id.is_empty() {
                        incident_ids.insert(from_id);
                    }
                }
            }

            if incident_ids.len() == 1 {
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_DANGLING_GENERIC_NODE,
                    NoticeSeverity::Warning,
                    "generic node is incident to only one pathway endpoint",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field(
                    "parentStation",
                    stop.parent_station.as_deref().unwrap_or(""),
                );
                notice.insert_context_field("stopId", stop_id);
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "parentStation".into(),
                    "stopId".into(),
                    "stopName".into(),
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
    use gtfs_guru_model::{LocationType, Pathway, Stop};

    #[test]
    fn detects_dangling_generic_node() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![Stop {
                stop_id: "G1".into(),
                location_type: Some(LocationType::GenericNode),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![Pathway {
                pathway_id: "P1".into(),
                from_stop_id: "G1".into(),
                to_stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayDanglingGenericNodeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_PATHWAY_DANGLING_GENERIC_NODE
        );
    }

    #[test]
    fn passes_when_node_has_two_pathways() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![Stop {
                stop_id: "G1".into(),
                location_type: Some(LocationType::GenericNode),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![
                Pathway {
                    pathway_id: "P1".into(),
                    from_stop_id: "G1".into(),
                    to_stop_id: "S1".into(),
                    ..Default::default()
                },
                Pathway {
                    pathway_id: "P2".into(),
                    from_stop_id: "S2".into(),
                    to_stop_id: "G1".into(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        PathwayDanglingGenericNodeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_non_generic_node() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: "S1".into(),
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Pathway {
                from_stop_id: "S1".into(),
                to_stop_id: "S2".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayDanglingGenericNodeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
