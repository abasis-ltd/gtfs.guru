use crate::{GtfsFeed, NoticeContainer, Validator};

#[derive(Debug, Default)]
pub struct LocationsGeoJsonNoticesValidator;

impl Validator for LocationsGeoJsonNoticesValidator {
    fn name(&self) -> &'static str {
        "locations_geojson_notices"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(locations) = feed.locations.as_ref() else {
            return;
        };

        for notice in &locations.notices {
            notices.push(notice.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geojson::LocationsGeoJson;
    use crate::{NoticeSeverity, ValidationNotice};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn emits_geojson_notices() {
        let notice = ValidationNotice::new(
            "geo_json_unknown_element",
            NoticeSeverity::Info,
            "unknown element",
        );
        let mut feed = dummy_feed();
        feed.locations = Some(LocationsGeoJson {
            location_ids: HashSet::new(),
            bounds_by_id: HashMap::new(),
            feature_index_by_id: HashMap::new(),
            notices: vec![notice.clone()],
        });

        let mut notices = NoticeContainer::new();
        LocationsGeoJsonNoticesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, notice.code);
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

    fn empty_table<T>() -> crate::CsvTable<T> {
        crate::CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}
