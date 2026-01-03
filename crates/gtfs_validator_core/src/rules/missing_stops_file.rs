use crate::feed::STOPS_FILE;
use crate::{GtfsFeed, NoticeContainer, Validator};

#[derive(Debug, Default)]
pub struct MissingStopsFileValidator;

impl Validator for MissingStopsFileValidator {
    fn name(&self) -> &'static str {
        "missing_stops_file"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !feed.stops.headers.is_empty() || !feed.stops.rows.is_empty() {
            return;
        }
        if feed.locations.is_some() {
            return;
        }
        notices.push_missing_file(STOPS_FILE);
    }
}

