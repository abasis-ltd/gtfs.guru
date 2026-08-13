use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{Datelike, NaiveDate};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{ExceptionType, GtfsDate, ServiceAvailability, StringId};

const MAX_GAP_DAYS: i64 = 13;
const MAX_FUTURE_EXTENT_DAYS: i64 = 2 * 365;
const FEED_WINDOW_THRESHOLD_DAYS: i64 = 14;

#[derive(Debug, Default)]
pub struct ServiceWindowsV8Validator;

impl Validator for ServiceWindowsV8Validator {
    fn name(&self) -> &'static str {
        "service_windows_v8"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let active_dates = build_active_dates(feed);
        validate_service_spread(feed, &active_dates, notices);
        validate_feed_window(feed, &active_dates, notices);
        validate_future_feed(feed, notices);
    }
}

fn validate_service_spread(
    feed: &GtfsFeed,
    active_dates: &BTreeMap<StringId, BTreeSet<NaiveDate>>,
    notices: &mut NoticeContainer,
) {
    let Some(calendar) = &feed.calendar else {
        return;
    };
    let mut visited = HashSet::new();
    let today = crate::validation_date();
    for service in &calendar.rows {
        if service.service_id.0 == 0 || !visited.insert(service.service_id) {
            continue;
        }
        let Some(dates) = active_dates
            .get(&service.service_id)
            .filter(|dates| !dates.is_empty())
        else {
            continue;
        };
        let mut previous: Option<NaiveDate> = None;
        for &date in dates {
            if let Some(previous_date) = previous {
                let gap = (date - previous_date).num_days() - 1;
                if gap > MAX_GAP_DAYS {
                    let mut notice = ValidationNotice::new(
                        "big_gap_in_service",
                        NoticeSeverity::Info,
                        "service has a gap of more than 13 days between active dates",
                    );
                    notice.insert_context_field(
                        "serviceId",
                        feed.pool.resolve(service.service_id).as_str(),
                    );
                    notice.insert_context_field("gapStartDate", previous_date.to_string());
                    notice.insert_context_field("gapEndDate", date.to_string());
                    notice.insert_context_field("gapDurationDays", gap);
                    notice.field_order = vec![
                        "serviceId".into(),
                        "gapStartDate".into(),
                        "gapEndDate".into(),
                        "gapDurationDays".into(),
                    ];
                    notices.push(notice);
                }
            }
            previous = Some(date);
        }
        let last_active = *dates.last().unwrap();
        if (last_active - today).num_days() > MAX_FUTURE_EXTENT_DAYS {
            let mut notice = ValidationNotice::new(
                "service_extends_far_in_the_future",
                NoticeSeverity::Info,
                "service end date is more than two years in the future",
            );
            notice
                .insert_context_field("serviceId", feed.pool.resolve(service.service_id).as_str());
            notice.insert_context_field("serviceWindowEndDate", last_active.to_string());
            notice.field_order = vec!["serviceId".into(), "serviceWindowEndDate".into()];
            notices.push(notice);
        }
    }
}

