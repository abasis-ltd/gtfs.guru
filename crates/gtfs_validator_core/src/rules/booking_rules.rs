use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{BookingRules, BookingType};

const CODE_FORBIDDEN_REALTIME_FIELDS: &str = "forbidden_real_time_booking_field_value";
const CODE_FORBIDDEN_SAME_DAY_FIELDS: &str = "forbidden_same_day_booking_field_value";
const CODE_FORBIDDEN_PRIOR_DAY_FIELDS: &str = "forbidden_prior_day_booking_field_value";
const CODE_INVALID_PRIOR_NOTICE_DURATION_MIN: &str = "invalid_prior_notice_duration_min";
const CODE_MISSING_PRIOR_NOTICE_DURATION_MIN: &str = "missing_prior_notice_duration_min";
const CODE_FORBIDDEN_PRIOR_NOTICE_START_DAY: &str = "forbidden_prior_notice_start_day";
const CODE_PRIOR_NOTICE_LAST_DAY_AFTER_START_DAY: &str = "prior_notice_last_day_after_start_day";
const CODE_FORBIDDEN_PRIOR_NOTICE_START_TIME: &str = "forbidden_prior_notice_start_time";
const CODE_MISSING_PRIOR_NOTICE_START_TIME: &str = "missing_prior_notice_start_time";
const CODE_MISSING_PRIOR_NOTICE_LAST_DAY: &str = "missing_prior_notice_last_day";
const CODE_MISSING_PRIOR_NOTICE_LAST_TIME: &str = "missing_prior_notice_last_time";

#[derive(Debug, Default)]
pub struct BookingRulesEntityValidator;

impl Validator for BookingRulesEntityValidator {
    fn name(&self) -> &'static str {
        "booking_rules_entity"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(booking_rules) = &feed.booking_rules else {
            return;
        };

        for (index, rule) in booking_rules.rows.iter().enumerate() {
            let row_number = booking_rules.row_number(index);
            let booking_rule_id = feed.pool.resolve(rule.booking_rule_id);
            let booking_rule_id = booking_rule_id.as_str();
            validate_booking_type(rule, booking_rule_id, row_number, notices);
            validate_prior_notice_duration_min(rule, booking_rule_id, row_number, notices);
            validate_prior_notice_start_day(rule, booking_rule_id, row_number, notices);
            validate_prior_notice_day_range(rule, row_number, notices);
            validate_missing_prior_day_fields(rule, booking_rule_id, row_number, notices);
            validate_prior_notice_start_time(rule, booking_rule_id, row_number, notices);
        }
    }
}

fn validate_booking_type(
    rule: &BookingRules,
    booking_rule_id: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    match rule.booking_type {
        BookingType::Realtime => {
            let forbidden = find_forbidden_fields_realtime(rule);
            if !forbidden.is_empty() {
                notices.push(forbidden_realtime_fields_notice(
                    "REALTIME",
                    booking_rule_id,
                    &forbidden,
                    row_number,
                ));
            }
        }
        BookingType::SameDay => {
            let forbidden = find_forbidden_fields_same_day(rule);
            if !forbidden.is_empty() {
                notices.push(forbidden_same_day_fields_notice(
                    "SAMEDAY",
                    booking_rule_id,
                    &forbidden,
                    row_number,
                ));
            }
        }
        BookingType::PriorDay => {
            let forbidden = find_forbidden_fields_prior_day(rule);
            if !forbidden.is_empty() {
                notices.push(forbidden_prior_day_fields_notice(
                    "PRIORDAY",
                    booking_rule_id,
                    &forbidden,
                    row_number,
                ));
            }
        }
        BookingType::Other => {}
    }
}

fn validate_prior_notice_duration_min(
    rule: &BookingRules,
    booking_rule_id: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    if rule.booking_type == BookingType::SameDay && rule.prior_notice_duration_min.is_none() {
        notices.push(missing_prior_notice_duration_min_notice(
            booking_rule_id,
            row_number,
        ));
    }

    if let (Some(min), Some(max)) = (
        rule.prior_notice_duration_min,
        rule.prior_notice_duration_max,
    ) {
        if max < min {
            notices.push(invalid_duration_min_notice(
                booking_rule_id,
                min,
                max,
                row_number,
            ));
        }
    }
}

fn validate_prior_notice_start_day(
    rule: &BookingRules,
    booking_rule_id: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    if rule.prior_notice_duration_max.is_some() && rule.prior_notice_start_day.is_some() {
        notices.push(forbidden_prior_notice_start_day_notice(
            booking_rule_id,
            row_number,
            rule.prior_notice_duration_max,
            rule.prior_notice_start_day,
        ));
    }
}

fn validate_prior_notice_day_range(
    rule: &BookingRules,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    if let (Some(last_day), Some(start_day)) =
        (rule.prior_notice_last_day, rule.prior_notice_start_day)
    {
        if last_day > start_day {
            notices.push(prior_notice_last_day_after_start_day_notice(
                row_number, last_day, start_day,
            ));
        }
    }
}

