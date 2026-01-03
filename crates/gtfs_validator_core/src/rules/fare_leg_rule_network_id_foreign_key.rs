use std::collections::HashSet;

use crate::feed::FARE_LEG_RULES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

#[derive(Debug, Default)]
pub struct FareLegRuleNetworkIdForeignKeyValidator;

impl Validator for FareLegRuleNetworkIdForeignKeyValidator {
    fn name(&self) -> &'static str {
        "fare_leg_rule_network_id_foreign_key"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_leg_rules) = &feed.fare_leg_rules else {
            return;
        };

        let mut network_ids: HashSet<&str> = feed
            .routes
            .rows
            .iter()
            .filter_map(|route| route.network_id.as_deref())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect();
        if let Some(networks) = &feed.networks {
            for network in &networks.rows {
                let value = network.network_id.trim();
                if !value.is_empty() {
                    network_ids.insert(value);
                }
            }
        }

        for (index, rule) in fare_leg_rules.rows.iter().enumerate() {
            let row_number = fare_leg_rules.row_number(index);
            let Some(network_id) = rule
                .network_id
                .as_deref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if !network_ids.contains(network_id) {
                let mut notice = ValidationNotice::new(
                    CODE_FOREIGN_KEY_VIOLATION,
                    NoticeSeverity::Error,
                    "missing referenced network_id",
                );
                notice.insert_context_field("childFieldName", "network_id");
                notice.insert_context_field("childFilename", FARE_LEG_RULES_FILE);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldValue", network_id);
                notice.insert_context_field("parentFieldName", "network_id");
                notice.insert_context_field("parentFilename", "routes.txt or networks.txt");
                notice.field_order = vec![
                    "childFieldName".to_string(),
                    "childFilename".to_string(),
                    "csvRowNumber".to_string(),
                    "fieldValue".to_string(),
                    "parentFieldName".to_string(),
                    "parentFilename".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

