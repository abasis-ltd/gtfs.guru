use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::PickupDropOffType;

const CODE_FORBIDDEN_PICKUP_TYPE: &str = "forbidden_pickup_type";
const CODE_FORBIDDEN_DROP_OFF_TYPE: &str = "forbidden_drop_off_type";

#[derive(Debug, Default)]
pub struct PickupDropOffTypeValidator;

impl Validator for PickupDropOffTypeValidator {
    fn name(&self) -> &'static str {
        "pickup_drop_off_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !has_pickup_drop_off_window_headers(&feed.stop_times.headers) {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let has_window = stop_time.start_pickup_drop_off_window.is_some()
                || stop_time.end_pickup_drop_off_window.is_some();
            if !has_window {
                continue;
            }

            let pickup_type = normalized_pickup_drop_off_type(stop_time.pickup_type);
            if matches!(
                pickup_type,
                PickupDropOffType::Regular | PickupDropOffType::MustCoordinateWithDriver
            ) {
                notices.push(forbidden_pickup_type_notice(
                    row_number,
                    stop_time.start_pickup_drop_off_window,
                    stop_time.end_pickup_drop_off_window,
                ));
            }

            let drop_off_type = normalized_pickup_drop_off_type(stop_time.drop_off_type);
            if matches!(drop_off_type, PickupDropOffType::Regular) {
                notices.push(forbidden_drop_off_type_notice(
                    row_number,
                    stop_time.start_pickup_drop_off_window,
                    stop_time.end_pickup_drop_off_window,
                ));
            }
        }
    }
}

fn has_pickup_drop_off_window_headers(headers: &[String]) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case("start_pickup_drop_off_window"))
        || headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("end_pickup_drop_off_window"))
}

fn normalized_pickup_drop_off_type(value: Option<PickupDropOffType>) -> PickupDropOffType {
    value.unwrap_or(PickupDropOffType::Regular)
}

fn forbidden_pickup_type_notice(
    row_number: u64,
    start_window: Option<gtfs_model::GtfsTime>,
    end_window: Option<gtfs_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_PICKUP_TYPE,
        NoticeSeverity::Error,
        "pickup_type forbids pickup/drop_off windows",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("startPickupDropOffWindow", time_value(start_window));
    notice.insert_context_field("endPickupDropOffWindow", time_value(end_window));
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "endPickupDropOffWindow".to_string(),
        "startPickupDropOffWindow".to_string(),
    ];
    notice
}

fn forbidden_drop_off_type_notice(
    row_number: u64,
    start_window: Option<gtfs_model::GtfsTime>,
    end_window: Option<gtfs_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_DROP_OFF_TYPE,
        NoticeSeverity::Error,
        "drop_off_type forbids pickup/drop_off windows",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("startPickupDropOffWindow", time_value(start_window));
    notice.insert_context_field("endPickupDropOffWindow", time_value(end_window));
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "endPickupDropOffWindow".to_string(),
        "startPickupDropOffWindow".to_string(),
    ];
    notice
}

fn time_value(value: Option<gtfs_model::GtfsTime>) -> String {
    value.map(|time| time.to_string()).unwrap_or_default()
}