fn validate_feed_window(
    feed: &GtfsFeed,
    active_dates: &BTreeMap<StringId, BTreeSet<NaiveDate>>,
    notices: &mut NoticeContainer,
) {
    // First-appearance order over trips.txt: `StringId` values are assigned by
    // racing parallel CSV workers, so ordering by them (e.g. via a BTreeSet)
    // varies between runs and the capped notice sample keeps a different
    // subset each run.
    let mut seen_service_ids = HashSet::new();
    let service_ids: Vec<_> = feed
        .trips
        .rows
        .iter()
        .filter_map(|trip| {
            (trip.service_id.0 != 0 && seen_service_ids.insert(trip.service_id))
                .then_some(trip.service_id)
        })
        .collect();
    let service_windows: Vec<_> = service_ids
        .iter()
        .filter_map(|service_id| {
            let dates = active_dates.get(service_id)?;
            Some((*service_id, *dates.first()?, *dates.last()?))
        })
        .collect();
    let Some(total_start) = service_windows.iter().map(|(_, start, _)| *start).min() else {
        return;
    };
    let total_end = service_windows
        .iter()
        .map(|(_, _, end)| *end)
        .max()
        .unwrap();

    let today = crate::validation_date();
    if total_start > today {
        let mut notice = ValidationNotice::new(
            "future_calendar",
            NoticeSeverity::Info,
            "all services in the feed start in the future",
        );
        notice.insert_context_field("minServiceStartDate", total_start.to_string());
        notice.insert_context_field("currentDate", today.to_string());
        notice.field_order = vec!["minServiceStartDate".into(), "currentDate".into()];
        notices.push(notice);
    }

    let Some(feed_info) = feed.feed_info.as_ref().and_then(|table| table.rows.first()) else {
        return;
    };
    let (Some(feed_start), Some(feed_end)) = (
        feed_info.feed_start_date.and_then(gtfs_date_to_naive),
        feed_info.feed_end_date.and_then(gtfs_date_to_naive),
    ) else {
        return;
    };

    for (service_id, service_start, service_end) in service_windows {
        let days_before = if service_start < feed_start {
            (feed_start - service_start).num_days()
        } else {
            0
        };
        let days_after = if service_end > feed_end {
            (service_end - feed_end).num_days()
        } else {
            0
        };
        if days_before == 0 && days_after == 0 {
            continue;
        }
        let mut notice = ValidationNotice::new(
            "service_window_outside_feed_period",
            NoticeSeverity::Info,
            "service window is not covered by the feed validity period",
        );
        notice.insert_context_field("serviceId", feed.pool.resolve(service_id).as_str());
        notice.insert_context_field("serviceWindowStartDate", service_start.to_string());
        notice.insert_context_field("serviceWindowEndDate", service_end.to_string());
        notice.insert_context_field("daysBeforeFeedStart", days_before);
        notice.insert_context_field("daysAfterFeedEnd", days_after);
        notice.field_order = vec![
            "serviceId".into(),
            "serviceWindowStartDate".into(),
            "serviceWindowEndDate".into(),
            "daysBeforeFeedStart".into(),
            "daysAfterFeedEnd".into(),
        ];
        notices.push(notice);
    }

    if feed_start < total_start - chrono::Duration::days(FEED_WINDOW_THRESHOLD_DAYS)
        || feed_end > total_end + chrono::Duration::days(FEED_WINDOW_THRESHOLD_DAYS)
    {
        let mut notice = ValidationNotice::new(
            "feed_valid_beyond_total_service_window",
            NoticeSeverity::Info,
            "feed validity extends more than 14 days beyond its service window",
        );
        notice.insert_context_field("feedStartDate", feed_start.to_string());
        notice.insert_context_field("feedEndDate", feed_end.to_string());
        notice.insert_context_field("serviceWindowStartDate", total_start.to_string());
        notice.insert_context_field("serviceWindowEndDate", total_end.to_string());
        notice.field_order = vec![
            "feedStartDate".into(),
            "feedEndDate".into(),
            "serviceWindowStartDate".into(),
            "serviceWindowEndDate".into(),
        ];
        notices.push(notice);
    }
}

fn validate_future_feed(feed: &GtfsFeed, notices: &mut NoticeContainer) {
    let Some(feed_info) = &feed.feed_info else {
        return;
    };
    let Some(min_start) = feed_info
        .rows
        .iter()
        .filter_map(|row| row.feed_start_date.and_then(gtfs_date_to_naive))
        .min()
    else {
        return;
    };
    let today = crate::validation_date();
    if min_start <= today {
        return;
    }
    let mut notice = ValidationNotice::new(
        "future_feed",
        NoticeSeverity::Info,
        "feed_info indicates that the feed covers the future only",
    );
    notice.insert_context_field("feedStartDate", min_start.format("%Y%m%d").to_string());
    notice.insert_context_field("currentDate", today.format("%Y%m%d").to_string());
    notice.field_order = vec!["feedStartDate".into(), "currentDate".into()];
    notices.push(notice);
}

