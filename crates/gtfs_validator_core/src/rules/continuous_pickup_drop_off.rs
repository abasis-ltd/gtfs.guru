use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::ContinuousPickupDropOff;

const CODE_FORBIDDEN_CONTINUOUS_PICKUP_DROP_OFF: &str = "forbidden_continuous_pickup_drop_off";

/// Returns true if continuous pickup/drop-off is enabled (values 0, 2, 3).
/// Value 1 (NoContinuous) means disabled and should not trigger validation.
fn is_continuous_enabled(value: Option<ContinuousPickupDropOff>) -> bool {
    matches!(
        value,
        Some(ContinuousPickupDropOff::Continuous)
            | Some(ContinuousPickupDropOff::MustPhone)
            | Some(ContinuousPickupDropOff::MustCoordinateWithDriver)
    )
}

#[derive(Debug, Default)]
pub struct ContinuousPickupDropOffValidator;

impl Validator for ContinuousPickupDropOffValidator {
    fn name(&self) -> &'static str {
        "continuous_pickup_drop_off"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !has_route_headers(&feed.routes.headers)
            || !has_stop_time_headers(&feed.stop_times.headers)
        {
            return;
        }

        let mut stop_times_by_trip: HashMap<
            gtfs_guru_model::StringId,
            Vec<(u64, &gtfs_guru_model::StopTime)>,
        > = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .push((row_number, stop_time));
        }

        for (route_index, route) in feed.routes.rows.iter().enumerate() {
            let route_row_number = feed.routes.row_number(route_index);
            let route_id = route.route_id;
            if route_id.0 == 0 {
                continue;
            }
            if !is_continuous_enabled(route.continuous_pickup)
                && !is_continuous_enabled(route.continuous_drop_off)
            {
                continue;
            }
            for trip in feed
                .trips
                .rows
                .iter()
                .filter(|trip| trip.route_id == route_id)
            {
                let trip_id = trip.trip_id;
                if trip_id.0 == 0 {
                    continue;
                }
                let Some(stop_times) = stop_times_by_trip.get(&trip_id) else {
                    continue;
                };
                for (row_number, stop_time) in stop_times {
                    if stop_time.start_pickup_drop_off_window.is_some()
                        || stop_time.end_pickup_drop_off_window.is_some()
                    {
                        let trip_id_value = feed.pool.resolve(trip_id);
                        let mut notice = ValidationNotice::new(
                            CODE_FORBIDDEN_CONTINUOUS_PICKUP_DROP_OFF,
                            NoticeSeverity::Error,
                            "continuous pickup/drop-off forbids pickup/drop-off windows",
                        );
                        notice.insert_context_field(
                            "endPickupDropOffWindow",
                            time_value(stop_time.end_pickup_drop_off_window),
                        );
                        notice.insert_context_field("routeCsvRowNumber", route_row_number);
                        notice.insert_context_field(
                            "startPickupDropOffWindow",
                            time_value(stop_time.start_pickup_drop_off_window),
                        );
                        notice.insert_context_field("stopTimeCsvRowNumber", *row_number);
                        notice.insert_context_field("tripId", trip_id_value.as_str());
                        notice.field_order = vec![
                            "endPickupDropOffWindow".into(),
                            "routeCsvRowNumber".into(),
                            "startPickupDropOffWindow".into(),
                            "stopTimeCsvRowNumber".into(),
                            "tripId".into(),
                        ];
                        notices.push(notice);
                    }
                }
            }
        }
    }
}

fn has_route_headers(headers: &[String]) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case("continuous_pickup"))
        || headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("continuous_drop_off"))
}

fn has_stop_time_headers(headers: &[String]) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case("start_pickup_drop_off_window"))
        || headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("end_pickup_drop_off_window"))
}

fn time_value(value: Option<gtfs_guru_model::GtfsTime>) -> String {
    value.map(|time| time.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, Route, StopTime, Trip};

    #[test]
    fn detects_forbidden_windows() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "continuous_pickup".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                continuous_pickup: Some(ContinuousPickupDropOff::Continuous),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "start_pickup_drop_off_window".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                start_pickup_drop_off_window: Some(GtfsTime::parse("08:00:00").unwrap()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ContinuousPickupDropOffValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_CONTINUOUS_PICKUP_DROP_OFF
        );
    }

    #[test]
    fn passes_without_windows() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "continuous_pickup".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                continuous_pickup: Some(ContinuousPickupDropOff::Continuous),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ContinuousPickupDropOffValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_without_headers() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // stop_times missing window headers, validator should skip
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                start_pickup_drop_off_window: Some(GtfsTime::parse("08:00:00").unwrap()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ContinuousPickupDropOffValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_when_continuous_disabled() {
        // NoContinuous (value=1) means continuous is disabled, should not trigger error
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "continuous_pickup".into(),
                "continuous_drop_off".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                continuous_pickup: Some(ContinuousPickupDropOff::NoContinuous),
                continuous_drop_off: Some(ContinuousPickupDropOff::NoContinuous),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["route_id".into(), "trip_id".into()],
            rows: vec![Trip {
                route_id: feed.pool.intern("R1"),
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
            ],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                start_pickup_drop_off_window: Some(GtfsTime::parse("08:00:00").unwrap()),
                end_pickup_drop_off_window: Some(GtfsTime::parse("18:00:00").unwrap()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ContinuousPickupDropOffValidator.validate(&feed, &mut notices);

        // Should NOT generate error because continuous is disabled (value=1)
        assert_eq!(notices.len(), 0);
    }
}
