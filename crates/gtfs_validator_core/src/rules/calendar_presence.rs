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
        GtfsFeed {
            agency: empty_table(),
            stops: empty_table(),
            routes: empty_table(),
            trips: empty_table(),
            stop_times: empty_table(),
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
            route_networks: None,
        }
    }

    fn empty_table<T>() -> CsvTable<T> {
        CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}