fn build_active_dates(feed: &GtfsFeed) -> BTreeMap<StringId, BTreeSet<NaiveDate>> {
    let mut result = BTreeMap::new();
    if let Some(calendar) = &feed.calendar {
        for service in &calendar.rows {
            if service.service_id.0 == 0 {
                continue;
            }
            let entry = result
                .entry(service.service_id)
                .or_insert_with(BTreeSet::new);
            let (Some(mut date), Some(end)) = (
                gtfs_date_to_naive(service.start_date),
                gtfs_date_to_naive(service.end_date),
            ) else {
                continue;
            };
            while date <= end {
                if active_on_weekday(service, date) {
                    entry.insert(date);
                }
                let Some(next) = date.succ_opt() else {
                    break;
                };
                date = next;
            }
        }
    }
    if let Some(calendar_dates) = &feed.calendar_dates {
        for exception in &calendar_dates.rows {
            if exception.service_id.0 == 0 {
                continue;
            }
            let Some(date) = gtfs_date_to_naive(exception.date) else {
                continue;
            };
            let entry = result
                .entry(exception.service_id)
                .or_insert_with(BTreeSet::new);
            match exception.exception_type {
                ExceptionType::Added => {
                    entry.insert(date);
                }
                ExceptionType::Removed => {
                    entry.remove(&date);
                }
                ExceptionType::Other => {}
            }
        }
    }
    result
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

fn active_on_weekday(service: &gtfs_guru_model::Calendar, date: NaiveDate) -> bool {
    let availability = match date.weekday() {
        chrono::Weekday::Mon => service.monday,
        chrono::Weekday::Tue => service.tuesday,
        chrono::Weekday::Wed => service.wednesday,
        chrono::Weekday::Thu => service.thursday,
        chrono::Weekday::Fri => service.friday,
        chrono::Weekday::Sat => service.saturday,
        chrono::Weekday::Sun => service.sunday,
    };
    availability == ServiceAvailability::Available
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Calendar, CalendarDate, FeedInfo, Trip};

    #[test]
    fn emits_all_v8_service_window_notices() {
        let _guard = crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()));
        let mut feed = GtfsFeed::default();
        let service_id = feed.pool.intern("SVC");
        feed.calendar = Some(CsvTable {
            headers: vec!["service_id".into()],
            rows: vec![Calendar {
                service_id,
                monday: ServiceAvailability::Available,
                tuesday: ServiceAvailability::Available,
                wednesday: ServiceAvailability::Available,
                thursday: ServiceAvailability::Available,
                friday: ServiceAvailability::Available,
                saturday: ServiceAvailability::Available,
                sunday: ServiceAvailability::Available,
                start_date: GtfsDate::parse("20250101").unwrap(),
                end_date: GtfsDate::parse("20270102").unwrap(),
            }],
            row_numbers: vec![2],
        });
        let removed_dates = (2..=20)
            .map(|day| CalendarDate {
                service_id,
                date: GtfsDate::parse(&format!("202501{day:02}")).unwrap(),
                exception_type: ExceptionType::Removed,
            })
            .collect();
        feed.calendar_dates = Some(CsvTable {
            headers: vec!["service_id".into(), "date".into(), "exception_type".into()],
            rows: removed_dates,
            row_numbers: Vec::new(),
        });
        feed.trips.rows = vec![Trip {
            trip_id: feed.pool.intern("T1"),
            service_id,
            ..Default::default()
        }];
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_start_date".into(), "feed_end_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Publisher".into(),
                feed_publisher_url: feed.pool.intern("https://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: Some(GtfsDate::parse("20250201").unwrap()),
                feed_end_date: Some(GtfsDate::parse("20280101").unwrap()),
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        ServiceWindowsV8Validator.validate(&feed, &mut notices);
        let codes: HashSet<_> = notices.iter().map(|notice| notice.code.as_str()).collect();

        for expected in [
            "big_gap_in_service",
            "service_extends_far_in_the_future",
            "future_calendar",
            "service_window_outside_feed_period",
            "feed_valid_beyond_total_service_window",
            "future_feed",
        ] {
            assert!(codes.contains(expected), "missing {expected}");
        }
    }
}
