use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{ExceptionType, GtfsDate, ServiceAvailability};

const CODE_TRIP_COVERAGE_NOT_ACTIVE_FOR_NEXT_7_DAYS: &str =
    "trip_coverage_not_active_for_next7_days";
const MAX_SERVICE_DATE_TRIP_COUNT_RATIO: f64 = 0.90;
const MAX_SERVICE_DATE_TRIP_COUNT_LIMIT: usize = 30;
const MAJORITY_TRIP_COUNT_RATIO: f64 = 0.75;

#[derive(Debug, Default)]
pub struct DateTripsValidator;

impl Validator for DateTripsValidator {
    fn name(&self) -> &'static str {
        "date_trips"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let service_dates = build_service_dates(feed);
        let trip_counts = count_trips_for_each_service_date(feed, &service_dates);
        let Some((majority_start, majority_end)) = compute_majority_service_coverage(&trip_counts)
        else {
            return;
        };

        let validation_date = crate::validation_date();
        let validation_date_plus_seven_days = validation_date + Duration::days(7);

        if majority_start > validation_date || majority_end < validation_date_plus_seven_days {
            notices.push(trip_coverage_notice());
        }
    }
}

fn trip_coverage_notice() -> ValidationNotice {
    ValidationNotice::new(
        CODE_TRIP_COVERAGE_NOT_ACTIVE_FOR_NEXT_7_DAYS,
        NoticeSeverity::Warning,
        "trip coverage is not active for the next 7 days",
    )
}

fn compute_majority_service_coverage(
    trip_count_by_date: &BTreeMap<NaiveDate, i32>,
) -> Option<(NaiveDate, NaiveDate)> {
    if trip_count_by_date.is_empty() {
        return None;
    }

    let mut sorted_counts: Vec<i32> = trip_count_by_date.values().copied().collect();
    sorted_counts.sort_unstable();
    let max_service_date_index = usize::max(
        (sorted_counts.len() as f64 * MAX_SERVICE_DATE_TRIP_COUNT_RATIO) as usize,
        sorted_counts
            .len()
            .saturating_sub(MAX_SERVICE_DATE_TRIP_COUNT_LIMIT),
    );
    let majority_trip_count_threshold =
        (MAJORITY_TRIP_COUNT_RATIO * sorted_counts[max_service_date_index] as f64) as i32;

    let mut majority_start = *trip_count_by_date.keys().next().expect("non-empty map");
    let mut majority_end = *trip_count_by_date
        .keys()
        .next_back()
        .expect("non-empty map");

    for (date, count) in trip_count_by_date.iter() {
        if *count >= majority_trip_count_threshold {
            majority_start = *date;
            break;
        }
    }

    for (date, count) in trip_count_by_date.iter().rev() {
        if *count >= majority_trip_count_threshold {
            majority_end = *date;
            break;
        }
    }

    Some((majority_start, majority_end))
}

fn count_trips_for_each_service_date(
    feed: &GtfsFeed,
    service_dates: &HashMap<gtfs_guru_model::StringId, BTreeSet<NaiveDate>>,
) -> BTreeMap<NaiveDate, i32> {
    let mut trip_count_by_date: BTreeMap<NaiveDate, i32> = BTreeMap::new();
    if feed.trips.rows.is_empty() || service_dates.is_empty() {
        return trip_count_by_date;
    }

    let mut frequencies_by_trip: HashMap<
        gtfs_guru_model::StringId,
        Vec<&gtfs_guru_model::Frequency>,
    > = HashMap::new();
    if let Some(frequencies) = &feed.frequencies {
        for frequency in &frequencies.rows {
            let trip_id = frequency.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            frequencies_by_trip
                .entry(trip_id)
                .or_default()
                .push(frequency);
        }
    }

    let mut trip_count_by_service_id: HashMap<gtfs_guru_model::StringId, i32> = HashMap::new();
    for trip in &feed.trips.rows {
        let trip_id = trip.trip_id;
        if trip_id.0 == 0 {
            continue;
        }
        let service_id = trip.service_id;
        if service_id.0 == 0 {
            continue;
        }
        let trip_count = compute_trip_count(trip_id, &frequencies_by_trip);
        *trip_count_by_service_id.entry(service_id).or_insert(0) += trip_count;
    }

    for (service_id, trip_count) in trip_count_by_service_id {
        let Some(dates) = service_dates.get(&service_id) else {
            continue;
        };
        for date in dates {
            *trip_count_by_date.entry(*date).or_insert(0) += trip_count;
        }
    }

    trip_count_by_date
}

fn compute_trip_count(
    trip_id: gtfs_guru_model::StringId,
    frequencies_by_trip: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Frequency>>,
) -> i32 {
    let Some(frequencies) = frequencies_by_trip.get(&trip_id) else {
        return 1;
    };

    if frequencies.is_empty() {
        return 1;
    }

    let mut trip_count = 0;
    for frequency in frequencies {
        trip_count += 1;
        if frequency.headway_secs > 0 {
            let headway_secs = frequency.headway_secs as i32;
            let span_secs =
                frequency.end_time.total_seconds() - frequency.start_time.total_seconds();
            trip_count += (span_secs - 1) / headway_secs;
        }
    }
    trip_count
}

fn build_service_dates(feed: &GtfsFeed) -> HashMap<gtfs_guru_model::StringId, BTreeSet<NaiveDate>> {
    let mut dates_by_service: HashMap<gtfs_guru_model::StringId, BTreeSet<NaiveDate>> =
        HashMap::new();

    if let Some(calendar) = &feed.calendar {
        for row in &calendar.rows {
            let Some(mut current) = gtfs_date_to_naive(row.start_date) else {
                continue;
            };
            let Some(mut end_date) = gtfs_date_to_naive(row.end_date) else {
                continue;
            };
            if current > end_date {
                end_date = current;
            }

            let service_id = row.service_id;
            if service_id.0 == 0 {
                continue;
            }
            let entry = dates_by_service.entry(service_id).or_default();
            while current <= end_date {
                if service_available_on_date(row, current) {
                    entry.insert(current);
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
            let service_id = row.service_id;
            if service_id.0 == 0 {
                continue;
            }
            let entry = dates_by_service.entry(service_id).or_default();
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

fn service_available_on_date(calendar: &gtfs_guru_model::Calendar, date: NaiveDate) -> bool {
    let availability = match date.weekday() {
        chrono::Weekday::Mon => calendar.monday,
        chrono::Weekday::Tue => calendar.tuesday,
        chrono::Weekday::Wed => calendar.wednesday,
        chrono::Weekday::Thu => calendar.thursday,
        chrono::Weekday::Fri => calendar.friday,
        chrono::Weekday::Sat => calendar.saturday,
        chrono::Weekday::Sun => calendar.sunday,
    };
    is_available(availability)
}

fn is_available(availability: ServiceAvailability) -> bool {
    matches!(availability, ServiceAvailability::Available)
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn test_date_trips_no_coverage() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            rows: vec![gtfs_guru_model::Trip {
                trip_id: feed.pool.intern("T1"),
                service_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            ..Default::default()
        };
        let _guard = crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()));

        let mut notices = NoticeContainer::new();
        DateTripsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
