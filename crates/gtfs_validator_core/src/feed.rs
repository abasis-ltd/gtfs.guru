use gtfs_guru_model::{
    Agency, Area, Attribution, BookingRules, Calendar, CalendarDate, FareAttribute,
    FareLegJoinRule, FareLegRule, FareMedia, FareProduct, FareRule, FareTransferRule, FeedInfo,
    Frequency, Level, LocationGroup, LocationGroupStop, Network, Pathway, RiderCategory, Route,
    RouteNetwork, Shape, Stop, StopArea, StopTime, Timeframe, Transfer, Translation, Trip,
};

use crate::geojson::{GeoJsonFeatureCollection, LocationsGeoJson};
use crate::input::GtfsBytesReader;
use crate::{CsvTable, GtfsInput, GtfsInputError, GtfsInputReader, NoticeContainer};

pub const AGENCY_FILE: &str = "agency.txt";
pub const STOPS_FILE: &str = "stops.txt";
pub const ROUTES_FILE: &str = "routes.txt";
pub const TRIPS_FILE: &str = "trips.txt";
pub const STOP_TIMES_FILE: &str = "stop_times.txt";
pub const CALENDAR_FILE: &str = "calendar.txt";
pub const CALENDAR_DATES_FILE: &str = "calendar_dates.txt";
pub const FARE_ATTRIBUTES_FILE: &str = "fare_attributes.txt";
pub const FARE_RULES_FILE: &str = "fare_rules.txt";
pub const FARE_MEDIA_FILE: &str = "fare_media.txt";
pub const FARE_PRODUCTS_FILE: &str = "fare_products.txt";
pub const FARE_LEG_RULES_FILE: &str = "fare_leg_rules.txt";
pub const FARE_TRANSFER_RULES_FILE: &str = "fare_transfer_rules.txt";
pub const FARE_LEG_JOIN_RULES_FILE: &str = "fare_leg_join_rules.txt";
pub const AREAS_FILE: &str = "areas.txt";
pub const STOP_AREAS_FILE: &str = "stop_areas.txt";
pub const TIMEFRAMES_FILE: &str = "timeframes.txt";
pub const RIDER_CATEGORIES_FILE: &str = "rider_categories.txt";
pub const SHAPES_FILE: &str = "shapes.txt";
pub const FREQUENCIES_FILE: &str = "frequencies.txt";
pub const TRANSFERS_FILE: &str = "transfers.txt";
pub const LOCATION_GROUPS_FILE: &str = "location_groups.txt";
pub const LOCATION_GROUP_STOPS_FILE: &str = "location_group_stops.txt";
pub const LOCATIONS_GEOJSON_FILE: &str = "locations.geojson";
pub const BOOKING_RULES_FILE: &str = "booking_rules.txt";
pub const NETWORKS_FILE: &str = "networks.txt";
pub const ROUTE_NETWORKS_FILE: &str = "route_networks.txt";
pub const FEED_INFO_FILE: &str = "feed_info.txt";
pub const ATTRIBUTIONS_FILE: &str = "attributions.txt";
pub const LEVELS_FILE: &str = "levels.txt";
pub const PATHWAYS_FILE: &str = "pathways.txt";
pub const TRANSLATIONS_FILE: &str = "translations.txt";

pub const GTFS_FILE_NAMES: &[&str] = &[
    AGENCY_FILE,
    STOPS_FILE,
    ROUTES_FILE,
    TRIPS_FILE,
    STOP_TIMES_FILE,
    CALENDAR_FILE,
    CALENDAR_DATES_FILE,
    FARE_ATTRIBUTES_FILE,
    FARE_RULES_FILE,
    FARE_MEDIA_FILE,
    FARE_PRODUCTS_FILE,
    FARE_LEG_RULES_FILE,
    FARE_TRANSFER_RULES_FILE,
    FARE_LEG_JOIN_RULES_FILE,
    AREAS_FILE,
    STOP_AREAS_FILE,
    TIMEFRAMES_FILE,
    RIDER_CATEGORIES_FILE,
    SHAPES_FILE,
    FREQUENCIES_FILE,
    TRANSFERS_FILE,
    LOCATION_GROUPS_FILE,
    LOCATION_GROUP_STOPS_FILE,
    LOCATIONS_GEOJSON_FILE,
    BOOKING_RULES_FILE,
    NETWORKS_FILE,
    ROUTE_NETWORKS_FILE,
    FEED_INFO_FILE,
    ATTRIBUTIONS_FILE,
    LEVELS_FILE,
    PATHWAYS_FILE,
    TRANSLATIONS_FILE,
];

