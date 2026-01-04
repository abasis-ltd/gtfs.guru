use std::collections::HashSet;

use crate::feed::{FARE_ATTRIBUTES_FILE, FARE_RULES_FILE, ROUTES_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

#[derive(Debug, Default)]
pub struct FareRulesValidator;

impl Validator for FareRulesValidator {
    fn name(&self) -> &'static str {
        "fare_rules_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let fare_attributes = match &feed.fare_attributes {
            Some(table) => Some(
                table
                    .rows
                    .iter()
                    .map(|fare| fare.fare_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect::<HashSet<_>>(),
            ),
            None => None,
        };
        let route_ids: HashSet<&str> = feed
            .routes
            .rows
            .iter()
            .map(|route| route.route_id.trim())
            .filter(|value| !value.is_empty())
            .collect();

        if feed.fare_rules.is_some() && fare_attributes.is_none() {
            notices.push_missing_file(FARE_ATTRIBUTES_FILE);
        }

        if let Some(fare_rules) = &feed.fare_rules {
            for (index, rule) in fare_rules.rows.iter().enumerate() {
                let row_number = fare_rules.row_number(index);
                if let Some(ref fare_ids) = fare_attributes {
                    let fare_id = rule.fare_id.trim();
                    if !fare_id.is_empty() && !fare_ids.contains(fare_id) {
                        notices.push(foreign_key_notice(
                            FARE_RULES_FILE,
                            "fare_id",
                            FARE_ATTRIBUTES_FILE,
                            "fare_id",
                            fare_id,
                            row_number,
                        ));
                    }
                }

                if let Some(route_id) = rule
                    .route_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !route_ids.contains(route_id) {
                        notices.push(foreign_key_notice(
                            FARE_RULES_FILE,
                            "route_id",
                            ROUTES_FILE,
                            "route_id",
                            route_id,
                            row_number,
                        ));
                    }
                }
            }
        }
    }
}

fn foreign_key_notice(
    child_file: &str,
    child_field: &str,
    parent_file: &str,
    parent_field: &str,
    id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FOREIGN_KEY_VIOLATION,
        NoticeSeverity::Error,
        format!("missing referenced id {}", id),
    );
    notice.row = Some(row_number);
    notice.field_order = vec![
        "childFieldName".to_string(),
        "childFilename".to_string(),
        "csvRowNumber".to_string(),
        "fieldValue".to_string(),
        "parentFieldName".to_string(),
        "parentFilename".to_string(),
    ];
    notice.insert_context_field("childFieldName", child_field);
    notice.insert_context_field("childFilename", child_file);
    notice.insert_context_field("parentFieldName", parent_field);
    notice.insert_context_field("parentFilename", parent_file);
    notice.insert_context_field("fieldValue", id);
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::{FareAttribute, FareRule, Route};

    #[test]
    fn detects_missing_fare_id() {
        let mut feed = GtfsFeed::default();
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".to_string()],
            rows: vec![FareAttribute {
                fare_id: "F1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.fare_rules = Some(CsvTable {
            headers: vec!["fare_id".to_string()],
            rows: vec![FareRule {
                fare_id: "UNKNOWN".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareRulesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FOREIGN_KEY_VIOLATION
        );
    }

    #[test]
    fn detects_missing_route_id() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.fare_rules = Some(CsvTable {
            headers: vec!["fare_id".to_string(), "route_id".to_string()],
            rows: vec![FareRule {
                fare_id: "F1".to_string(),
                route_id: Some("UNKNOWN".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareRulesValidator.validate(&feed, &mut notices);

        assert!(notices.len() >= 1);
        assert!(notices.iter().any(|n| n.code == CODE_FOREIGN_KEY_VIOLATION));
    }

    #[test]
    fn passes_valid_rule() {
        let mut feed = GtfsFeed::default();
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".to_string()],
            rows: vec![FareAttribute {
                fare_id: "F1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.fare_rules = Some(CsvTable {
            headers: vec!["fare_id".to_string(), "route_id".to_string()],
            rows: vec![FareRule {
                fare_id: "F1".to_string(),
                route_id: Some("R1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareRulesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