fn validate_missing_prior_day_fields(
    rule: &BookingRules,
    booking_rule_id: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    if rule.booking_type != BookingType::PriorDay {
        return;
    }

    if rule.prior_notice_last_day.is_none() {
        notices.push(missing_prior_notice_last_day_notice(
            booking_rule_id,
            row_number,
        ));
    }
    if rule.prior_notice_last_time.is_none() {
        notices.push(missing_prior_notice_last_time_notice(
            booking_rule_id,
            row_number,
        ));
    }
}

fn validate_prior_notice_start_time(
    rule: &BookingRules,
    booking_rule_id: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    match (
        rule.prior_notice_start_time.is_some(),
        rule.prior_notice_start_day.is_some(),
    ) {
        (true, false) => notices.push(forbidden_prior_notice_start_time_notice(
            booking_rule_id,
            row_number,
            rule.prior_notice_start_time,
        )),
        (false, true) => notices.push(missing_prior_notice_start_time_notice(
            booking_rule_id,
            row_number,
            rule.prior_notice_start_day,
        )),
        _ => {}
    }
}

fn find_forbidden_fields_realtime(rule: &BookingRules) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if rule.prior_notice_duration_min.is_some() {
        fields.push("prior_notice_duration_min");
    }
    if rule.prior_notice_duration_max.is_some() {
        fields.push("prior_notice_duration_max");
    }
    if rule.prior_notice_last_day.is_some() {
        fields.push("prior_notice_last_day");
    }
    if rule.prior_notice_last_time.is_some() {
        fields.push("prior_notice_last_time");
    }
    if rule.prior_notice_start_day.is_some() {
        fields.push("prior_notice_start_day");
    }
    if rule.prior_notice_start_time.is_some() {
        fields.push("prior_notice_start_time");
    }
    if rule.prior_notice_service_id.is_some() {
        fields.push("prior_notice_service_id");
    }
    fields
}

fn find_forbidden_fields_same_day(rule: &BookingRules) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if rule.prior_notice_last_day.is_some() {
        fields.push("prior_notice_last_day");
    }
    if rule.prior_notice_last_time.is_some() {
        fields.push("prior_notice_last_time");
    }
    if rule.prior_notice_service_id.is_some() {
        fields.push("prior_notice_service_id");
    }
    fields
}

fn find_forbidden_fields_prior_day(rule: &BookingRules) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if rule.prior_notice_duration_min.is_some() {
        fields.push("prior_notice_duration_min");
    }
    if rule.prior_notice_duration_max.is_some() {
        fields.push("prior_notice_duration_max");
    }
    fields
}

fn forbidden_realtime_fields_notice(
    booking_type: &str,
    booking_rule_id: &str,
    fields: &[&'static str],
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_REALTIME_FIELDS,
        NoticeSeverity::Error,
        format!(
            "booking_type {} forbids fields for booking_rule_id {}: {}",
            booking_type,
            booking_rule_id,
            fields.join(", ")
        ),
    );
    populate_forbidden_fields_notice(&mut notice, booking_rule_id, fields, row_number);
    notice
}

fn forbidden_same_day_fields_notice(
    booking_type: &str,
    booking_rule_id: &str,
    fields: &[&'static str],
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_SAME_DAY_FIELDS,
        NoticeSeverity::Error,
        format!(
            "booking_type {} forbids fields for booking_rule_id {}: {}",
            booking_type,
            booking_rule_id,
            fields.join(", ")
        ),
    );
    populate_forbidden_fields_notice(&mut notice, booking_rule_id, fields, row_number);
    notice
}

fn forbidden_prior_day_fields_notice(
    booking_type: &str,
    booking_rule_id: &str,
    fields: &[&'static str],
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_PRIOR_DAY_FIELDS,
        NoticeSeverity::Error,
        format!(
            "booking_type {} forbids fields for booking_rule_id {}: {}",
            booking_type,
            booking_rule_id,
            fields.join(", ")
        ),
    );
    populate_forbidden_fields_notice(&mut notice, booking_rule_id, fields, row_number);
    notice
}

fn populate_forbidden_fields_notice(
    notice: &mut ValidationNotice,
    booking_rule_id: &str,
    fields: &[&'static str],
    row_number: u64,
) {
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "fieldNames",
        fields
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    );
    notice.field_order = vec![
        "bookingRuleId".into(),
        "csvRowNumber".into(),
        "fieldNames".into(),
    ];
}

fn invalid_duration_min_notice(
    booking_rule_id: &str,
    min: i32,
    max: i32,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_INVALID_PRIOR_NOTICE_DURATION_MIN,
        NoticeSeverity::Error,
        format!(
            "prior_notice_duration_max {} is less than prior_notice_duration_min {}",
            max, min
        ),
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("priorNoticeDurationMax", max);
    notice.insert_context_field("priorNoticeDurationMin", min);
    notice.field_order = vec![
        "bookingRuleId".into(),
        "csvRowNumber".into(),
        "priorNoticeDurationMax".into(),
        "priorNoticeDurationMin".into(),
    ];
    notice
}

