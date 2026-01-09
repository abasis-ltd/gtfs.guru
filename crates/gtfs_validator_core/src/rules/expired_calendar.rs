use std::collections::{BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{ExceptionType, GtfsDate, ServiceAvailability, StringId};

const CODE_EXPIRED_CALENDAR: &str = "expired_calendar";

#[derive(Debug, Default)]
pub struct ExpiredCalendarValidator;

struct ServiceDates {
    dates_by_service: HashMap<StringId, BTreeSet<NaiveDate>>,
    calendar_row_by_service_id: HashMap<StringId, u64>,
    calendar_date_row_by_service_id: HashMap<StringId, u64>,
}

impl Validator for ExpiredCalendarValidator {
    fn name(&self) -> &'static str {
        "expired_calendar"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let service_dates = build_service_dates(feed);
        let validation_date = crate::validation_date();

        let is_calendar_empty = feed
            .calendar
            .as_ref()
            .map_or(true, |table| table.rows.is_empty());

        let mut notices_to_return: Vec<ValidationNotice> = Vec::new();
        let mut expired_calendar_dates_notices: Vec<ValidationNotice> = Vec::new();
        let mut all_calendar_expired = true;

        for (service_id, dates) in &service_dates.dates_by_service {
            let Some(last_date) = dates.iter().last().copied() else {
                continue;
            };
            if last_date < validation_date {
                if let Some(row_number) = service_dates.calendar_row_by_service_id.get(service_id) {
                    notices_to_return.push(expired_notice(feed, *row_number, *service_id));
                } else if is_calendar_empty && all_calendar_expired {
                    let row_number = service_dates
                        .calendar_date_row_by_service_id
                        .get(service_id)
                        .copied()
                        .unwrap_or(0);
                    expired_calendar_dates_notices.push(expired_notice(
                        feed,
                        row_number,
                        *service_id,
                    ));
                }
            } else {
                all_calendar_expired = false;
            }
        }

        if is_calendar_empty && all_calendar_expired {
            notices_to_return.extend(expired_calendar_dates_notices);
        }

        notices_to_return.sort_by_key(notice_csv_row_number);
        for notice in notices_to_return {
            notices.push(notice);
        }
    }
}

fn expired_notice(feed: &GtfsFeed, row_number: u64, service_id: StringId) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_EXPIRED_CALENDAR,
        NoticeSeverity::Warning,
        "service dates are expired",
    );
    let service_id_value = feed.pool.resolve(service_id);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("serviceId", service_id_value.as_str());
    notice.field_order = vec!["csvRowNumber".into(), "serviceId".into()];
    notice
}

