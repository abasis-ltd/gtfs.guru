use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_LEVEL_ID: &str = "missing_level_id";

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
        // Rule: If levels.txt is present, level_id is required for stops in pathways.
        if feed.levels.is_none() {
            return;
        }

        let mut pathway_stop_ids: HashSet<&str> = HashSet::new();
        for pathway in &pathways.rows {
            let from_id = pathway.from_stop_id.trim();
            if !from_id.is_empty() {
                pathway_stop_ids.insert(from_id);
            }
            let to_id = pathway.to_stop_id.trim();
            if !to_id.is_empty() {
                pathway_stop_ids.insert(to_id);
            }
        }

        if pathway_stop_ids.is_empty() {
            return;
        }

        let mut stops_by_id: HashMap<&str, &gtfs_guru_model::Stop> = HashMap::new();
        let mut rows_by_id: HashMap<&str, u64> = HashMap::new();
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            rows_by_id.insert(stop_id, feed.stops.row_number(index));
        }

        let mut sorted_stop_ids: Vec<&str> = pathway_stop_ids.into_iter().collect();
        sorted_stop_ids.sort();

        for stop_id in sorted_stop_ids {
            let Some(stop) = stops_by_id.get(stop_id) else {
                continue;
            };
            let has_level_id = stop
                .level_id
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            if !has_level_id {
                let row_number = rows_by_id.get(stop_id).copied().unwrap_or(2);
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_LEVEL_ID,
                    NoticeSeverity::Error,
                    "stops.level_id is required when levels.txt is present and stop is part of a pathway",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopId", stop_id);
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "stopId".to_string(),
                    "stopName".to_string(),
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
    use gtfs_guru_model::{Level, Pathway, Stop};

    #[test]
    fn detects_missing_level_id_when_levels_present() {
        let mut feed = GtfsFeed::default();
        feed.levels = Some(CsvTable {
            headers: vec!["level_id".to_string()],
            rows: vec![Level {
                level_id: "L1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".to_string(),
                "from_stop_id".to_string(),
                "to_stop_id".to_string(),
            ],
            rows: vec![Pathway {
                pathway_id: "P1".to_string(),
                from_stop_id: "S1".to_string(),
                to_stop_id: "S2".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    level_id: None,
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(),
                    level_id: Some("L1".to_string()),
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
            "stops.level_id is required when levels.txt is present and stop is part of a pathway"
        );
    }

    #[test]
    fn passes_when_levels_missing() {
        let mut feed = GtfsFeed::default();
        feed.levels = None;
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".to_string()],
            rows: vec![Pathway {
                from_stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
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
    fn passes_when_stop_not_in_pathway() {
        let mut feed = GtfsFeed::default();
        feed.levels = Some(CsvTable {
            headers: vec!["level_id".to_string()],
            rows: vec![Level::default()],
            row_numbers: vec![2],
        });
        feed.pathways = Some(CsvTable {
            headers: vec!["from_stop_id".to_string()],
            rows: vec![Pathway {
                from_stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![Stop {
                stop_id: "S2".to_string(),
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
