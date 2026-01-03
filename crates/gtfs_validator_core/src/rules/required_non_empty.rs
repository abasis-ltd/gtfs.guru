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

