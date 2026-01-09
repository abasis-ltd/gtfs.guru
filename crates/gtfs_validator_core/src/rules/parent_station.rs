use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_WRONG_PARENT_LOCATION_TYPE: &str = "wrong_parent_location_type";
const CODE_UNUSED_STATION: &str = "unused_station";
const CODE_STATION_WITH_PARENT_STATION: &str = "station_with_parent_station";
const CODE_PARENT_STATION_REQUIRED: &str = "parent_station_required";
const CODE_STOP_TOO_FAR_FROM_PARENT_STATION: &str = "stop_too_far_from_parent_station";
const STOP_TOO_FAR_FROM_PARENT_STATION_THRESHOLD_METERS: f64 = 1000.0;

#[derive(Debug, Default)]
pub struct ParentStationValidator;

impl Validator for ParentStationValidator {
    fn name(&self) -> &'static str {
        "parent_station"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        let mut rows_by_id: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            rows_by_id.insert(stop_id, feed.stops.row_number(index));
        }

        let mut stations: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        let mut stations_with_stops: HashSet<gtfs_guru_model::StringId> = HashSet::new();

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let location_type = normalized_location_type(stop.location_type);
            if location_type == LocationType::Station {
                stations.insert(stop.stop_id);
                // Check if station has a parent_station (which is invalid)
                if let Some(parent_station) = stop.parent_station.filter(|id| id.0 != 0) {
                    let parent_station_value = feed.pool.resolve(parent_station);
                    let mut notice = ValidationNotice::new(
                        CODE_STATION_WITH_PARENT_STATION,
                        NoticeSeverity::Error,
                        "station must not have a parent_station",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice.insert_context_field("parentStation", parent_station_value.as_str());
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "stopId".into(),
                        "parentStation".into(),
                    ];
                    notices.push(notice);
                }
                continue;
            }

            let parent_station = match stop.parent_station.filter(|id| id.0 != 0) {
                Some(parent_station) => parent_station,
                None => {
                    if requires_parent_station(location_type) {
                        let mut notice = ValidationNotice::new(
                            CODE_PARENT_STATION_REQUIRED,
                            NoticeSeverity::Error,
                            "parent_station is required for this location type",
                        );
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field(
                            "stopId",
                            feed.pool.resolve(stop.stop_id).as_str(),
                        );
                        notice.insert_context_field(
                            "locationType",
                            location_type_value(location_type),
                        );
                        notice.field_order = vec![
                            "csvRowNumber".into(),
                            "stopId".into(),
                            "locationType".into(),
                        ];
                        notices.push(notice);
                    }
                    continue;
                }
            };

            let parent_stop = match stops_by_id.get(&parent_station) {
                Some(stop) => *stop,
                None => continue,
            };

            if location_type == LocationType::StopOrPlatform {
                stations_with_stops.insert(parent_station);
            }

            // Distance check
            if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) = (
                stop.stop_lat,
                stop.stop_lon,
                parent_stop.stop_lat,
                parent_stop.stop_lon,
            ) {
                let distance_m = haversine_m(lat1, lon1, lat2, lon2);
                if distance_m > STOP_TOO_FAR_FROM_PARENT_STATION_THRESHOLD_METERS
                    && distance_m < 500_000.0
                {
                    let parent_station_value = feed.pool.resolve(parent_station);
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_TOO_FAR_FROM_PARENT_STATION,
                        NoticeSeverity::Warning,
                        "stop is too far from its parent station",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice
                        .insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                    notice.insert_context_field("parentStation", parent_station_value.as_str());
                    notice.insert_context_field(
                        "parentStopName",
                        parent_stop.stop_name.as_deref().unwrap_or(""),
                    );
                    notice.insert_context_field("distanceInMeters", distance_m);
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "stopId".into(),
                        "stopName".into(),
                        "parentStation".into(),
                        "parentStopName".into(),
                        "distanceInMeters".into(),
                    ];
                    notices.push(notice);
                }
            }

            if let Some(expected) = expected_parent_location_type(location_type) {
                let parent_location_type = normalized_location_type(parent_stop.location_type);
                if parent_location_type != expected {
                    let parent_station_value = feed.pool.resolve(parent_station);
                    let mut notice = ValidationNotice::new(
                        CODE_WRONG_PARENT_LOCATION_TYPE,
                        NoticeSeverity::Error,
                        "parent_station has invalid location_type",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field(
                        "expectedLocationType",
                        location_type_value(expected),
                    );
                    notice.insert_context_field("locationType", location_type_value(location_type));
                    notice.insert_context_field(
                        "parentCsvRowNumber",
                        rows_by_id.get(&parent_station).copied().unwrap_or(2),
                    );
                    notice.insert_context_field(
                        "parentLocationType",
                        location_type_value(parent_location_type),
                    );
                    notice.insert_context_field("parentStation", parent_station_value.as_str());
                    notice.insert_context_field(
                        "parentStopName",
                        parent_stop.stop_name.as_deref().unwrap_or(""),
                    );
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice
                        .insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "expectedLocationType".into(),
                        "locationType".into(),
                        "parentCsvRowNumber".into(),
                        "parentLocationType".into(),
                        "parentStation".into(),
                        "parentStopName".into(),
                        "stopId".into(),
                        "stopName".into(),
                    ];
                    notices.push(notice);
                }
            }
        }

        for station_id in stations.difference(&stations_with_stops) {
            let station_id = *station_id;
            let Some(station_stop) = stops_by_id.get(&station_id) else {
                continue;
            };
            let row_number = rows_by_id.get(&station_id).copied().unwrap_or(2);
            let station_id_value = feed.pool.resolve(station_id);
            let mut notice = ValidationNotice::new(
                CODE_UNUSED_STATION,
                NoticeSeverity::Info,
                "station is not referenced by any stop",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("stopId", station_id_value.as_str());
            notice
                .insert_context_field("stopName", station_stop.stop_name.as_deref().unwrap_or(""));
            notice.field_order = vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
            notices.push(notice);
        }
    }
}

