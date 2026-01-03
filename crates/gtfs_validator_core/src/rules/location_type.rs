use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::LocationType;

const CODE_STATION_WITH_PARENT_STATION: &str = "station_with_parent_station";
const CODE_LOCATION_WITHOUT_PARENT_STATION: &str = "location_without_parent_station";
const CODE_PLATFORM_WITHOUT_PARENT_STATION: &str = "platform_without_parent_station";

#[derive(Debug, Default)]
pub struct LocationTypeValidator;

impl Validator for LocationTypeValidator {
    fn name(&self) -> &'static str {
        "location_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            let parent_station = stop
                .parent_station
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty());

            if parent_station.is_some() {
                if location_type == LocationType::Station {
                    notices.push(station_with_parent_station_notice(stop, row_number));
                }
                continue;
            }

            match location_type {
                LocationType::StopOrPlatform => {
                    if has_platform_code(stop.platform_code.as_deref()) {
                        notices.push(platform_without_parent_station_notice(stop, row_number));
                    }
                }
                LocationType::EntranceOrExit
                | LocationType::GenericNode
                | LocationType::BoardingArea => {
                    notices.push(location_without_parent_station_notice(stop, row_number));
                }
                _ => {}
            }
        }
    }
}

fn has_platform_code(value: Option<&str>) -> bool {
    value.map(|val| !val.trim().is_empty()).unwrap_or(false)
}

fn station_with_parent_station_notice(
    stop: &gtfs_model::Stop,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STATION_WITH_PARENT_STATION,
        NoticeSeverity::Error,
        "station must not have parent_station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field(
        "parentStation",
        stop.parent_station.as_deref().unwrap_or(""),
    );
    notice.insert_context_field("stopId", stop.stop_id.as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "parentStation".to_string(),
        "stopId".to_string(),
        "stopName".to_string(),
    ];
    notice
}

fn location_without_parent_station_notice(
    stop: &gtfs_model::Stop,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_LOCATION_WITHOUT_PARENT_STATION,
        NoticeSeverity::Error,
        "location requires parent_station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("locationType", location_type_value(stop.location_type));
    notice.insert_context_field("stopId", stop.stop_id.as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "locationType".to_string(),
        "stopId".to_string(),
        "stopName".to_string(),
    ];
    notice
}

fn platform_without_parent_station_notice(
    stop: &gtfs_model::Stop,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_PLATFORM_WITHOUT_PARENT_STATION,
        NoticeSeverity::Info,
        "platform has no parent_station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("stopId", stop.stop_id.as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "stopId".to_string(),
        "stopName".to_string(),
    ];
    notice
}

