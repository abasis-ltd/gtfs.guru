use std::collections::HashSet;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;
use gtfs_guru_model::StringId;

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
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }

            let mut incident_ids: HashSet<StringId> = HashSet::new();
            for pathway in &pathways.rows {
                if pathway.from_stop_id == stop_id {
                    let to_id = pathway.to_stop_id;
                    if to_id.0 != 0 {
                        incident_ids.insert(to_id);
                    }
                }
                if pathway.to_stop_id == stop_id {
                    let from_id = pathway.from_stop_id;
                    if from_id.0 != 0 {
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
                    feed.pool
                        .resolve(stop.parent_station.unwrap_or(StringId(0)))
                        .as_str(),
                );
                notice.insert_context_field("stopId", feed.pool.resolve(stop_id).as_str());
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
                stop_id: feed.pool.intern("G1"),
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
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("G1"),
                to_stop_id: feed.pool.intern("S1"),
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
                stop_id: feed.pool.intern("G1"),
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
                    pathway_id: feed.pool.intern("P1"),
                    from_stop_id: feed.pool.intern("G1"),
                    to_stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Pathway {
                    pathway_id: feed.pool.intern("P2"),
                    from_stop_id: feed.pool.intern("S2"),
                    to_stop_id: feed.pool.intern("G1"),
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
                stop_id: feed.pool.intern("S1"),
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Pathway {
                from_stop_id: feed.pool.intern("S1"),
                to_stop_id: feed.pool.intern("S2"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayDanglingGenericNodeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
