use compact_str::CompactString;
use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate, Weekday};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{Calendar, ExceptionType, GtfsDate, GtfsTime, ServiceAvailability};

const CODE_BLOCK_TRIPS_WITH_OVERLAPPING_STOP_TIMES: &str =
    "block_trips_with_overlapping_stop_times";

#[derive(Debug, Default)]
pub struct BlockTripsWithOverlappingStopTimesValidator;

impl Validator for BlockTripsWithOverlappingStopTimesValidator {
    fn name(&self) -> &'static str {
        "block_trips_with_overlapping_stop_times"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut stop_times_by_trip: HashMap<&str, Vec<&gtfs_guru_model::StopTime>> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .push(stop_time);
        }
        for stop_times in stop_times_by_trip.values_mut() {
            stop_times.sort_by_key(|stop_time| stop_time.stop_sequence);
        }

        let service_dates = build_service_dates(feed);
        let mut blocks: HashMap<&str, Vec<TripWindow<'_>>> = HashMap::new();

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let block_id = match trip
                .block_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(block_id) => block_id,
                None => continue,
            };

            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let stop_times = match stop_times_by_trip.get(trip_id) {
                Some(times) => times,
                None => continue,
            };
            let service_id = trip.service_id.trim();
            if service_id.is_empty() {
                continue;
            }

            let Some((start, end)) = trip_time_window(stop_times) else {
                continue;
            };

            blocks.entry(block_id).or_default().push(TripWindow {
                block_id,
                trip_id,
                service_id,
                start,
                end,
                row_number,
            });
        }

        for windows in blocks.values_mut() {
            windows.sort_by_key(|window| window.start.total_seconds());
        }

        for windows in blocks.values() {
            for i in 0..windows.len() {
                let current = &windows[i];
                for next in windows.iter().skip(i + 1) {
                    if next.start.total_seconds() >= current.end.total_seconds() {
                        break;
                    }
                    if !services_overlap(current.service_id, next.service_id, &service_dates) {
                        continue;
                    }
                    let mut notice = ValidationNotice::new(
                        CODE_BLOCK_TRIPS_WITH_OVERLAPPING_STOP_TIMES,
                        NoticeSeverity::Error,
                        "trips in the same block have overlapping stop times",
                    );
                    notice.insert_context_field("blockId", current.block_id);
                    notice.insert_context_field("csvRowNumberA", current.row_number);
                    notice.insert_context_field("csvRowNumberB", next.row_number);
                    notice.insert_context_field("intersection", overlap_label(current, next));
                    notice.insert_context_field("serviceIdA", current.service_id);
                    notice.insert_context_field("serviceIdB", next.service_id);
                    notice.insert_context_field("tripIdA", current.trip_id);
                    notice.insert_context_field("tripIdB", next.trip_id);
                    notice.field_order = vec![
                        "blockId".into(),
                        "csvRowNumberA".into(),
                        "csvRowNumberB".into(),
                        "intersection".into(),
                        "serviceIdA".into(),
                        "serviceIdB".into(),
                        "tripIdA".into(),
                        "tripIdB".into(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TripWindow<'a> {
    block_id: &'a str,
    trip_id: &'a str,
    service_id: &'a str,
    start: GtfsTime,
    end: GtfsTime,
    row_number: u64,
}

fn trip_time_window(stop_times: &[&gtfs_guru_model::StopTime]) -> Option<(GtfsTime, GtfsTime)> {
    let mut start = None;
    let mut end = None;

    for stop_time in stop_times {
        if start.is_none() {
            start = stop_time_start_time(stop_time);
        }
        if let Some(value) = stop_time_end_time(stop_time) {
            end = Some(value);
        }
    }

    match (start, end) {
        (Some(start), Some(end)) => Some((start, end)),
        _ => None,
    }
}

fn stop_time_start_time(stop_time: &gtfs_guru_model::StopTime) -> Option<GtfsTime> {
    match (stop_time.arrival_time, stop_time.departure_time) {
        (Some(arrival), Some(departure)) => {
            if arrival.total_seconds() <= departure.total_seconds() {
                Some(arrival)
            } else {
                Some(departure)
            }
        }
        (Some(arrival), None) => Some(arrival),
        (None, Some(departure)) => Some(departure),
        _ => None,
    }
}

fn stop_time_end_time(stop_time: &gtfs_guru_model::StopTime) -> Option<GtfsTime> {
    match (stop_time.arrival_time, stop_time.departure_time) {
        (Some(arrival), Some(departure)) => {
            if arrival.total_seconds() >= departure.total_seconds() {
                Some(arrival)
            } else {
                Some(departure)
            }
        }
        (Some(arrival), None) => Some(arrival),
        (None, Some(departure)) => Some(departure),
        _ => None,
    }
}

fn overlap_label(current: &TripWindow<'_>, next: &TripWindow<'_>) -> String {
    let start = if current.start.total_seconds() >= next.start.total_seconds() {
        current.start
    } else {
        next.start
    };
    let end = if current.end.total_seconds() <= next.end.total_seconds() {
        current.end
    } else {
        next.end
    };
    format!("{}-{}", start, end)
}

fn services_overlap(
    left_service_id: &str,
    right_service_id: &str,
    service_dates: &HashMap<CompactString, HashSet<NaiveDate>>,
) -> bool {
    if left_service_id == right_service_id {
        return true;
    }

    let left_dates = service_dates.get(left_service_id);
    let right_dates = service_dates.get(right_service_id);

    match (left_dates, right_dates) {
        (Some(left), Some(right)) => left.iter().any(|date| right.contains(date)),
        _ => false,
    }
}

fn build_service_dates(feed: &GtfsFeed) -> HashMap<CompactString, HashSet<NaiveDate>> {
    let mut dates_by_service: HashMap<CompactString, HashSet<NaiveDate>> = HashMap::new();

    if let Some(calendar) = &feed.calendar {
        for row in &calendar.rows {
            let Some(mut current) = gtfs_date_to_naive(row.start_date) else {
                continue;
            };
            let Some(end_date) = gtfs_date_to_naive(row.end_date) else {
                continue;
            };

            while current <= end_date {
                if service_available_on_date(row, current) {
                    dates_by_service
                        .entry(row.service_id.clone())
                        .or_default()
                        .insert(current);
                }
                match current.succ_opt() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }

    if let Some(calendar_dates) = &feed.calendar_dates {
        for row in &calendar_dates.rows {
            let Some(date) = gtfs_date_to_naive(row.date) else {
                continue;
            };
            let entry = dates_by_service.entry(row.service_id.clone()).or_default();
            match row.exception_type {
                ExceptionType::Added => {
                    entry.insert(date);
                }
                ExceptionType::Removed => {
                    entry.remove(&date);
                }
                _ => {}
            }
        }
    }

    dates_by_service
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

fn service_available_on_date(calendar: &Calendar, date: NaiveDate) -> bool {
    match date.weekday() {
        Weekday::Mon => is_available(calendar.monday),
        Weekday::Tue => is_available(calendar.tuesday),
        Weekday::Wed => is_available(calendar.wednesday),
        Weekday::Thu => is_available(calendar.thursday),
        Weekday::Fri => is_available(calendar.friday),
        Weekday::Sat => is_available(calendar.saturday),
        Weekday::Sun => is_available(calendar.sunday),
    }
}

fn is_available(availability: ServiceAvailability) -> bool {
    matches!(availability, ServiceAvailability::Available)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsDate, RouteType, StopTime};

    #[test]
    fn emits_notice_for_overlapping_trips_in_same_block() {
        let mut feed = base_feed();
        feed.trips.rows = vec![trip("T1", "SVC1", "BLOCK1"), trip("T2", "SVC1", "BLOCK1")];
        feed.stop_times.rows = stop_times_for_trip("T1", "08:00:00", "09:00:00");
        feed.stop_times
            .rows
            .extend(stop_times_for_trip("T2", "08:30:00", "09:30:00"));
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![calendar_row("SVC1", "20240101", Weekday::Mon)],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        BlockTripsWithOverlappingStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_BLOCK_TRIPS_WITH_OVERLAPPING_STOP_TIMES
        );
    }

    #[test]
    fn no_notice_for_non_overlapping_trips() {
        let mut feed = base_feed();
        feed.trips.rows = vec![trip("T1", "SVC1", "BLOCK1"), trip("T2", "SVC1", "BLOCK1")];
        feed.stop_times.rows = stop_times_for_trip("T1", "08:00:00", "09:00:00");
        feed.stop_times
            .rows
            .extend(stop_times_for_trip("T2", "09:00:00", "10:00:00"));
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![calendar_row("SVC1", "20240101", Weekday::Mon)],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        BlockTripsWithOverlappingStopTimesValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn no_notice_when_service_dates_do_not_overlap() {
        let mut feed = base_feed();
        feed.trips.rows = vec![trip("T1", "SVC1", "BLOCK1"), trip("T2", "SVC2", "BLOCK1")];
        feed.stop_times.rows = stop_times_for_trip("T1", "08:00:00", "09:00:00");
        feed.stop_times
            .rows
            .extend(stop_times_for_trip("T2", "08:30:00", "09:30:00"));
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![
                calendar_row("SVC1", "20240101", Weekday::Mon),
                calendar_row("SVC2", "20240102", Weekday::Tue),
            ],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        BlockTripsWithOverlappingStopTimesValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn emits_notice_when_service_dates_overlap() {
        let mut feed = base_feed();
        feed.trips.rows = vec![trip("T1", "SVC1", "BLOCK1"), trip("T2", "SVC2", "BLOCK1")];
        feed.stop_times.rows = stop_times_for_trip("T1", "08:00:00", "09:00:00");
        feed.stop_times
            .rows
            .extend(stop_times_for_trip("T2", "08:30:00", "09:30:00"));
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![
                calendar_row("SVC1", "20240101", Weekday::Mon),
                calendar_row("SVC2", "20240101", Weekday::Mon),
            ],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        BlockTripsWithOverlappingStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_BLOCK_TRIPS_WITH_OVERLAPPING_STOP_TIMES
        );
    }

    fn base_feed() -> GtfsFeed {
        GtfsFeed {
            agency: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_guru_model::Agency {
                    agency_id: None,
                    agency_name: "Agency".into(),
                    agency_url: "https://example.com".into(),
                    agency_timezone: "UTC".into(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: Vec::new(),
            },
            stops: CsvTable {
                headers: Vec::new(),
                rows: vec![
                    gtfs_guru_model::Stop {
                        stop_id: "STOP1".into(),
                        stop_name: Some("Stop 1".into()),
                        stop_lat: Some(10.0),
                        stop_lon: Some(20.0),
                        ..Default::default()
                    },
                    gtfs_guru_model::Stop {
                        stop_id: "STOP2".into(),
                        stop_name: Some("Stop 2".into()),
                        stop_lat: Some(10.1),
                        stop_lon: Some(20.1),
                        ..Default::default()
                    },
                ],
                row_numbers: Vec::new(),
            },
            routes: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_guru_model::Route {
                    route_id: "R1".into(),
                    route_short_name: Some("R1".into()),
                    route_type: RouteType::Bus,
                    ..Default::default()
                }],
                row_numbers: Vec::new(),
            },
            trips: CsvTable::default(),
            stop_times: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            calendar: None,
            calendar_dates: None,
            fare_attributes: None,
            fare_rules: None,
            fare_media: None,
            fare_products: None,
            fare_leg_rules: None,
            fare_transfer_rules: None,
            fare_leg_join_rules: None,
            areas: None,
            stop_areas: None,
            timeframes: None,
            rider_categories: None,
            shapes: None,
            frequencies: None,
            transfers: None,
            location_groups: None,
            location_group_stops: None,
            locations: None,
            booking_rules: None,
            feed_info: None,
            attributions: None,
            levels: None,
            pathways: None,
            translations: None,
            networks: None,
            stop_times_by_trip: std::collections::HashMap::new(),
            route_networks: None,
        }
    }

    fn trip(trip_id: &str, service_id: &str, block_id: &str) -> gtfs_guru_model::Trip {
        gtfs_guru_model::Trip {
            route_id: "R1".into(),
            service_id: service_id.into(),
            trip_id: trip_id.into(),
            block_id: Some(block_id.into()),
            ..Default::default()
        }
    }

    fn stop_times_for_trip(trip_id: &str, start: &str, end: &str) -> Vec<StopTime> {
        vec![
            StopTime {
                trip_id: trip_id.into(),
                stop_id: "STOP1".into(),
                stop_sequence: 1,
                arrival_time: Some(GtfsTime::parse(start).unwrap()),
                departure_time: Some(GtfsTime::parse(start).unwrap()),
                ..Default::default()
            },
            StopTime {
                trip_id: trip_id.into(),
                stop_id: "STOP2".into(),
                stop_sequence: 2,
                arrival_time: Some(GtfsTime::parse(end).unwrap()),
                departure_time: Some(GtfsTime::parse(end).unwrap()),
                ..Default::default()
            },
        ]
    }

    fn calendar_row(service_id: &str, date_str: &str, weekday: Weekday) -> Calendar {
        let date = GtfsDate::parse(date_str).unwrap();
        Calendar {
            service_id: service_id.into(),
            monday: if weekday == Weekday::Mon {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            tuesday: if weekday == Weekday::Tue {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            wednesday: if weekday == Weekday::Wed {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            thursday: if weekday == Weekday::Thu {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            friday: if weekday == Weekday::Fri {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            saturday: if weekday == Weekday::Sat {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            sunday: if weekday == Weekday::Sun {
                ServiceAvailability::Available
            } else {
                ServiceAvailability::Unavailable
            },
            start_date: date,
            end_date: date,
        }
    }
}