fn build_service_dates(feed: &GtfsFeed) -> ServiceDates {
    let mut dates_by_service: HashMap<StringId, BTreeSet<NaiveDate>> = HashMap::new();
    let mut calendar_row_by_service_id: HashMap<StringId, u64> = HashMap::new();
    let mut calendar_date_row_by_service_id: HashMap<StringId, u64> = HashMap::new();

    if let Some(calendar) = &feed.calendar {
        for (index, row) in calendar.rows.iter().enumerate() {
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
            calendar_row_by_service_id
                .entry(service_id)
                .or_insert(calendar.row_number(index));
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
        for (index, row) in calendar_dates.rows.iter().enumerate() {
            let Some(date) = gtfs_date_to_naive(row.date) else {
                continue;
            };
            let service_id = row.service_id;
            if service_id.0 == 0 {
                continue;
            }
            let row_number = calendar_dates.row_number(index);
            calendar_date_row_by_service_id
                .entry(service_id)
                .and_modify(|existing| {
                    if row_number < *existing {
                        *existing = row_number;
                    }
                })
                .or_insert(row_number);
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

    ServiceDates {
        dates_by_service,
        calendar_row_by_service_id,
        calendar_date_row_by_service_id,
    }
}

fn notice_csv_row_number(notice: &ValidationNotice) -> u64 {
    notice
        .context
        .get("csvRowNumber")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

fn service_available_on_date(calendar: &gtfs_guru_model::Calendar, date: NaiveDate) -> bool {
    match date.weekday() {
        chrono::Weekday::Mon => is_available(calendar.monday),
        chrono::Weekday::Tue => is_available(calendar.tuesday),
        chrono::Weekday::Wed => is_available(calendar.wednesday),
        chrono::Weekday::Thu => is_available(calendar.thursday),
        chrono::Weekday::Fri => is_available(calendar.friday),
        chrono::Weekday::Sat => is_available(calendar.saturday),
        chrono::Weekday::Sun => is_available(calendar.sunday),
    }
}

fn is_available(availability: ServiceAvailability) -> bool {
    matches!(availability, ServiceAvailability::Available)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn emits_notice_for_expired_calendar() {
        let today = chrono::Utc::now().date_naive();
        let past = today - chrono::Duration::days(10);

        let mut feed = base_feed();
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![calendar_row("SVC1", past, past, &feed)],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        ExpiredCalendarValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_EXPIRED_CALENDAR);
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "serviceId"), "SVC1");
    }

    #[test]
    fn passes_when_calendar_not_expired() {
        let today = chrono::Utc::now().date_naive();
        let future = today + chrono::Duration::days(10);

        let mut feed = base_feed();
        feed.calendar = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![calendar_row("SVC1", today, future, &feed)],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        ExpiredCalendarValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn emits_notice_for_expired_calendar_dates_when_calendar_missing() {
        let today = chrono::Utc::now().date_naive();
        let past = today - chrono::Duration::days(1);

        let mut feed = base_feed();
        feed.calendar = None;
        feed.calendar_dates = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::CalendarDate {
                service_id: feed.pool.intern("SVC1"),
                date: gtfs_date(past),
                exception_type: ExceptionType::Added,
            }],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        ExpiredCalendarValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_EXPIRED_CALENDAR);
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "serviceId"), "SVC1");
    }

    #[test]
    fn skips_calendar_dates_when_not_all_expired() {
        let today = chrono::Utc::now().date_naive();
        let past = today - chrono::Duration::days(1);
        let future = today + chrono::Duration::days(1);

        let mut feed = base_feed();
        feed.calendar = None;
        feed.calendar_dates = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![
                gtfs_guru_model::CalendarDate {
                    service_id: feed.pool.intern("SVC1"),
                    date: gtfs_date(past),
                    exception_type: ExceptionType::Added,
                },
                gtfs_guru_model::CalendarDate {
                    service_id: feed.pool.intern("SVC2"),
                    date: gtfs_date(future),
                    exception_type: ExceptionType::Added,
                },
            ],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        ExpiredCalendarValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn calendar_row(
        service_id: &str,
        start: NaiveDate,
        end: NaiveDate,
        feed: &GtfsFeed,
    ) -> gtfs_guru_model::Calendar {
        gtfs_guru_model::Calendar {
            service_id: feed.pool.intern(service_id),
            monday: ServiceAvailability::Available,
            tuesday: ServiceAvailability::Available,
            wednesday: ServiceAvailability::Available,
            thursday: ServiceAvailability::Available,
            friday: ServiceAvailability::Available,
            saturday: ServiceAvailability::Available,
            sunday: ServiceAvailability::Available,
            start_date: gtfs_date(start),
            end_date: gtfs_date(end),
        }
    }

    fn gtfs_date(date: NaiveDate) -> GtfsDate {
        GtfsDate::parse(&date.format("%Y%m%d").to_string()).expect("date")
    }

    fn base_feed() -> GtfsFeed {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency".into(),
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                agency_lang: None,
                agency_phone: None,
                agency_fare_url: None,
                agency_email: None,
            }],
            row_numbers: Vec::new(),
        };
        feed.stops = CsvTable::default();
        feed.routes = CsvTable::default();
        feed.trips = CsvTable::default();
        feed.stop_times = CsvTable::default();
        feed
    }

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }

    fn context_str<'a>(notice: &'a ValidationNotice, key: &str) -> &'a str {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("")
    }
}
