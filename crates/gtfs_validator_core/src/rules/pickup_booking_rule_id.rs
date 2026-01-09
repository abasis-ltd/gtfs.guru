use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::PickupDropOffType;
use gtfs_guru_model::StringId;

const CODE_MISSING_BOOKING_RULE_ID: &str = "missing_pickup_drop_off_booking_rule_id";

#[derive(Debug, Default)]
pub struct PickupBookingRuleIdValidator;

impl Validator for PickupBookingRuleIdValidator {
    fn name(&self) -> &'static str {
        "pickup_booking_rule_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.booking_rules.is_none() {
            return;
        }

        let has_pickup_type = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("pickup_type"));
        let has_drop_off_type = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("drop_off_type"));
        if !has_pickup_type && !has_drop_off_type {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            if stop_time.start_pickup_drop_off_window.is_some()
                && is_must_phone(stop_time.pickup_type)
                && !has_value(stop_time.pickup_booking_rule_id)
            {
                notices.push(missing_booking_rule_notice(
                    stop_time,
                    row_number,
                    "trip uses start_pickup_drop_off_window but pickup_booking_rule_id is empty",
                ));
            }

            if stop_time.end_pickup_drop_off_window.is_some()
                && is_must_phone(stop_time.drop_off_type)
                && !has_value(stop_time.drop_off_booking_rule_id)
            {
                notices.push(missing_booking_rule_notice(
                    stop_time,
                    row_number,
                    "trip uses end_pickup_drop_off_window but drop_off_booking_rule_id is empty",
                ));
            }
        }
    }
}

fn has_value(value: Option<StringId>) -> bool {
    matches!(value, Some(id) if id.0 != 0)
}

fn is_must_phone(value: Option<PickupDropOffType>) -> bool {
    matches!(value, Some(PickupDropOffType::MustPhone))
}

fn missing_booking_rule_notice(
    stop_time: &gtfs_guru_model::StopTime,
    row_number: u64,
    message: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_BOOKING_RULE_ID,
        NoticeSeverity::Warning,
        message,
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "dropOffType",
        pickup_drop_off_value(stop_time.drop_off_type),
    );
    notice.insert_context_field("pickupType", pickup_drop_off_value(stop_time.pickup_type));
    notice.field_order = vec![
        "csvRowNumber".into(),
        "dropOffType".into(),
        "pickupType".into(),
    ];
    notice
}

fn pickup_drop_off_value(value: Option<PickupDropOffType>) -> Option<i32> {
    match value {
        Some(PickupDropOffType::Regular) => Some(0),
        Some(PickupDropOffType::NoPickup) => Some(1),
        Some(PickupDropOffType::MustPhone) => Some(2),
        Some(PickupDropOffType::MustCoordinateWithDriver) => Some(3),
        Some(PickupDropOffType::Other) => None,
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, StopTime};

    #[test]
    fn detects_missing_pickup_booking_rule_id() {
        let mut feed = GtfsFeed::default();
        feed.booking_rules = Some(CsvTable::default());
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "pickup_type".into(),
                "start_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                pickup_type: Some(PickupDropOffType::MustPhone),
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                pickup_booking_rule_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupBookingRuleIdValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_BOOKING_RULE_ID));
    }

    #[test]
    fn detects_missing_drop_off_booking_rule_id() {
        let mut feed = GtfsFeed::default();
        feed.booking_rules = Some(CsvTable::default());
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "drop_off_type".into(),
                "end_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                drop_off_type: Some(PickupDropOffType::MustPhone),
                end_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                drop_off_booking_rule_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupBookingRuleIdValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_BOOKING_RULE_ID));
    }

    #[test]
    fn passes_when_booking_rule_ids_present() {
        let mut feed = GtfsFeed::default();
        feed.booking_rules = Some(CsvTable::default());
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "pickup_booking_rule_id".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                pickup_booking_rule_id: Some(feed.pool.intern("B1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupBookingRuleIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
