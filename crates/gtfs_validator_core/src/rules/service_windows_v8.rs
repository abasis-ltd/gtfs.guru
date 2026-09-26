use std::collections::{BTreeMap, HashMap, HashSet};

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
    active_dates: &HashMap<StringId, ServiceActivity>,
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
        let Some(activity) = active_dates.get(&service.service_id) else {
            continue;
        };
        for &(previous_date, date, gap) in &activity.big_gaps {
            let mut notice = ValidationNotice::new(
                "big_gap_in_service",
                NoticeSeverity::Info,
                "service has a gap of more than 13 days between active dates",
            );
            notice
                .insert_context_field("serviceId", feed.pool.resolve(service.service_id).as_str());
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
        let last_active = activity.last;
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
    active_dates: &HashMap<StringId, ServiceActivity>,
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
            let activity = active_dates.get(service_id)?;
            Some((*service_id, activity.first, activity.last))
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

/// What the service-window checks need from a service's active dates: the
/// first, the last, and every gap longer than [`MAX_GAP_DAYS`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceActivity {
    first: NaiveDate,
    last: NaiveDate,
    /// `(previous active date, next active date, inactive days between)`.
    big_gaps: Vec<(NaiveDate, NaiveDate, i64)>,
}

/// Summarises each service's active dates without listing them day by day, so
/// a calendar row running to 99991231 costs no more than one running a week.
///
/// calendar.txt rows are merged into disjoint date ranges, each with the
/// weekdays that any row covering it runs on. Inside such a range two
/// consecutive active dates are at most six days apart, so a gap over
/// [`MAX_GAP_DAYS`] can only open around calendar_dates.txt exceptions or
/// between ranges; the walk visits the range ends and the exceptions only.
/// The result equals expanding every row into dates and then applying the
/// exceptions in file order.
fn build_active_dates(feed: &GtfsFeed) -> HashMap<StringId, ServiceActivity> {
    let mut ranges_by_service: HashMap<StringId, Vec<(NaiveDate, NaiveDate, u8)>> = HashMap::new();
    if let Some(calendar) = &feed.calendar {
        for service in &calendar.rows {
            if service.service_id.0 == 0 {
                continue;
            }
            let entry = ranges_by_service.entry(service.service_id).or_default();
            let (Some(start), Some(end)) = (
                gtfs_date_to_naive(service.start_date),
                gtfs_date_to_naive(service.end_date),
            ) else {
                continue;
            };
            if start <= end {
                entry.push((start, end, weekday_mask(service)));
            }
        }
    }

    // The last Added or Removed exception for a date decides it.
    let mut overrides_by_service: HashMap<StringId, BTreeMap<NaiveDate, bool>> = HashMap::new();
    if let Some(calendar_dates) = &feed.calendar_dates {
        for exception in &calendar_dates.rows {
            if exception.service_id.0 == 0 {
                continue;
            }
            let Some(date) = gtfs_date_to_naive(exception.date) else {
                continue;
            };
            let overrides = overrides_by_service
                .entry(exception.service_id)
                .or_default();
            match exception.exception_type {
                ExceptionType::Added => {
                    overrides.insert(date, true);
                }
                ExceptionType::Removed => {
                    overrides.insert(date, false);
                }
                ExceptionType::Other => {}
            }
        }
    }

    let mut service_ids: Vec<StringId> = ranges_by_service.keys().copied().collect();
    service_ids.extend(
        overrides_by_service
            .keys()
            .filter(|id| !ranges_by_service.contains_key(id)),
    );

    let empty_overrides = BTreeMap::new();
    let mut result = HashMap::new();
    for service_id in service_ids {
        let ranges = ranges_by_service
            .get(&service_id)
            .map(|rows| merge_calendar_ranges(rows))
            .unwrap_or_default();
        let overrides = overrides_by_service
            .get(&service_id)
            .unwrap_or(&empty_overrides);
        if let Some(activity) = summarize_activity(&ranges, overrides) {
            result.insert(service_id, activity);
        }
    }
    result
}

