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

    fn empty_table<T>() -> crate::CsvTable<T> {
        crate::CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}
