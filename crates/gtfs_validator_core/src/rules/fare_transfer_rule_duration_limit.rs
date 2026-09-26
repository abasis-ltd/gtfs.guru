use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_DURATION_LIMIT_WITHOUT_TYPE: &str = "fare_transfer_rule_duration_limit_without_type";
const CODE_TYPE_WITHOUT_DURATION_LIMIT: &str =
    "fare_transfer_rule_duration_limit_type_without_duration_limit";

#[derive(Debug, Default)]
pub struct FareTransferRuleDurationLimitTypeValidator;

impl Validator for FareTransferRuleDurationLimitTypeValidator {
    fn name(&self) -> &'static str {
        "fare_transfer_rule_duration_limit_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_transfer_rules) = &feed.fare_transfer_rules else {
            return;
        };

        for (index, rule) in fare_transfer_rules.rows.iter().enumerate() {
            let row_number = fare_transfer_rules.row_number(index);
            // A negative duration_limit fails to parse in the canonical
            // loader (number_out_of_range), which drops the whole row.
            if rule.duration_limit.is_some_and(|v| v < 0) {
                continue;
            }
            let has_duration_limit = rule.duration_limit.is_some();
            let has_duration_limit_type = rule.duration_limit_type.is_some();
            if has_duration_limit && !has_duration_limit_type {
                let mut notice = ValidationNotice::new(
                    CODE_DURATION_LIMIT_WITHOUT_TYPE,
                    NoticeSeverity::Error,
                    "duration_limit_type is required when duration_limit is set",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into()];
                notices.push(notice);
            }
            if !has_duration_limit && has_duration_limit_type {
                let mut notice = ValidationNotice::new(
                    CODE_TYPE_WITHOUT_DURATION_LIMIT,
                    NoticeSeverity::Error,
                    "duration_limit is required when duration_limit_type is set",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into()];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{DurationLimitType, FareTransferRule};

    #[test]
    fn detects_duration_limit_without_type() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec!["duration_limit".into()],
            rows: vec![FareTransferRule {
                duration_limit: Some(3600),
                duration_limit_type: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleDurationLimitTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_DURATION_LIMIT_WITHOUT_TYPE
        );
    }

    #[test]
    fn detects_type_without_duration_limit() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec!["duration_limit_type".into()],
            rows: vec![FareTransferRule {
                duration_limit: None,
                duration_limit_type: Some(DurationLimitType::DepartureToArrival),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleDurationLimitTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_TYPE_WITHOUT_DURATION_LIMIT
        );
    }

    #[test]
    fn skips_rows_with_negative_duration_limit() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec!["duration_limit".into(), "duration_limit_type".into()],
            rows: vec![
                FareTransferRule {
                    duration_limit: Some(-1),
                    duration_limit_type: Some(DurationLimitType::DepartureToArrival),
                    ..Default::default()
                },
                FareTransferRule {
                    duration_limit: None,
                    duration_limit_type: Some(DurationLimitType::DepartureToArrival),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleDurationLimitTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_TYPE_WITHOUT_DURATION_LIMIT);
        assert_eq!(notice.context["csvRowNumber"], 3);
    }

    #[test]
    fn passes_valid_combination() {
        let mut feed = GtfsFeed::default();
        feed.fare_transfer_rules = Some(CsvTable {
            headers: vec!["duration_limit".into(), "duration_limit_type".into()],
            rows: vec![FareTransferRule {
                duration_limit: Some(3600),
                duration_limit_type: Some(DurationLimitType::DepartureToArrival),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareTransferRuleDurationLimitTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
