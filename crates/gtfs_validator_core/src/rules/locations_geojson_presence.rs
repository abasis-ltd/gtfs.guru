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

        let has_location_id_value = feed.stop_times.rows.iter().any(|stop_time| {
            stop_time
                .location_id
                .as_deref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .is_some()
        });

        if has_location_id_value {
            notices.push_missing_file(LOCATIONS_GEOJSON_FILE);
        }
    }
}

