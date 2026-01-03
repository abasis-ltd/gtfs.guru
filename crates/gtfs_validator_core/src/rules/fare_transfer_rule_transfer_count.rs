use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_INVALID_TRANSFER_COUNT: &str = "fare_transfer_rule_invalid_transfer_count";
const CODE_MISSING_TRANSFER_COUNT: &str = "fare_transfer_rule_missing_transfer_count";
const CODE_FORBIDDEN_TRANSFER_COUNT: &str = "fare_transfer_rule_with_forbidden_transfer_count";

#[derive(Debug, Default)]
pub struct FareTransferRuleTransferCountValidator;

impl Validator for FareTransferRuleTransferCountValidator {
    fn name(&self) -> &'static str {
        "fare_transfer_rule_transfer_count"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_transfer_rules) = &feed.fare_transfer_rules else {
            return;
        };

        for (index, rule) in fare_transfer_rules.rows.iter().enumerate() {
            let row_number = fare_transfer_rules.row_number(index);
            let from_leg_group_id = normalized(rule.from_leg_group_id.as_deref());
            let to_leg_group_id = normalized(rule.to_leg_group_id.as_deref());
            let has_transfer_count = rule.transfer_count.is_some();

            if let (Some(from_id), Some(to_id)) = (from_leg_group_id, to_leg_group_id) {
                if from_id == to_id {
                    if let Some(transfer_count) = rule.transfer_count {
                        if transfer_count < -1 || transfer_count == 0 {
                            notices.push(invalid_transfer_count_notice(row_number, transfer_count));
                        }
                    } else {
                        notices.push(missing_transfer_count_notice(row_number));
                    }
                    continue;
                }
            }

            if has_transfer_count {
                notices.push(forbidden_transfer_count_notice(row_number));
            }
        }
    }
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(|val| val.trim()).filter(|val| !val.is_empty())
}

fn invalid_transfer_count_notice(row_number: u64, transfer_count: i32) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_INVALID_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count has an invalid value",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("transferCount", transfer_count);
    notice.field_order = vec!["csvRowNumber".to_string(), "transferCount".to_string()];
    notice
}

fn missing_transfer_count_notice(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count is required when from_leg_group_id equals to_leg_group_id",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["csvRowNumber".to_string()];
    notice
}

fn forbidden_transfer_count_notice(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count is forbidden when leg group ids differ",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["csvRowNumber".to_string()];
    notice
}

