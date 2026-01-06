use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FORBIDDEN_ARRIVAL_OR_DEPARTURE_TIME: &str = "forbidden_arrival_or_departure_time";
const CODE_MISSING_PICKUP_OR_DROP_OFF_WINDOW: &str = "missing_pickup_or_drop_off_window";
const CODE_INVALID_PICKUP_DROP_OFF_WINDOW: &str = "invalid_pickup_drop_off_window";

#[derive(Debug, Default)]
pub struct PickupDropOffWindowValidator;

impl Validator for PickupDropOffWindowValidator {
    fn name(&self) -> &'static str {
        "pickup_drop_off_window"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !has_pickup_drop_off_window_headers(&feed.stop_times.headers) {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let start = stop_time.start_pickup_drop_off_window;
            let end = stop_time.end_pickup_drop_off_window;
            if start.is_none() && end.is_none() {
                continue;
            }

            let has_arrival = stop_time.arrival_time.is_some();
            let has_departure = stop_time.departure_time.is_some();
            if has_arrival || has_departure {
                notices.push(forbidden_arrival_or_departure_notice(
                    row_number,
                    stop_time.arrival_time,
                    stop_time.departure_time,
                    start,
                    end,
                ));
            }

            if start.is_none() || end.is_none() {
                notices.push(missing_pickup_or_drop_off_window_notice(
                    row_number, start, end,
                ));
                continue;
            }

            let (start, end) = (start.expect("start"), end.expect("end"));
            if start.total_seconds() >= end.total_seconds() {
                notices.push(invalid_pickup_drop_off_window_notice(
                    row_number,
                    Some(start),
                    Some(end),
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

fn forbidden_arrival_or_departure_notice(
    row_number: u64,
    arrival: Option<gtfs_guru_model::GtfsTime>,
    departure: Option<gtfs_guru_model::GtfsTime>,
    start: Option<gtfs_guru_model::GtfsTime>,
    end: Option<gtfs_guru_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FORBIDDEN_ARRIVAL_OR_DEPARTURE_TIME,
        NoticeSeverity::Error,
        "arrival_time or departure_time must be empty when pickup/drop_off windows are set",
    );
    notice.insert_context_field("arrivalTime", time_value(arrival));
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("departureTime", time_value(departure));
    notice.insert_context_field("endPickupDropOffWindow", time_value(end));
    notice.insert_context_field("startPickupDropOffWindow", time_value(start));
    notice.field_order = vec![
        "arrivalTime".into(),
        "csvRowNumber".into(),
        "departureTime".into(),
        "endPickupDropOffWindow".into(),
        "startPickupDropOffWindow".into(),
    ];
    notice
}

fn missing_pickup_or_drop_off_window_notice(
    row_number: u64,
    start: Option<gtfs_guru_model::GtfsTime>,
    end: Option<gtfs_guru_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_PICKUP_OR_DROP_OFF_WINDOW,
        NoticeSeverity::Error,
        "start_pickup_drop_off_window and end_pickup_drop_off_window must both be set",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("endPickupDropOffWindow", time_value(end));
    notice.insert_context_field("startPickupDropOffWindow", time_value(start));
    notice.field_order = vec![
        "csvRowNumber".into(),
        "endPickupDropOffWindow".into(),
        "startPickupDropOffWindow".into(),
    ];
    notice
}

fn invalid_pickup_drop_off_window_notice(
    row_number: u64,
    start: Option<gtfs_guru_model::GtfsTime>,
    end: Option<gtfs_guru_model::GtfsTime>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_INVALID_PICKUP_DROP_OFF_WINDOW,
        NoticeSeverity::Error,
        "end_pickup_drop_off_window must be later than start_pickup_drop_off_window",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("endPickupDropOffWindow", time_value(end));
    notice.insert_context_field("startPickupDropOffWindow", time_value(start));
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
    use gtfs_guru_model::{GtfsTime, StopTime};

    #[test]
    fn detects_forbidden_arrival_departure() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "arrival_time".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                arrival_time: Some(GtfsTime::from_seconds(3700)), // Forbidden
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffWindowValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_FORBIDDEN_ARRIVAL_OR_DEPARTURE_TIME));
    }

    #[test]
    fn detects_missing_window_half() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                end_pickup_drop_off_window: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffWindowValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_PICKUP_OR_DROP_OFF_WINDOW));
    }

    #[test]
    fn detects_invalid_window_order() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                end_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)), // Invalid order
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffWindowValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_INVALID_PICKUP_DROP_OFF_WINDOW));
    }

    #[test]
    fn passes_valid_window() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_sequence: 1,
                start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        PickupDropOffWindowValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