fn weekday_mask(service: &gtfs_guru_model::Calendar) -> u8 {
    [
        service.monday,
        service.tuesday,
        service.wednesday,
        service.thursday,
        service.friday,
        service.saturday,
        service.sunday,
    ]
    .iter()
    .enumerate()
    .filter(|(_, availability)| **availability == ServiceAvailability::Available)
    .fold(0, |mask, (bit, _)| mask | (1 << bit))
}

fn runs_on(mask: u8, date: NaiveDate) -> bool {
    mask & (1 << date.weekday().num_days_from_monday()) != 0
}

/// Splits overlapping calendar rows into disjoint, sorted ranges that each
/// carry the union of the weekdays of the rows covering them. Ranges that run
/// on no weekday are dropped.
fn merge_calendar_ranges(rows: &[(NaiveDate, NaiveDate, u8)]) -> Vec<(NaiveDate, NaiveDate, u8)> {
    // (date, row starts here?, mask): a row covers [start, end + 1).
    let mut events: Vec<(NaiveDate, bool, u8)> = Vec::with_capacity(rows.len() * 2);
    for &(start, end, mask) in rows {
        events.push((start, true, mask));
        if let Some(after_end) = end.succ_opt() {
            events.push((after_end, false, mask));
        }
    }
    events.sort_unstable_by_key(|(date, _, _)| *date);

    let mut counts = [0usize; 7];
    let mut merged = Vec::new();
    let mut index = 0;
    while index < events.len() {
        let date = events[index].0;
        while index < events.len() && events[index].0 == date {
            let (_, starts, mask) = events[index];
            for (bit, count) in counts.iter_mut().enumerate() {
                if mask & (1 << bit) != 0 {
                    if starts {
                        *count += 1;
                    } else {
                        *count -= 1;
                    }
                }
            }
            index += 1;
        }
        let mask = counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .fold(0u8, |mask, (bit, _)| mask | (1 << bit));
        let (Some(next), true) = (events.get(index).map(|event| event.0), mask != 0) else {
            continue;
        };
        if let Some(until) = next.pred_opt() {
            merged.push((date, until, mask));
        }
    }
    merged
}

#[derive(Default)]
struct ActivityWalk {
    first: Option<NaiveDate>,
    last: Option<NaiveDate>,
    big_gaps: Vec<(NaiveDate, NaiveDate, i64)>,
}

impl ActivityWalk {
    /// Records an active date later than every date recorded so far.
    fn visit(&mut self, date: NaiveDate) {
        match self.last {
            Some(previous) => {
                let gap = (date - previous).num_days() - 1;
                if gap > MAX_GAP_DAYS {
                    self.big_gaps.push((previous, date, gap));
                }
            }
            None => self.first = Some(date),
        }
        self.last = Some(date);
    }

    /// Records every date in `from..=to` that runs on `mask`. Consecutive such
    /// dates are at most six days apart, so only the ends matter.
    fn visit_weekly(&mut self, from: NaiveDate, to: NaiveDate, mask: u8) {
        let first = from
            .iter_days()
            .take(7)
            .take_while(|date| *date <= to)
            .find(|date| runs_on(mask, *date));
        let Some(first) = first else {
            return;
        };
        let last = std::iter::successors(Some(to), |date| date.pred_opt())
            .take(7)
            .find(|date| runs_on(mask, *date))
            .unwrap_or(first);
        self.visit(first);
        self.last = Some(last);
    }
}

