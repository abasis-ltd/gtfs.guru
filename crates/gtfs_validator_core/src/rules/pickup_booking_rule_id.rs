use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::PickupDropOffType;

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
        let has_start_window = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("start_pickup_drop_off_window"));
        let has_end_window = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("end_pickup_drop_off_window"));

        if !has_pickup_type && !has_drop_off_type && !has_start_window && !has_end_window {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            if stop_time.start_pickup_drop_off_window.is_some()
                && !has_value(stop_time.pickup_booking_rule_id.as_deref())
            {
                notices.push(missing_booking_rule_notice(
                    stop_time,
                    row_number,
                    "trip uses start_pickup_drop_off_window but pickup_booking_rule_id is empty",
                ));
            }

            if stop_time.end_pickup_drop_off_window.is_some()
                && !has_value(stop_time.drop_off_booking_rule_id.as_deref())
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

fn has_value(value: Option<&str>) -> bool {
    value.map(|val| !val.trim().is_empty()).unwrap_or(false)
}

fn missing_booking_rule_notice(
    stop_time: &gtfs_model::StopTime,
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
        "csvRowNumber".to_string(),
        "dropOffType".to_string(),
        "pickupType".to_string(),
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

