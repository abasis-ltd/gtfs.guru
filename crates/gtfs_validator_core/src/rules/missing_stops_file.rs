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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notice::NOTICE_CODE_MISSING_FILE;
    use crate::{CsvTable, GtfsFeed, NoticeContainer};

    #[test]
    fn detects_missing_stops_file_when_no_locations() {
        let feed = GtfsFeed {
            stops: CsvTable::default(),
            locations: None,
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        MissingStopsFileValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            NOTICE_CODE_MISSING_FILE
        );
    }

    #[test]
    fn passes_when_stops_file_present() {
        let feed = GtfsFeed {
            stops: CsvTable {
                headers: vec!["stop_id".into()],
                rows: vec![gtfs_guru_model::Stop::default()],
                row_numbers: vec![2],
            },
            locations: None,
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        MissingStopsFileValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn passes_when_locations_present() {
        let feed = GtfsFeed {
            stops: CsvTable::default(),
            locations: Some(crate::geojson::LocationsGeoJson::default()),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        MissingStopsFileValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
