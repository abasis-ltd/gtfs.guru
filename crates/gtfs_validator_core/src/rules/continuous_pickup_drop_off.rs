use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FORBIDDEN_CONTINUOUS_PICKUP_DROP_OFF: &str = "forbidden_continuous_pickup_drop_off";

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

        let mut stop_times_by_trip: HashMap<&str, Vec<(u64, &gtfs_guru_model::StopTime)>> =
            HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .push((row_number, stop_time));
        }

        for (route_index, route) in feed.routes.rows.iter().enumerate() {
            let route_row_number = feed.routes.row_number(route_index);
            let route_id = route.route_id.trim();
            if route_id.is_empty() {
                continue;
            }
            if route.continuous_pickup.is_none() && route.continuous_drop_off.is_none() {
                continue;
            }
            for trip in feed
                .trips
                .rows
                .iter()
                .filter(|trip| trip.route_id.trim() == route_id)
            {
                let trip_id = trip.trip_id.trim();
                if trip_id.is_empty() {
                    continue;
                }
                let Some(stop_times) = stop_times_by_trip.get(trip_id) else {
                    continue;
                };
                for (row_number, stop_time) in stop_times {
                    if stop_time.start_pickup_drop_off_window.is_some()
                        || stop_time.end_pickup_drop_off_window.is_some()
                    {
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
                        notice.insert_context_field("tripId", trip_id);
                        notice.field_order = vec![
                            "endPickupDropOffWindow".to_string(),
                            "routeCsvRowNumber".to_string(),
                            "startPickupDropOffWindow".to_string(),
                            "stopTimeCsvRowNumber".to_string(),
                            "tripId".to_string(),
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
    use gtfs_guru_model::{ContinuousPickupDropOff, GtfsTime, Route, StopTime, Trip};

    #[test]
    fn detects_forbidden_windows() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string(), "continuous_pickup".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                continuous_pickup: Some(ContinuousPickupDropOff::Continuous),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["route_id".to_string(), "trip_id".to_string()],
            rows: vec![Trip {
                route_id: "R1".to_string(),
                trip_id: "T1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "start_pickup_drop_off_window".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
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
            headers: vec!["route_id".to_string(), "continuous_pickup".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                continuous_pickup: Some(ContinuousPickupDropOff::Continuous),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["route_id".to_string(), "trip_id".to_string()],
            rows: vec![Trip {
                route_id: "R1".to_string(),
                trip_id: "T1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
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
            headers: vec!["route_id".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // stop_times missing window headers, validator should skip
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                start_pickup_drop_off_window: Some(GtfsTime::parse("08:00:00").unwrap()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ContinuousPickupDropOffValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
