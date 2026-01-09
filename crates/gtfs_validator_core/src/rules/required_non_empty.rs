use crate::validation_context::thorough_mode_enabled;
use crate::{
    feed::{AGENCY_FILE, ROUTES_FILE, STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE},
    GtfsFeed, NoticeContainer, Validator,
};

#[derive(Debug, Default)]
pub struct RequiredTablesNotEmptyValidator;

impl Validator for RequiredTablesNotEmptyValidator {
    fn name(&self) -> &'static str {
        "required_tables_not_empty"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Only run in thorough mode to match Java default behavior
        if !thorough_mode_enabled() {
            return;
        }
        if feed.agency.rows.is_empty() {
            notices.push_empty_table(AGENCY_FILE);
        }
        if feed.stops.rows.is_empty() && !feed.stops.headers.is_empty() {
            notices.push_empty_table(STOPS_FILE);
        }
        if feed.routes.rows.is_empty() {
            notices.push_empty_table(ROUTES_FILE);
        }
        if feed.trips.rows.is_empty() {
            notices.push_empty_table(TRIPS_FILE);
        }
        if feed.stop_times.rows.is_empty() {
            notices.push_empty_table(STOP_TIMES_FILE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notice::NOTICE_CODE_EMPTY_TABLE;
    use crate::{GtfsFeed, NoticeContainer};
    use gtfs_guru_model::{Agency, Route, Stop, StopTime, Trip};

    #[test]
    fn detects_empty_required_tables() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        // CsvTable::default() has empty rows and empty headers by default.
        // For stops.txt, it only emits NOTICE_CODE_EMPTY_TABLE if headers are not empty.
        feed.stops.headers = vec!["stop_id".into()];

        let mut notices = NoticeContainer::new();
        RequiredTablesNotEmptyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 5);
        let codes: Vec<_> = notices.iter().map(|n| n.code.as_str()).collect();
        assert!(codes.iter().all(|&c| c == NOTICE_CODE_EMPTY_TABLE));
    }

    #[test]
    fn passes_when_tables_are_not_empty() {
        let mut feed = GtfsFeed::default();
        feed.agency.rows = vec![Agency::default()];
        feed.stops.rows = vec![Stop::default()];
        feed.routes.rows = vec![Route::default()];
        feed.trips.rows = vec![Trip::default()];
        feed.stop_times.rows = vec![StopTime::default()];

        let mut notices = NoticeContainer::new();
        RequiredTablesNotEmptyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