fn location_type_value(value: Option<LocationType>) -> i32 {
    match value.unwrap_or(LocationType::StopOrPlatform) {
        LocationType::StopOrPlatform => 0,
        LocationType::Station => 1,
        LocationType::EntranceOrExit => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Other => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn emits_notice_for_station_with_parent_station() {
        let feed = feed_with_stops(vec![gtfs_model::Stop {
                        
            stop_id: "STATION1".to_string(),
            stop_code: None,
            stop_name: Some("Station".to_string()),
            tts_stop_name: None,
            stop_desc: None,
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            zone_id: None,
            stop_url: None,
            location_type: Some(LocationType::Station),
            parent_station: Some("PARENT".to_string()),
            stop_timezone: None,
            wheelchair_boarding: None,
            level_id: None,
            platform_code: None,
            stop_address: None,
            stop_city: None,
            stop_region: None,
            stop_postcode: None,
            stop_country: None,
            stop_phone: None,
            ..Default::default()
        }]);

        let mut notices = NoticeContainer::new();
        LocationTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_STATION_WITH_PARENT_STATION));
        let notice = notices
            .iter()
            .find(|notice| notice.code == CODE_STATION_WITH_PARENT_STATION)
            .unwrap();
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "parentStation"), "PARENT");
        assert_eq!(context_str(notice, "stopId"), "STATION1");
        assert_eq!(context_str(notice, "stopName"), "Station");
    }

    #[test]
    fn emits_notice_for_location_missing_parent_station() {
        let feed = feed_with_stops(vec![gtfs_model::Stop {
                        
            stop_id: "ENTRANCE1".to_string(),
            stop_code: None,
            stop_name: Some("Entrance".to_string()),
            tts_stop_name: None,
            stop_desc: None,
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            zone_id: None,
            stop_url: None,
            location_type: Some(LocationType::EntranceOrExit),
            parent_station: None,
            stop_timezone: None,
            wheelchair_boarding: None,
            level_id: None,
            platform_code: None,
            stop_address: None,
            stop_city: None,
            stop_region: None,
            stop_postcode: None,
            stop_country: None,
            stop_phone: None,
            ..Default::default()
        }]);

        let mut notices = NoticeContainer::new();
        LocationTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_LOCATION_WITHOUT_PARENT_STATION));
        let notice = notices
            .iter()
            .find(|notice| notice.code == CODE_LOCATION_WITHOUT_PARENT_STATION)
            .unwrap();
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_i64(notice, "locationType"), 2);
        assert_eq!(context_str(notice, "stopId"), "ENTRANCE1");
        assert_eq!(context_str(notice, "stopName"), "Entrance");
    }

    #[test]
    fn emits_notice_for_platform_without_parent_station() {
        let feed = feed_with_stops(vec![gtfs_model::Stop {
                        
            stop_id: "STOP1".to_string(),
            stop_code: None,
            stop_name: Some("Platform".to_string()),
            tts_stop_name: None,
            stop_desc: None,
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            zone_id: None,
            stop_url: None,
            location_type: Some(LocationType::StopOrPlatform),
            parent_station: None,
            stop_timezone: None,
            wheelchair_boarding: None,
            level_id: None,
            platform_code: Some("PLAT".to_string()),
            stop_address: None,
            stop_city: None,
            stop_region: None,
            stop_postcode: None,
            stop_country: None,
            stop_phone: None,
            ..Default::default()
        }]);

        let mut notices = NoticeContainer::new();
        LocationTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_PLATFORM_WITHOUT_PARENT_STATION));
        let notice = notices
            .iter()
            .find(|notice| notice.code == CODE_PLATFORM_WITHOUT_PARENT_STATION)
            .unwrap();
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "stopId"), "STOP1");
        assert_eq!(context_str(notice, "stopName"), "Platform");
    }

    #[test]
    fn skips_stop_without_parent_station() {
        let feed = feed_with_stops(vec![gtfs_model::Stop {
                        
            stop_id: "STOP1".to_string(),
            stop_code: None,
            stop_name: Some("Stop".to_string()),
            tts_stop_name: None,
            stop_desc: None,
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            zone_id: None,
            stop_url: None,
            location_type: Some(LocationType::StopOrPlatform),
            parent_station: None,
            stop_timezone: None,
            wheelchair_boarding: None,
            level_id: None,
            platform_code: None,
            stop_address: None,
            stop_city: None,
            stop_region: None,
            stop_postcode: None,
            stop_country: None,
            stop_phone: None,
            ..Default::default()
        }]);

        let mut notices = NoticeContainer::new();
        LocationTypeValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn feed_with_stops(stops: Vec<gtfs_model::Stop>) -> GtfsFeed {
        GtfsFeed {
            agency: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_model::Agency {
                    agency_id: None,
                    agency_name: "Agency".to_string(),
                    agency_url: "https://example.com".to_string(),
                    agency_timezone: "UTC".to_string(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: Vec::new(),
            },
            stops: CsvTable {
                headers: Vec::new(),
                rows: stops,
                row_numbers: Vec::new(),
            },
            routes: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            trips: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            stop_times: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
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

    fn context_str<'a>(notice: &'a ValidationNotice, key: &str) -> &'a str {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("")
    }

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }

    fn context_i64(notice: &ValidationNotice, key: &str) -> i64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_i64())
            .unwrap_or(0)
    }
}
