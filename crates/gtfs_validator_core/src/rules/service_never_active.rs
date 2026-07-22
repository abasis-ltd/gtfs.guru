use std::collections::HashSet;

use crate::feed::CALENDAR_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{ExceptionType, ServiceAvailability, StringId};

const CODE_SERVICE_NEVER_ACTIVE: &str = "service_never_active";

/// Flags calendar.txt rows whose weekday flags are all zero and whose
/// service_id has no added dates in calendar_dates.txt. All-zero calendars
/// are a valid pattern when service is defined entirely through added dates,
/// so only rows without any exception_type=1 entry are reported.
#[derive(Debug, Default)]
pub struct ServiceNeverActiveValidator;

impl Validator for ServiceNeverActiveValidator {
    fn name(&self) -> &'static str {
        "service_never_active"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(calendar) = &feed.calendar else {
            return;
        };

        let mut services_with_added_dates: Option<HashSet<StringId>> = None;

        for (index, row) in calendar.rows.iter().enumerate() {
            let all_days_unavailable = [
                row.monday,
                row.tuesday,
                row.wednesday,
                row.thursday,
                row.friday,
                row.saturday,
                row.sunday,
            ]
            .iter()
            .all(|day| *day == ServiceAvailability::Unavailable);
            if !all_days_unavailable {
                continue;
            }

            let added = services_with_added_dates.get_or_insert_with(|| {
                feed.calendar_dates
                    .as_ref()
                    .map(|dates| {
                        dates
                            .rows
                            .iter()
                            .filter(|date| date.exception_type == ExceptionType::Added)
                            .map(|date| date.service_id)
                            .collect()
                    })
                    .unwrap_or_default()
            });
            if added.contains(&row.service_id) {
                continue;
            }

            let row_number = calendar.row_number(index);
            let service_id_value = feed.pool.resolve(row.service_id);
            let mut notice = ValidationNotice::new(
                CODE_SERVICE_NEVER_ACTIVE,
                NoticeSeverity::Warning,
                "calendar row has no active weekdays and no added dates in calendar_dates.txt",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("filename", CALENDAR_FILE);
            notice.insert_context_field("serviceId", service_id_value.as_str());
            notice.field_order = vec!["csvRowNumber".into(), "filename".into(), "serviceId".into()];
            notices.push(notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Calendar, CalendarDate, GtfsDate};

    fn all_zero_calendar(feed: &mut GtfsFeed, service_id: &str) -> Calendar {
        Calendar {
            service_id: feed.pool.intern(service_id),
            start_date: GtfsDate::parse("20260101").unwrap(),
            end_date: GtfsDate::parse("20261231").unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn flags_all_zero_calendar_without_added_dates() {
        let mut feed = GtfsFeed::default();
        let row = all_zero_calendar(&mut feed, "S1");
        feed.calendar = Some(CsvTable {
            rows: vec![row],
            row_numbers: vec![2],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        ServiceNeverActiveValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_SERVICE_NEVER_ACTIVE);
    }

    #[test]
    fn skips_all_zero_calendar_with_added_dates() {
        let mut feed = GtfsFeed::default();
        let row = all_zero_calendar(&mut feed, "S1");
        let service_id = row.service_id;
        feed.calendar = Some(CsvTable {
            rows: vec![row],
            row_numbers: vec![2],
            ..Default::default()
        });
        feed.calendar_dates = Some(CsvTable {
            rows: vec![CalendarDate {
                service_id,
                date: GtfsDate::parse("20260601").unwrap(),
                exception_type: ExceptionType::Added,
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        ServiceNeverActiveValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn flags_all_zero_calendar_with_only_removed_dates() {
        let mut feed = GtfsFeed::default();
        let row = all_zero_calendar(&mut feed, "S1");
        let service_id = row.service_id;
        feed.calendar = Some(CsvTable {
            rows: vec![row],
            row_numbers: vec![2],
            ..Default::default()
        });
        feed.calendar_dates = Some(CsvTable {
            rows: vec![CalendarDate {
                service_id,
                date: GtfsDate::parse("20260601").unwrap(),
                exception_type: ExceptionType::Removed,
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        ServiceNeverActiveValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn skips_calendar_with_active_weekday() {
        let mut feed = GtfsFeed::default();
        let mut row = all_zero_calendar(&mut feed, "S1");
        row.wednesday = ServiceAvailability::Available;
        feed.calendar = Some(CsvTable {
            rows: vec![row],
            row_numbers: vec![2],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        ServiceNeverActiveValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
