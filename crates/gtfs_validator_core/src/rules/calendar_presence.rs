use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_CALENDAR_FILES: &str = "missing_calendar_and_calendar_date_files";

#[derive(Debug, Default)]
pub struct CalendarPresenceValidator;

impl Validator for CalendarPresenceValidator {
    fn name(&self) -> &'static str {
        "calendar_presence"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.calendar.is_none() && feed.calendar_dates.is_none() {
            let notice = ValidationNotice::new(
                CODE_MISSING_CALENDAR_FILES,
                NoticeSeverity::Error,
                "missing calendar.txt and calendar_dates.txt",
            );
            notices.push(notice);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn emits_notice_when_both_calendar_tables_missing() {
        let feed = dummy_feed();
        let mut notices = NoticeContainer::new();

        CalendarPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_CALENDAR_FILES
        );
    }

    #[test]
    fn passes_when_calendar_present() {
        let mut feed = dummy_feed();
        feed.calendar = Some(empty_table());
        let mut notices = NoticeContainer::new();

        CalendarPresenceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn dummy_feed() -> GtfsFeed {
        let mut feed = GtfsFeed::default();
        feed.agency = empty_table();
        feed.stops = empty_table();
        feed.routes = empty_table();
        feed.trips = empty_table();
        feed.stop_times = empty_table();
        feed
    }

    fn empty_table<T>() -> CsvTable<T> {
        CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}
