use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::PickupDropOffType;

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
    start_window: Option<gtfs_guru_model::GtfsTime>,
    end_window: Option<gtfs_guru_model::GtfsTime>,
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
        "csvRowNumber".into(),
        "endPickupDropOffWindow".into(),
        "startPickupDropOffWindow".into(),
    ];
    notice
}

fn forbidden_drop_off_type_notice(
    row_number: u64,
    start_window: Option<gtfs_guru_model::GtfsTime>,
    end_window: Option<gtfs_guru_model::GtfsTime>,
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
        "csvRowNumber".into(),
        "endPickupDropOffWindow".into(),
        "startPickupDropOffWindow".into(),
    ];
    notice
}

fn time_value(value: Option<gtfs_guru_model::GtfsTime>) -> String {
    value.map(|time| time.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, PickupDropOffType, StopTime};

    #[test]
    fn detects_forbidden_pickup_type() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "pickup_type".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                pickup_type: Some(PickupDropOffType::Regular), // Forbidden for windows
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffTypeValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_FORBIDDEN_PICKUP_TYPE));
    }

    #[test]
    fn detects_forbidden_drop_off_type() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "drop_off_type".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                drop_off_type: Some(PickupDropOffType::Regular), // Forbidden for windows
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_FORBIDDEN_DROP_OFF_TYPE));
    }

    #[test]
    fn passes_valid_types_for_windows() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "pickup_type".into(),
                "drop_off_type".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                pickup_type: Some(PickupDropOffType::MustPhone),
                drop_off_type: Some(PickupDropOffType::NoPickup),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
