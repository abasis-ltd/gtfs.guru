use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;
use gtfs_guru_model::StringId;

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
            let parent_station = stop.parent_station.filter(|id| id.0 != 0);

            if parent_station.is_some() {
                if location_type == LocationType::Station {
                    notices.push(station_with_parent_station_notice(stop, row_number, feed));
                }
                continue;
            }

            match location_type {
                LocationType::StopOrPlatform => {
                    if has_platform_code(stop.platform_code.as_deref()) {
                        notices.push(platform_without_parent_station_notice(
                            stop, row_number, feed,
                        ));
                    }
                }
                LocationType::EntranceOrExit
                | LocationType::GenericNode
                | LocationType::BoardingArea => {
                    notices.push(location_without_parent_station_notice(
                        stop, row_number, feed,
                    ));
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
    stop: &gtfs_guru_model::Stop,
    row_number: u64,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_STATION_WITH_PARENT_STATION,
        NoticeSeverity::Error,
        "station must not have parent_station",
    );
    let parent_station = feed
        .pool
        .resolve(stop.parent_station.unwrap_or(StringId(0)));
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("parentStation", parent_station.as_str());
    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec![
        "csvRowNumber".into(),
        "parentStation".into(),
        "stopId".into(),
        "stopName".into(),
    ];
    notice
}

fn location_without_parent_station_notice(
    stop: &gtfs_guru_model::Stop,
    row_number: u64,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_LOCATION_WITHOUT_PARENT_STATION,
        NoticeSeverity::Error,
        "location requires parent_station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("locationType", location_type_value(stop.location_type));
    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec![
        "csvRowNumber".into(),
        "locationType".into(),
        "stopId".into(),
        "stopName".into(),
    ];
    notice
}

fn platform_without_parent_station_notice(
    stop: &gtfs_guru_model::Stop,
    row_number: u64,
    feed: &GtfsFeed,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_PLATFORM_WITHOUT_PARENT_STATION,
        NoticeSeverity::Info,
        "platform has no parent_station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
    notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
    notice.field_order = vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
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
        let mut feed = GtfsFeed::default();
        let stop = gtfs_guru_model::Stop {
            stop_id: feed.pool.intern("STATION1"),
            stop_code: None,
            stop_name: Some("Station".into()),
            tts_stop_name: None,
            stop_desc: None,
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            zone_id: None,
            stop_url: None,
            location_type: Some(LocationType::Station),
            parent_station: Some(feed.pool.intern("PARENT")),
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
        };
        feed_with_stops(vec![stop], &mut feed);

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
        let mut feed = GtfsFeed::default();
        let stop = gtfs_guru_model::Stop {
            stop_id: feed.pool.intern("ENTRANCE1"),
            stop_code: None,
            stop_name: Some("Entrance".into()),
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
        };
        feed_with_stops(vec![stop], &mut feed);

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
        let mut feed = GtfsFeed::default();
        let stop = gtfs_guru_model::Stop {
            stop_id: feed.pool.intern("STOP1"),
            stop_code: None,
            stop_name: Some("Platform".into()),
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
            platform_code: Some("PLAT".into()),
            stop_address: None,
            stop_city: None,
            stop_region: None,
            stop_postcode: None,
            stop_country: None,
            stop_phone: None,
            ..Default::default()
        };
        feed_with_stops(vec![stop], &mut feed);

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
        let mut feed = GtfsFeed::default();
        let stop = gtfs_guru_model::Stop {
            stop_id: feed.pool.intern("STOP1"),
            stop_code: None,
            stop_name: Some("Stop".into()),
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
        };
        feed_with_stops(vec![stop], &mut feed);

        let mut notices = NoticeContainer::new();
        LocationTypeValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn feed_with_stops(stops: Vec<gtfs_guru_model::Stop>, feed: &mut GtfsFeed) {
        feed.agency = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Agency {
                agency_id: None,
                agency_name: "Agency".into(),
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                agency_lang: None,
                agency_phone: None,
                agency_fare_url: None,
                agency_email: None,
            }],
            row_numbers: Vec::new(),
        };
        feed.stops = CsvTable {
            headers: Vec::new(),
            rows: stops,
            row_numbers: Vec::new(),
        };
        feed.routes = CsvTable::default();
        feed.trips = CsvTable::default();
        feed.stop_times = CsvTable::default();
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