fn normalized_location_type(location_type: Option<LocationType>) -> LocationType {
    location_type.unwrap_or(LocationType::StopOrPlatform)
}

fn requires_parent_station(location_type: LocationType) -> bool {
    matches!(
        location_type,
        LocationType::EntranceOrExit | LocationType::GenericNode | LocationType::BoardingArea
    )
}

fn expected_parent_location_type(location_type: LocationType) -> Option<LocationType> {
    match location_type {
        LocationType::StopOrPlatform | LocationType::EntranceOrExit | LocationType::GenericNode => {
            Some(LocationType::Station)
        }
        LocationType::BoardingArea => Some(LocationType::StopOrPlatform),
        _ => None,
    }
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_m = 6371000.0;
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    radius_m * c
}

#[allow(dead_code)]
fn unused_parent_station_notice(
    row_number: u64,
    stop_id: &str,
    stop_name: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unused_parent_station",
        NoticeSeverity::Info,
        "unused parent station",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("stopId", stop_id);
    notice.insert_context_field("stopName", stop_name);
    notice.field_order = vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
    notice
}

fn location_type_value(location_type: LocationType) -> i32 {
    match location_type {
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
    fn emits_notice_for_wrong_parent_type() {
        let mut feed = GtfsFeed::default();
        let stops = vec![
            station("STATION1", &feed),
            stop_with_parent(
                "STOP1",
                "STATION1",
                Some(LocationType::StopOrPlatform),
                &feed,
            ),
            stop_with_parent("STOP2", "STOP1", Some(LocationType::StopOrPlatform), &feed),
        ];
        feed_with_stops(stops, &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_WRONG_PARENT_LOCATION_TYPE));
        let notice = notices
            .iter()
            .find(|notice| notice.code == CODE_WRONG_PARENT_LOCATION_TYPE)
            .unwrap();
        assert_eq!(context_u64(notice, "csvRowNumber"), 4);
        assert_eq!(context_i64(notice, "expectedLocationType"), 1);
        assert_eq!(context_i64(notice, "locationType"), 0);
        assert_eq!(context_u64(notice, "parentCsvRowNumber"), 3);
        assert_eq!(context_i64(notice, "parentLocationType"), 0);
        assert_eq!(context_str(notice, "parentStation"), "STOP1");
        assert_eq!(context_str(notice, "parentStopName"), "Stop");
        assert_eq!(context_str(notice, "stopId"), "STOP2");
        assert_eq!(context_str(notice, "stopName"), "Stop");
    }

    #[test]
    fn emits_unused_station_notice() {
        let mut feed = GtfsFeed::default();
        let stops = vec![station("STATION1", &feed)];
        feed_with_stops(stops, &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_UNUSED_STATION));
        let notice = notices
            .iter()
            .find(|notice| notice.code == CODE_UNUSED_STATION)
            .unwrap();
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "stopId"), "STATION1");
        assert_eq!(context_str(notice, "stopName"), "Station");
    }

    #[test]
    fn skips_when_station_has_stop() {
        let mut feed = GtfsFeed::default();
        let stops = vec![
            station("STATION1", &feed),
            stop_with_parent(
                "STOP1",
                "STATION1",
                Some(LocationType::StopOrPlatform),
                &feed,
            ),
        ];
        feed_with_stops(stops, &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(!notices
            .iter()
            .any(|notice| notice.code == CODE_UNUSED_STATION));
    }

    #[test]
    fn emits_notice_for_station_with_parent() {
        let mut feed = GtfsFeed::default();
        let mut station1 = station("STATION1", &feed);
        station1.parent_station = Some(feed.pool.intern("STATION2"));
        let stops = vec![station1, station("STATION2", &feed)];
        feed_with_stops(stops, &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STATION_WITH_PARENT_STATION));
    }

    #[test]
    fn emits_notice_for_missing_required_parent() {
        let mut feed = GtfsFeed::default();
        let mut entrance =
            stop_with_parent("ENTRANCE1", "", Some(LocationType::EntranceOrExit), &feed);
        entrance.parent_station = None;
        feed_with_stops(vec![entrance], &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_PARENT_STATION_REQUIRED));
    }

    #[test]
    fn emits_notice_for_stop_too_far() {
        let mut feed = GtfsFeed::default();
        let mut station1 = station("STATION1", &feed);
        station1.stop_lat = Some(0.0);
        station1.stop_lon = Some(0.0);

        let mut stop1 = stop_with_parent(
            "STOP1",
            "STATION1",
            Some(LocationType::StopOrPlatform),
            &feed,
        );
        stop1.stop_lat = Some(1.0); // Very far
        stop1.stop_lon = Some(1.0);

        feed_with_stops(vec![station1, stop1], &mut feed);

        let mut notices = NoticeContainer::new();
        ParentStationValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_STOP_TOO_FAR_FROM_PARENT_STATION));
    }

    fn station(stop_id: &str, feed: &GtfsFeed) -> gtfs_guru_model::Stop {
        gtfs_guru_model::Stop {
            stop_id: feed.pool.intern(stop_id),
            stop_name: Some("Station".into()),
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            location_type: Some(LocationType::Station),
            ..Default::default()
        }
    }

    fn stop_with_parent(
        stop_id: &str,
        parent_station: &str,
        location_type: Option<LocationType>,
        feed: &GtfsFeed,
    ) -> gtfs_guru_model::Stop {
        gtfs_guru_model::Stop {
            stop_id: feed.pool.intern(stop_id),
            stop_name: Some("Stop".into()),
            stop_lat: Some(10.0),
            stop_lon: Some(20.0),
            location_type,
            parent_station: Some(feed.pool.intern(parent_station)),
            ..Default::default()
        }
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
