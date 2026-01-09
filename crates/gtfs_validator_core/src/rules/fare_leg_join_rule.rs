use std::collections::HashSet;

use crate::feed::FARE_LEG_JOIN_RULES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";
const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";

#[derive(Debug, Default)]
pub struct FareLegJoinRuleValidator;

impl Validator for FareLegJoinRuleValidator {
    fn name(&self) -> &'static str {
        "fare_leg_join_rule"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_leg_join_rules) = &feed.fare_leg_join_rules else {
            return;
        };

        let mut network_ids: HashSet<gtfs_guru_model::StringId> = feed
            .routes
            .rows
            .iter()
            .filter_map(|route| route.network_id)
            .filter(|id| id.0 != 0)
            .collect();
        if let Some(networks) = &feed.networks {
            for network in &networks.rows {
                if network.network_id.0 != 0 {
                    network_ids.insert(network.network_id);
                }
            }
        }

        for (index, rule) in fare_leg_join_rules.rows.iter().enumerate() {
            let row_number = fare_leg_join_rules.row_number(index);
            let from_network_id = rule.from_network_id;
            if from_network_id.0 != 0 && !network_ids.contains(&from_network_id) {
                let id_value = feed.pool.resolve(from_network_id);
                notices.push(missing_ref_notice(
                    "from_network_id",
                    id_value.as_str(),
                    row_number,
                ));
            }

            let to_network_id = rule.to_network_id;
            if to_network_id.0 != 0 && !network_ids.contains(&to_network_id) {
                let id_value = feed.pool.resolve(to_network_id);
                notices.push(missing_ref_notice(
                    "to_network_id",
                    id_value.as_str(),
                    row_number,
                ));
            }

            let from_stop_id = rule.from_stop_id.filter(|id| id.0 != 0);
            let to_stop_id = rule.to_stop_id.filter(|id| id.0 != 0);
            if from_stop_id.is_some() && to_stop_id.is_none() {
                notices.push(missing_required_field_notice("to_stop_id", row_number));
            } else if to_stop_id.is_some() && from_stop_id.is_none() {
                notices.push(missing_required_field_notice("from_stop_id", row_number));
            }
        }
    }
}

fn missing_ref_notice(field: &str, id: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FOREIGN_KEY_VIOLATION,
        NoticeSeverity::Error,
        format!("missing referenced id {}", id),
    );
    notice.insert_context_field("childFieldName", field);
    notice.insert_context_field("childFilename", FARE_LEG_JOIN_RULES_FILE);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldValue", id);
    notice.insert_context_field("parentFieldName", "network_id");
    notice.insert_context_field("parentFilename", "routes.txt or networks.txt");
    notice.field_order = vec![
        "childFieldName".into(),
        "childFilename".into(),
        "csvRowNumber".into(),
        "fieldValue".into(),
        "parentFieldName".into(),
        "parentFilename".into(),
    ];
    notice
}

fn missing_required_field_notice(field: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_REQUIRED_FIELD,
        NoticeSeverity::Error,
        "required field is missing",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("filename", FARE_LEG_JOIN_RULES_FILE);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{FareLegJoinRule, Network, Route};

    #[test]
    fn detects_missing_network_id() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "network_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                network_id: Some(feed.pool.intern("N1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.fare_leg_join_rules = Some(CsvTable {
            headers: vec!["from_network_id".into()],
            rows: vec![FareLegJoinRule {
                from_network_id: feed.pool.intern("UNKNOWN"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareLegJoinRuleValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FOREIGN_KEY_VIOLATION
        );
    }

    #[test]
    fn detects_missing_required_stop_id() {
        let mut feed = GtfsFeed::default();
        feed.fare_leg_join_rules = Some(CsvTable {
            headers: vec!["from_stop_id".into()],
            rows: vec![FareLegJoinRule {
                from_stop_id: Some(feed.pool.intern("S1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareLegJoinRuleValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_REQUIRED_FIELD
        );
    }

    #[test]
    fn passes_valid_rule() {
        let mut feed = GtfsFeed::default();
        feed.networks = Some(CsvTable {
            headers: vec!["network_id".into()],
            rows: vec![Network {
                network_id: feed.pool.intern("N1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.fare_leg_join_rules = Some(CsvTable {
            headers: vec![
                "from_network_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![FareLegJoinRule {
                from_network_id: feed.pool.intern("N1"),
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareLegJoinRuleValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