#[derive(Debug, Clone, Default)]
pub struct GtfsFeed {
    pub agency: CsvTable<Agency>,
    pub stops: CsvTable<Stop>,
    pub routes: CsvTable<Route>,
    pub trips: CsvTable<Trip>,
    pub stop_times: CsvTable<StopTime>,
    pub calendar: Option<CsvTable<Calendar>>,
    pub calendar_dates: Option<CsvTable<CalendarDate>>,
    pub fare_attributes: Option<CsvTable<FareAttribute>>,
    pub fare_rules: Option<CsvTable<FareRule>>,
    pub fare_media: Option<CsvTable<FareMedia>>,
    pub fare_products: Option<CsvTable<FareProduct>>,
    pub fare_leg_rules: Option<CsvTable<FareLegRule>>,
    pub fare_transfer_rules: Option<CsvTable<FareTransferRule>>,
    pub fare_leg_join_rules: Option<CsvTable<FareLegJoinRule>>,
    pub areas: Option<CsvTable<Area>>,
    pub stop_areas: Option<CsvTable<StopArea>>,
    pub timeframes: Option<CsvTable<Timeframe>>,
    pub rider_categories: Option<CsvTable<RiderCategory>>,
    pub shapes: Option<CsvTable<Shape>>,
    pub frequencies: Option<CsvTable<Frequency>>,
    pub transfers: Option<CsvTable<Transfer>>,
    pub location_groups: Option<CsvTable<LocationGroup>>,
    pub location_group_stops: Option<CsvTable<LocationGroupStop>>,
    pub locations: Option<LocationsGeoJson>,
    pub booking_rules: Option<CsvTable<BookingRules>>,
    pub networks: Option<CsvTable<Network>>,
    pub route_networks: Option<CsvTable<RouteNetwork>>,
    pub feed_info: Option<CsvTable<FeedInfo>>,
    pub attributions: Option<CsvTable<Attribution>>,
    pub levels: Option<CsvTable<Level>>,
    pub pathways: Option<CsvTable<Pathway>>,
    pub translations: Option<CsvTable<Translation>>,
}

impl GtfsFeed {
    pub fn from_input(input: &GtfsInput) -> Result<Self, GtfsInputError> {
        let mut notices = NoticeContainer::new();
        Self::from_input_with_notices(input, &mut notices)
    }

    pub fn from_reader(reader: &GtfsInputReader) -> Result<Self, GtfsInputError> {
        let mut notices = NoticeContainer::new();
        Self::from_reader_with_notices(reader, &mut notices)
    }

    pub fn from_input_with_notices(
        input: &GtfsInput,
        notices: &mut NoticeContainer,
    ) -> Result<Self, GtfsInputError> {
        let reader = input.reader();
        Self::from_reader_with_notices(&reader, notices)
    }

