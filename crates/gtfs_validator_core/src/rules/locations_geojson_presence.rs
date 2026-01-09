use crate::feed::LOCATIONS_GEOJSON_FILE;
use crate::{GtfsFeed, NoticeContainer, Validator};

#[derive(Debug, Default)]
pub struct LocationsGeoJsonPresenceValidator;

impl Validator for LocationsGeoJsonPresenceValidator {
    fn name(&self) -> &'static str {
        "locations_geojson_presence"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.locations.is_some() {
            return;
        }

        let has_location_id_header = feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("location_id"));
        if !has_location_id_header {
            return;
        }

        let has_location_id_value = feed
            .stop_times
            .rows
            .iter()
            .any(|stop_time| stop_time.location_id.map(|id| id.0 != 0).unwrap_or(false));

        if has_location_id_value {
            notices.push_missing_file(LOCATIONS_GEOJSON_FILE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geojson::LocationsGeoJson;
    use crate::{CsvTable, NoticeContainer};
    use gtfs_guru_model::StopTime;

    #[test]
    fn detects_missing_locations_geojson() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into(), "location_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: Some(feed.pool.intern("L1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.locations = None;

        let mut notices = NoticeContainer::new();
        LocationsGeoJsonPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "missing_required_file");
        assert_eq!(
            notices.iter().next().unwrap().message,
            "missing required GTFS file"
        );
    }

    #[test]
    fn passes_when_locations_geojson_present() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into(), "location_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: Some(feed.pool.intern("L1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.locations = Some(LocationsGeoJson::default());

        let mut notices = NoticeContainer::new();
        LocationsGeoJsonPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn passes_when_no_location_id_used() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into(), "location_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.locations = None;

        let mut notices = NoticeContainer::new();
        LocationsGeoJsonPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
