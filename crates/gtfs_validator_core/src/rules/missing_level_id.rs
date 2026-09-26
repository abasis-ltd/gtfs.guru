use std::collections::HashSet;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::PathwayMode;

const CODE_MISSING_LEVEL_ID: &str = "missing_level_id";

/// Both ends of an elevator pathway need a `level_id`, whether or not the feed
/// ships levels.txt: the canonical validator checks elevators only, and only
/// against stops.txt.
#[derive(Debug, Default)]
pub struct MissingLevelIdValidator;

impl Validator for MissingLevelIdValidator {
    fn name(&self) -> &'static str {
        "missing_level_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        let mut elevator_stop_ids: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        for pathway in &pathways.rows {
            if pathway.pathway_mode != PathwayMode::Elevator {
                continue;
            }
            for stop_id in [pathway.from_stop_id, pathway.to_stop_id] {
                if stop_id.0 != 0 {
                    elevator_stop_ids.insert(stop_id);
                }
            }
        }

        if elevator_stop_ids.is_empty() {
            return;
        }

        // Stops file order keeps the output deterministic; a repeated stop_id
        // is judged by its first row, as the canonical id index keeps it.
        let mut seen: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let stop_id = stop.stop_id;
            if !elevator_stop_ids.contains(&stop_id) || !seen.insert(stop_id) {
                continue;
            }
            let has_level_id = stop.level_id.map(|id| id.0 != 0).unwrap_or(false);
            if has_level_id {
                continue;
            }
            let stop_id_value = feed.pool.resolve(stop_id);
            let mut notice = ValidationNotice::new(
                CODE_MISSING_LEVEL_ID,
                NoticeSeverity::Error,
                "stops.level_id is required for stops connected by an elevator pathway",
            );
            notice.insert_context_field("csvRowNumber", feed.stops.row_number(index));
            notice.insert_context_field("stopId", stop_id_value.as_str());
            notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
            notice.field_order = vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
            notices.push(notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Level, Pathway, Stop};

    #[test]
    fn detects_missing_level_id_when_levels_present() {
        let mut feed = GtfsFeed::default();
        feed.levels = Some(CsvTable {
            headers: vec!["level_id".into()],
            rows: vec![Level {
                level_id: feed.pool.intern("L1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
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
                pathway_mode: PathwayMode::Elevator,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    level_id: None,
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    level_id: Some(feed.pool.intern("L1")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        MissingLevelIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_MISSING_LEVEL_ID);
        assert_eq!(
            notices.iter().next().unwrap().message,
            "stops.level_id is required for stops connected by an elevator pathway"
        );
    }

    #[test]
    fn ignores_non_elevator_pathways() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "pathway_mode".into()],
            rows: vec![Pathway {
                from_stop_id: feed.pool.intern("S1"),
                pathway_mode: PathwayMode::Stairs,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                level_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        MissingLevelIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn detects_elevator_stops_without_levels_file() {
        let mut feed = GtfsFeed::default();
        feed.levels = None;
        feed.pathways = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "pathway_mode".into(),
            ],
            rows: vec![Pathway {
                from_stop_id: feed.pool.intern("E1"),
                to_stop_id: feed.pool.intern("N1"),
                pathway_mode: PathwayMode::Elevator,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("N1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("E1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![6, 7],
        };

        let mut notices = NoticeContainer::new();
        MissingLevelIdValidator.validate(&feed, &mut notices);

        let rows: Vec<u64> = notices
            .iter()
            .map(|n| n.context["csvRowNumber"].as_u64().unwrap())
            .collect();
        assert_eq!(rows, vec![6, 7]);
    }

    #[test]
    fn passes_when_stop_not_in_pathway() {
        let mut feed = GtfsFeed::default();
        feed.levels = Some(CsvTable {
            headers: vec!["level_id".into()],
            rows: vec![Level::default()],
            row_numbers: vec![2],
        });
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".into()],
            rows: vec![Pathway {
                from_stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S2"),
                level_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        MissingLevelIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