fn summarize_activity(
    ranges: &[(NaiveDate, NaiveDate, u8)],
    overrides: &BTreeMap<NaiveDate, bool>,
) -> Option<ServiceActivity> {
    let mut walk = ActivityWalk::default();
    let mut pending = overrides.iter().peekable();
    for &(from, to, mask) in ranges {
        while let Some((&date, &active)) = pending.next_if(|(date, _)| **date < from) {
            if active {
                walk.visit(date);
            }
        }
        let mut cursor = Some(from);
        while let Some((&date, &active)) = pending.next_if(|(date, _)| **date <= to) {
            if let (Some(start), Some(end)) = (cursor, date.pred_opt()) {
                if start <= end {
                    walk.visit_weekly(start, end, mask);
                }
            }
            if active {
                walk.visit(date);
            }
            cursor = date.succ_opt();
        }
        if let Some(start) = cursor {
            if start <= to {
                walk.visit_weekly(start, to, mask);
            }
        }
    }
    for (&date, &active) in pending {
        if active {
            walk.visit(date);
        }
    }
    Some(ServiceActivity {
        first: walk.first?,
        last: walk.last?,
        big_gaps: walk.big_gaps,
    })
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
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

    /// The day-by-day expansion the summary replaces.
    fn expand_day_by_day(
        rows: &[(NaiveDate, NaiveDate, u8)],
        exceptions: &[(NaiveDate, ExceptionType)],
    ) -> Option<ServiceActivity> {
        let mut dates = std::collections::BTreeSet::new();
        for &(start, end, mask) in rows {
            let mut date = start;
            while date <= end {
                if runs_on(mask, date) {
                    dates.insert(date);
                }
                date = date.succ_opt().unwrap();
            }
        }
        for &(date, exception_type) in exceptions {
            match exception_type {
                ExceptionType::Added => {
                    dates.insert(date);
                }
                ExceptionType::Removed => {
                    dates.remove(&date);
                }
                ExceptionType::Other => {}
            }
        }
        let mut walk = ActivityWalk::default();
        for date in dates {
            walk.visit(date);
        }
        Some(ServiceActivity {
            first: walk.first?,
            last: walk.last?,
            big_gaps: walk.big_gaps,
        })
    }

    #[test]
    fn summary_matches_day_by_day_expansion() {
        let base = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        for _ in 0..2000 {
            let rows: Vec<_> = (0..next(4))
                .map(|_| {
                    let start = base + chrono::Duration::days(next(200) as i64);
                    let end = start + chrono::Duration::days(next(120) as i64);
                    // Sparse masks make big gaps likely.
                    let mask = match next(4) {
                        0 => 0u8,
                        1 => 1 << next(7),
                        2 => (1 << next(7)) | (1 << next(7)),
                        _ => next(128) as u8,
                    };
                    (start, end, mask)
                })
                .collect();
            let exceptions: Vec<_> = (0..next(12))
                .map(|_| {
                    let date = base + chrono::Duration::days(next(360) as i64 - 20);
                    let exception_type = match next(5) {
                        0 => ExceptionType::Other,
                        1 | 2 => ExceptionType::Added,
                        _ => ExceptionType::Removed,
                    };
                    (date, exception_type)
                })
                .collect();

            let mut overrides = BTreeMap::new();
            for &(date, exception_type) in &exceptions {
                match exception_type {
                    ExceptionType::Added => {
                        overrides.insert(date, true);
                    }
                    ExceptionType::Removed => {
                        overrides.insert(date, false);
                    }
                    ExceptionType::Other => {}
                }
            }
            let valid_rows: Vec<_> = rows
                .iter()
                .copied()
                .filter(|(start, end, _)| start <= end)
                .collect();
            assert_eq!(
                summarize_activity(&merge_calendar_ranges(&valid_rows), &overrides),
                expand_day_by_day(&rows, &exceptions),
                "rows {rows:?} exceptions {exceptions:?}"
            );
        }
    }

    #[test]
    fn open_ended_calendar_is_summarised_without_expansion() {
        let _guard = crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2025, 6, 1).unwrap()));
        let mut feed = GtfsFeed::default();
        let rows = (0..200)
            .map(|index| Calendar {
                service_id: feed.pool.intern(&format!("SVC{index}")),
                monday: ServiceAvailability::Available,
                start_date: GtfsDate::parse("20250101").unwrap(),
                end_date: GtfsDate::parse("99991231").unwrap(),
                ..Default::default()
            })
            .collect();
        feed.calendar = Some(CsvTable {
            headers: vec!["service_id".into()],
            rows,
            row_numbers: Vec::new(),
        });

        let started = std::time::Instant::now();
        let activity = build_active_dates(&feed);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));

        let summary = &activity[&feed.pool.intern("SVC0")];
        assert_eq!(summary.first, NaiveDate::from_ymd_opt(2025, 1, 6).unwrap());
        assert_eq!(summary.last, NaiveDate::from_ymd_opt(9999, 12, 27).unwrap());
        assert!(summary.big_gaps.is_empty());
    }
}