fn missing_prior_notice_duration_min_notice(
    booking_rule_id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_PRIOR_NOTICE_DURATION_MIN,
        NoticeSeverity::Error,
        "prior_notice_duration_min is required when booking_type=SAMEDAY",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["bookingRuleId".into(), "csvRowNumber".into()];
    notice
}

fn forbidden_prior_notice_start_day_notice(
    booking_rule_id: &str,
    row_number: u64,
    prior_notice_duration_max: Option<i32>,
    prior_notice_start_day: Option<i32>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_PRIOR_NOTICE_START_DAY,
        NoticeSeverity::Error,
        "prior_notice_start_day is forbidden when prior_notice_duration_max is set",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "priorNoticeDurationMax",
        prior_notice_duration_max.unwrap_or_default(),
    );
    notice.insert_context_field(
        "priorNoticeStartDay",
        prior_notice_start_day.unwrap_or_default(),
    );
    notice.field_order = vec![
        "bookingRuleId".into(),
        "csvRowNumber".into(),
        "priorNoticeDurationMax".into(),
        "priorNoticeStartDay".into(),
    ];
    notice
}

fn prior_notice_last_day_after_start_day_notice(
    row_number: u64,
    prior_notice_last_day: i32,
    prior_notice_start_day: i32,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_PRIOR_NOTICE_LAST_DAY_AFTER_START_DAY,
        NoticeSeverity::Error,
        "prior_notice_last_day is greater than prior_notice_start_day",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("priorNoticeLastDay", prior_notice_last_day);
    notice.insert_context_field("priorNoticeStartDay", prior_notice_start_day);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "priorNoticeLastDay".into(),
        "priorNoticeStartDay".into(),
    ];
    notice
}

fn forbidden_prior_notice_start_time_notice(
    booking_rule_id: &str,
    row_number: u64,
    prior_notice_start_time: Option<gtfs_guru_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_PRIOR_NOTICE_START_TIME,
        NoticeSeverity::Error,
        "prior_notice_start_time is forbidden when prior_notice_start_day is not set",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "priorNoticeStartTime",
        prior_notice_start_time
            .map(|time| time.to_string())
            .unwrap_or_default(),
    );
    notice.field_order = vec![
        "bookingRuleId".into(),
        "csvRowNumber".into(),
        "priorNoticeStartTime".into(),
    ];
    notice
}

fn missing_prior_notice_start_time_notice(
    booking_rule_id: &str,
    row_number: u64,
    prior_notice_start_day: Option<i32>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_PRIOR_NOTICE_START_TIME,
        NoticeSeverity::Error,
        "prior_notice_start_time is required when prior_notice_start_day is set",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "priorNoticeStartDay",
        prior_notice_start_day.unwrap_or_default(),
    );
    notice.field_order = vec![
        "bookingRuleId".into(),
        "csvRowNumber".into(),
        "priorNoticeStartDay".into(),
    ];
    notice
}

fn missing_prior_notice_last_day_notice(
    booking_rule_id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_PRIOR_NOTICE_LAST_DAY,
        NoticeSeverity::Error,
        "prior_notice_last_day is required when booking_type=PRIORDAY",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["bookingRuleId".into(), "csvRowNumber".into()];
    notice
}

fn missing_prior_notice_last_time_notice(
    booking_rule_id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_PRIOR_NOTICE_LAST_TIME,
        NoticeSeverity::Error,
        "prior_notice_last_time is required when booking_type=PRIORDAY",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["bookingRuleId".into(), "csvRowNumber".into()];
    notice
}

#[allow(dead_code)]
fn missing_prior_day_booking_field_value_notice(
    booking_rule_id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_prior_day_booking_field_value",
        NoticeSeverity::Error,
        "prior_notice_last_day and prior_notice_last_time are required for booking_type=PRIORDAY",
    );
    notice.insert_context_field("bookingRuleId", booking_rule_id.trim());
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["bookingRuleId".into(), "csvRowNumber".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn test_forbidden_realtime_fields() {
        let mut feed = GtfsFeed::default();
        feed.booking_rules = Some(CsvTable {
            rows: vec![BookingRules {
                booking_rule_id: feed.pool.intern("R1"),
                booking_type: BookingType::Realtime,
                prior_notice_duration_min: Some(10), // Forbidden
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        BookingRulesEntityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_REALTIME_FIELDS
        );
    }

    #[test]
    fn test_missing_prior_notice_duration_min() {
        let mut feed = GtfsFeed::default();
        feed.booking_rules = Some(CsvTable {
            rows: vec![BookingRules {
                booking_rule_id: feed.pool.intern("R1"),
                booking_type: BookingType::SameDay,
                prior_notice_duration_min: None, // Required
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        BookingRulesEntityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_PRIOR_NOTICE_DURATION_MIN
        );
    }
}
