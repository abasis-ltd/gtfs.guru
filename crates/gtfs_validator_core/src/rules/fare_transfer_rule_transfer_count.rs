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
            let from_leg_group_id = rule.from_leg_group_id.filter(|id| id.0 != 0);
            let to_leg_group_id = rule.to_leg_group_id.filter(|id| id.0 != 0);
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

fn invalid_transfer_count_notice(row_number: u64, transfer_count: i32) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_INVALID_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count has an invalid value",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("transferCount", transfer_count);
    notice.field_order = vec!["csvRowNumber".into(), "transferCount".into()];
    notice
}

fn missing_transfer_count_notice(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count is required when from_leg_group_id equals to_leg_group_id",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["csvRowNumber".into()];
    notice
}

fn forbidden_transfer_count_notice(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_TRANSFER_COUNT,
        NoticeSeverity::Error,
        "transfer_count is forbidden when leg group ids differ",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["csvRowNumber".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::FareTransferRule;

    #[test]
    fn detects_invalid_transfer_count() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec![
                "from_leg_group_id".into(),
                "to_leg_group_id".into(),
                "transfer_count".into(),
            ],
            rows: vec![FareTransferRule {
                from_leg_group_id: Some(feed.pool.intern("G1")),
                to_leg_group_id: Some(feed.pool.intern("G1")),
                transfer_count: Some(0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleTransferCountValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_INVALID_TRANSFER_COUNT
        );
    }

    #[test]
    fn detects_missing_transfer_count() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec!["from_leg_group_id".into(), "to_leg_group_id".into()],
            rows: vec![FareTransferRule {
                from_leg_group_id: Some(feed.pool.intern("G1")),
                to_leg_group_id: Some(feed.pool.intern("G1")),
                transfer_count: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleTransferCountValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_TRANSFER_COUNT
        );
    }

    #[test]
    fn detects_forbidden_transfer_count() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec![
                "from_leg_group_id".into(),
                "to_leg_group_id".into(),
                "transfer_count".into(),
            ],
            rows: vec![FareTransferRule {
                from_leg_group_id: Some(feed.pool.intern("G1")),
                to_leg_group_id: Some(feed.pool.intern("G2")),
                transfer_count: Some(1),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleTransferCountValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_TRANSFER_COUNT
        );
    }

    #[test]
    fn passes_valid_combination() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec![
                "from_leg_group_id".into(),
                "to_leg_group_id".into(),
                "transfer_count".into(),
            ],
            rows: vec![
                FareTransferRule {
                    from_leg_group_id: Some(feed.pool.intern("G1")),
                    to_leg_group_id: Some(feed.pool.intern("G1")),
                    transfer_count: Some(1),
                    ..Default::default()
                },
                FareTransferRule {
                    from_leg_group_id: Some(feed.pool.intern("G1")),
                    to_leg_group_id: Some(feed.pool.intern("G2")),
                    transfer_count: None,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleTransferCountValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