    pub fn from_reader_with_notices(
        reader: &GtfsInputReader,
        notices: &mut NoticeContainer,
    ) -> Result<Self, GtfsInputError> {
        let agency = reader
            .read_optional_csv_with_notices(AGENCY_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(AGENCY_FILE);
                CsvTable::default()
            });
        let stops = reader
            .read_optional_csv_with_notices(STOPS_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(STOPS_FILE);
                CsvTable::default()
            });
        let routes = reader
            .read_optional_csv_with_notices(ROUTES_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(ROUTES_FILE);
                CsvTable::default()
            });
        let trips = reader
            .read_optional_csv_with_notices(TRIPS_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(TRIPS_FILE);
                CsvTable::default()
            });
        let stop_times = reader
            .read_optional_csv_with_notices(STOP_TIMES_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(STOP_TIMES_FILE);
                CsvTable::default()
            });

        let calendar = reader.read_optional_csv_with_notices(CALENDAR_FILE, notices)?;
        let calendar_dates = reader.read_optional_csv_with_notices(CALENDAR_DATES_FILE, notices)?;
        let fare_attributes =
            reader.read_optional_csv_with_notices(FARE_ATTRIBUTES_FILE, notices)?;
        let fare_rules = reader.read_optional_csv_with_notices(FARE_RULES_FILE, notices)?;
        let fare_media = reader.read_optional_csv_with_notices(FARE_MEDIA_FILE, notices)?;
        let fare_products = reader.read_optional_csv_with_notices(FARE_PRODUCTS_FILE, notices)?;
        let fare_leg_rules = reader.read_optional_csv_with_notices(FARE_LEG_RULES_FILE, notices)?;
        let fare_transfer_rules =
            reader.read_optional_csv_with_notices(FARE_TRANSFER_RULES_FILE, notices)?;
        let fare_leg_join_rules =
            reader.read_optional_csv_with_notices(FARE_LEG_JOIN_RULES_FILE, notices)?;
        let areas = reader.read_optional_csv_with_notices(AREAS_FILE, notices)?;
        let stop_areas = reader.read_optional_csv_with_notices(STOP_AREAS_FILE, notices)?;
        let timeframes = reader.read_optional_csv_with_notices(TIMEFRAMES_FILE, notices)?;
        let rider_categories =
            reader.read_optional_csv_with_notices(RIDER_CATEGORIES_FILE, notices)?;
        let shapes = reader.read_optional_csv_with_notices(SHAPES_FILE, notices)?;
        let frequencies = reader.read_optional_csv_with_notices(FREQUENCIES_FILE, notices)?;
        let transfers = reader.read_optional_csv_with_notices(TRANSFERS_FILE, notices)?;
        let location_groups =
            reader.read_optional_csv_with_notices(LOCATION_GROUPS_FILE, notices)?;
        let location_group_stops =
            reader.read_optional_csv_with_notices(LOCATION_GROUP_STOPS_FILE, notices)?;
        let locations =
            match reader.read_optional_json::<GeoJsonFeatureCollection>(LOCATIONS_GEOJSON_FILE) {
                Ok(data) => data.map(LocationsGeoJson::from),
                Err(GtfsInputError::Json { file, source }) if file == LOCATIONS_GEOJSON_FILE => {
                    Some(LocationsGeoJson::malformed_json(source.to_string()))
                }
                Err(err) => return Err(err),
            };
        let booking_rules = reader.read_optional_csv_with_notices(BOOKING_RULES_FILE, notices)?;
        let networks = reader.read_optional_csv_with_notices(NETWORKS_FILE, notices)?;
        let route_networks = reader.read_optional_csv_with_notices(ROUTE_NETWORKS_FILE, notices)?;
        let feed_info = reader.read_optional_csv_with_notices(FEED_INFO_FILE, notices)?;
        let attributions = reader.read_optional_csv_with_notices(ATTRIBUTIONS_FILE, notices)?;
        let levels = reader.read_optional_csv_with_notices(LEVELS_FILE, notices)?;
        let pathways = reader.read_optional_csv_with_notices(PATHWAYS_FILE, notices)?;
        let translations = reader.read_optional_csv_with_notices(TRANSLATIONS_FILE, notices)?;

        Ok(Self {
            agency,
            stops,
            routes,
            trips,
            stop_times,
            calendar,
            calendar_dates,
            fare_attributes,
            fare_rules,
            fare_media,
            fare_products,
            fare_leg_rules,
            fare_transfer_rules,
            fare_leg_join_rules,
            areas,
            stop_areas,
            timeframes,
            rider_categories,
            shapes,
            frequencies,
            transfers,
            location_groups,
            location_group_stops,
            locations,
            booking_rules,
            networks,
            route_networks,
            feed_info,
            attributions,
            levels,
            pathways,
            translations,
        })
    }

    /// Load GTFS feed from in-memory bytes (for WASM compatibility)
    pub fn from_bytes_reader(reader: &GtfsBytesReader) -> Result<Self, GtfsInputError> {
        let mut notices = NoticeContainer::new();
        Self::from_bytes_reader_with_notices(reader, &mut notices)
    }

    /// Load GTFS feed from in-memory bytes with notice collection
    pub fn from_bytes_reader_with_notices(
        reader: &GtfsBytesReader,
        notices: &mut NoticeContainer,
    ) -> Result<Self, GtfsInputError> {
        let agency = reader
            .read_optional_csv_with_notices(AGENCY_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(AGENCY_FILE);
                CsvTable::default()
            });
        let stops = reader
            .read_optional_csv_with_notices(STOPS_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(STOPS_FILE);
                CsvTable::default()
            });
        let routes = reader
            .read_optional_csv_with_notices(ROUTES_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(ROUTES_FILE);
                CsvTable::default()
            });
        let trips = reader
            .read_optional_csv_with_notices(TRIPS_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(TRIPS_FILE);
                CsvTable::default()
            });
        let stop_times = reader
            .read_optional_csv_with_notices(STOP_TIMES_FILE, notices)?
            .unwrap_or_else(|| {
                notices.push_missing_file(STOP_TIMES_FILE);
                CsvTable::default()
            });

        let calendar = reader.read_optional_csv_with_notices(CALENDAR_FILE, notices)?;
        let calendar_dates = reader.read_optional_csv_with_notices(CALENDAR_DATES_FILE, notices)?;
        let fare_attributes =
            reader.read_optional_csv_with_notices(FARE_ATTRIBUTES_FILE, notices)?;
        let fare_rules = reader.read_optional_csv_with_notices(FARE_RULES_FILE, notices)?;
        let fare_media = reader.read_optional_csv_with_notices(FARE_MEDIA_FILE, notices)?;
        let fare_products = reader.read_optional_csv_with_notices(FARE_PRODUCTS_FILE, notices)?;
        let fare_leg_rules = reader.read_optional_csv_with_notices(FARE_LEG_RULES_FILE, notices)?;
        let fare_transfer_rules =
            reader.read_optional_csv_with_notices(FARE_TRANSFER_RULES_FILE, notices)?;
        let fare_leg_join_rules =
            reader.read_optional_csv_with_notices(FARE_LEG_JOIN_RULES_FILE, notices)?;
        let areas = reader.read_optional_csv_with_notices(AREAS_FILE, notices)?;
        let stop_areas = reader.read_optional_csv_with_notices(STOP_AREAS_FILE, notices)?;
        let timeframes = reader.read_optional_csv_with_notices(TIMEFRAMES_FILE, notices)?;
        let rider_categories =
            reader.read_optional_csv_with_notices(RIDER_CATEGORIES_FILE, notices)?;
        let shapes = reader.read_optional_csv_with_notices(SHAPES_FILE, notices)?;
        let frequencies = reader.read_optional_csv_with_notices(FREQUENCIES_FILE, notices)?;
        let transfers = reader.read_optional_csv_with_notices(TRANSFERS_FILE, notices)?;
        let location_groups =
            reader.read_optional_csv_with_notices(LOCATION_GROUPS_FILE, notices)?;
        let location_group_stops =
            reader.read_optional_csv_with_notices(LOCATION_GROUP_STOPS_FILE, notices)?;
        let locations =
            match reader.read_optional_json::<GeoJsonFeatureCollection>(LOCATIONS_GEOJSON_FILE) {
                Ok(data) => data.map(LocationsGeoJson::from),
                Err(GtfsInputError::Json { file, source }) if file == LOCATIONS_GEOJSON_FILE => {
                    Some(LocationsGeoJson::malformed_json(source.to_string()))
                }
                Err(err) => return Err(err),
            };
        let booking_rules = reader.read_optional_csv_with_notices(BOOKING_RULES_FILE, notices)?;
        let networks = reader.read_optional_csv_with_notices(NETWORKS_FILE, notices)?;
        let route_networks = reader.read_optional_csv_with_notices(ROUTE_NETWORKS_FILE, notices)?;
        let feed_info = reader.read_optional_csv_with_notices(FEED_INFO_FILE, notices)?;
        let attributions = reader.read_optional_csv_with_notices(ATTRIBUTIONS_FILE, notices)?;
        let levels = reader.read_optional_csv_with_notices(LEVELS_FILE, notices)?;
        let pathways = reader.read_optional_csv_with_notices(PATHWAYS_FILE, notices)?;
        let translations = reader.read_optional_csv_with_notices(TRANSLATIONS_FILE, notices)?;

        Ok(Self {
            agency,
            stops,
            routes,
            trips,
            stop_times,
            calendar,
            calendar_dates,
            fare_attributes,
            fare_rules,
            fare_media,
            fare_products,
            fare_leg_rules,
            fare_transfer_rules,
            fare_leg_join_rules,
            areas,
            stop_areas,
            timeframes,
            rider_categories,
            shapes,
            frequencies,
            transfers,
            location_groups,
            location_group_stops,
            locations,
            booking_rules,
            networks,
            route_networks,
            feed_info,
            attributions,
            levels,
            pathways,
            translations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
    }

    fn write_file(dir: &std::path::Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).expect("write file");
    }

    #[test]
    fn loads_required_tables_from_directory() {
        let dir = temp_dir("gtfs_feed");
        fs::create_dir_all(&dir).expect("create dir");

        write_file(
            &dir,
            AGENCY_FILE,
            "agency_name,agency_url,agency_timezone\nTest Agency,https://example.com,UTC\n",
        );
        write_file(&dir, STOPS_FILE, "stop_id\nSTOP1\n");
        write_file(&dir, ROUTES_FILE, "route_id,route_type\nR1,3\n");
        write_file(
            &dir,
            TRIPS_FILE,
            "route_id,service_id,trip_id\nR1,SVC1,T1\n",
        );
        write_file(
            &dir,
            STOP_TIMES_FILE,
            "trip_id,stop_id,stop_sequence,arrival_time,departure_time\nT1,STOP1,1,08:00:00,08:00:00\n",
        );

        let input = GtfsInput::from_path(&dir).expect("input");
        let feed = GtfsFeed::from_input(&input).expect("load feed");
        assert_eq!(feed.agency.rows.len(), 1);
        assert_eq!(feed.stops.rows.len(), 1);
        assert_eq!(feed.routes.rows.len(), 1);
        assert_eq!(feed.trips.rows.len(), 1);
        assert_eq!(feed.stop_times.rows.len(), 1);
        assert!(feed.calendar.is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn captures_malformed_geojson_as_notice() {
        let dir = temp_dir("gtfs_geojson");
        fs::create_dir_all(&dir).expect("create dir");

        write_file(
            &dir,
            AGENCY_FILE,
            "agency_name,agency_url,agency_timezone\nTest Agency,https://example.com,UTC\n",
        );
        write_file(&dir, STOPS_FILE, "stop_id\nSTOP1\n");
        write_file(&dir, ROUTES_FILE, "route_id,route_type\nR1,3\n");
        write_file(
            &dir,
            TRIPS_FILE,
            "route_id,service_id,trip_id\nR1,SVC1,T1\n",
        );
        write_file(
            &dir,
            STOP_TIMES_FILE,
            "trip_id,stop_id,stop_sequence,arrival_time,departure_time\nT1,STOP1,1,08:00:00,08:00:00\n",
        );
        write_file(&dir, LOCATIONS_GEOJSON_FILE, "{");

        let input = GtfsInput::from_path(&dir).expect("input");
        let feed = GtfsFeed::from_input(&input).expect("load feed");
        let locations = feed.locations.expect("locations");

        assert_eq!(locations.notices.len(), 1);
        assert_eq!(locations.notices[0].code, "malformed_json");

        fs::remove_dir_all(&dir).ok();
    }
}
