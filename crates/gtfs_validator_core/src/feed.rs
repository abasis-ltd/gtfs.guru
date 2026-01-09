#[cfg(feature = "parallel")]
use chrono::NaiveDate;
use gtfs_guru_model::{
    Agency, Area, Attribution, BookingRules, Calendar, CalendarDate, FareAttribute,
    FareLegJoinRule, FareLegRule, FareMedia, FareProduct, FareRule, FareTransferRule, FeedInfo,
    Frequency, Level, LocationGroup, LocationGroupStop, Network, Pathway, RiderCategory, Route,
    RouteNetwork, Shape, Stop, StopArea, StopTime, Timeframe, Transfer, Translation, Trip,
};
use std::collections::HashMap;

#[cfg(feature = "parallel")]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::geojson::{GeoJsonFeatureCollection, LocationsGeoJson};
use crate::input::GtfsBytesReader;
use crate::progress::ProgressHandler;
use crate::{
    CsvTable, GtfsInput, GtfsInputError, GtfsInputReader, NoticeContainer, NoticeSeverity,
    TableStatus,
};

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
    pub stop_times_by_trip: HashMap<gtfs_guru_model::StringId, Vec<usize>>,
    pub pool: crate::StringPool,
    pub table_statuses: HashMap<&'static str, TableStatus>,
}

impl GtfsFeed {
    /// Rebuild stop_times_by_trip index. Call this after modifying stop_times directly.
    pub fn rebuild_stop_times_index(&mut self) {
        self.stop_times_by_trip = Self::build_stop_times_index(&self.stop_times);
    }

    pub fn table_status(&self, file_name: &str) -> TableStatus {
        self.table_statuses
            .get(file_name)
            .copied()
            .unwrap_or(TableStatus::Ok)
    }

    pub fn table_has_errors(&self, file_name: &str) -> bool {
        !self.table_status(file_name).is_parsed_successfully()
    }

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
        Self::from_input_with_notices_and_progress(input, notices, None)
    }

    pub fn from_input_with_notices_and_progress(
        input: &GtfsInput,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) -> Result<Self, GtfsInputError> {
        let reader = input.reader();
        Self::from_reader_with_notices_and_progress(&reader, notices, progress)
    }

    pub fn from_reader_with_notices(
        reader: &GtfsInputReader,
        notices: &mut NoticeContainer,
    ) -> Result<Self, GtfsInputError> {
        Self::from_reader_with_notices_and_progress(reader, notices, None)
    }

    pub fn from_reader_with_notices_and_progress(
        reader: &GtfsInputReader,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) -> Result<Self, GtfsInputError> {
        #[cfg(feature = "parallel")]
        {
            Self::from_reader_parallel(reader, notices, progress)
        }
        #[cfg(not(feature = "parallel"))]
        {
            Self::from_reader_sequential(reader, notices, progress)
        }
    }

    #[cfg(not(feature = "parallel"))]
    fn from_reader_sequential(
        reader: &GtfsInputReader,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) -> Result<Self, GtfsInputError> {
        let file_sizes = reader.get_files_with_sizes()?;
        let total_bytes = file_sizes.values().sum::<u64>().max(1);
        let mut loaded_bytes = 0u64;

        if let Some(p) = progress {
            p.set_total_files(GTFS_FILE_NAMES.len());
        }

        let pool = crate::StringPool::new();
        let pool_for_intern = pool.clone();
        let pool_for_resolve = pool.clone();

        gtfs_guru_model::set_thread_local_interner(move |s| pool_for_intern.intern(s));
        gtfs_guru_model::set_thread_local_resolver(move |id| pool_for_resolve.resolve(id));

        let mut table_statuses = HashMap::new();

        macro_rules! load_file {
            ($file:expr) => {{
                if let Some(p) = progress {
                    p.on_start_file_load($file);
                }
                let mut local_notices = NoticeContainer::new();
                let res = reader.read_optional_csv_with_notices($file, &mut local_notices);
                record_table_status(&mut table_statuses, $file, &res, &local_notices);
                notices.merge(local_notices);
                if let Some(p) = progress {
                    if let Some(size) = file_sizes.get($file) {
                        loaded_bytes += size;
                    }
                    p.on_progress(
                        loaded_bytes as f32 / total_bytes as f32,
                        &format!("Loaded {}", $file),
                    );
                    p.on_finish_file_load($file);
                }
                res
            }};
        }

        let agency = load_file!(AGENCY_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(AGENCY_FILE);
            CsvTable::default()
        });
        let stops = load_file!(STOPS_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(STOPS_FILE);
            CsvTable::default()
        });
        let routes = load_file!(ROUTES_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(ROUTES_FILE);
            CsvTable::default()
        });
        let trips = load_file!(TRIPS_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(TRIPS_FILE);
            CsvTable::default()
        });
        let stop_times = load_file!(STOP_TIMES_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(STOP_TIMES_FILE);
            CsvTable::default()
        });

        let calendar = load_file!(CALENDAR_FILE)?;
        let calendar_dates = load_file!(CALENDAR_DATES_FILE)?;
        let fare_attributes = load_file!(FARE_ATTRIBUTES_FILE)?;
        let fare_rules = load_file!(FARE_RULES_FILE)?;
        let fare_media = load_file!(FARE_MEDIA_FILE)?;
        let fare_products = load_file!(FARE_PRODUCTS_FILE)?;
        let fare_leg_rules = load_file!(FARE_LEG_RULES_FILE)?;
        let fare_transfer_rules = load_file!(FARE_TRANSFER_RULES_FILE)?;
        let fare_leg_join_rules = load_file!(FARE_LEG_JOIN_RULES_FILE)?;
        let areas = load_file!(AREAS_FILE)?;
        let stop_areas = load_file!(STOP_AREAS_FILE)?;
        let timeframes = load_file!(TIMEFRAMES_FILE)?;
        let rider_categories = load_file!(RIDER_CATEGORIES_FILE)?;
        let shapes = load_file!(SHAPES_FILE)?;
        let frequencies = load_file!(FREQUENCIES_FILE)?;
        let transfers = load_file!(TRANSFERS_FILE)?;
        let location_groups = load_file!(LOCATION_GROUPS_FILE)?;
        let location_group_stops = load_file!(LOCATION_GROUP_STOPS_FILE)?;

        // GeoJSON special case
        if let Some(p) = progress {
            p.on_start_file_load(LOCATIONS_GEOJSON_FILE);
        }
        let locations =
            match reader.read_optional_json::<GeoJsonFeatureCollection>(LOCATIONS_GEOJSON_FILE) {
                Ok(data) => data.map(|c| LocationsGeoJson::new(c, &pool)),
                Err(GtfsInputError::Json { file, source }) if file == LOCATIONS_GEOJSON_FILE => {
                    Some(LocationsGeoJson::malformed_json(source.to_string()))
                }
                Err(err) => return Err(err),
            };
        if let Some(p) = progress {
            p.on_finish_file_load(LOCATIONS_GEOJSON_FILE);
        }
        let locations_status = match &locations {
            Some(locations) => {
                if locations.has_fatal_errors() {
                    TableStatus::ParseError
                } else {
                    TableStatus::Ok
                }
            }
            None => TableStatus::MissingFile,
        };
        table_statuses.insert(LOCATIONS_GEOJSON_FILE, locations_status);

        let booking_rules = load_file!(BOOKING_RULES_FILE)?;
        let networks = load_file!(NETWORKS_FILE)?;
        let route_networks = load_file!(ROUTE_NETWORKS_FILE)?;
        let feed_info = load_file!(FEED_INFO_FILE)?;
        let attributions = load_file!(ATTRIBUTIONS_FILE)?;
        let levels = load_file!(LEVELS_FILE)?;
        let pathways = load_file!(PATHWAYS_FILE)?;
        let translations = load_file!(TRANSLATIONS_FILE)?;

        let stop_times_by_trip = Self::build_stop_times_index(&stop_times);

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
            stop_times_by_trip,
            pool,
            table_statuses,
        })
    }

    #[cfg(feature = "parallel")]
    fn from_reader_parallel(
        reader: &GtfsInputReader,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) -> Result<Self, GtfsInputError> {
        if let Some(p) = progress {
            p.set_total_files(GTFS_FILE_NAMES.len());
        }

        // Capture context from the main thread
        let thorough = crate::validation_context::thorough_mode_enabled();
        let google = crate::validation_context::google_rules_enabled();
        let country = crate::validation_context::validation_country_code();
        let date = crate::validation_context::validation_date();

        struct ParallelLoader<'a> {
            reader: &'a GtfsInputReader,
            progress: Option<&'a dyn ProgressHandler>,
            thorough: bool,
            google: bool,
            country: Option<String>,
            date: NaiveDate,
            pool: crate::StringPool,
            file_sizes: &'a HashMap<String, u64>,
            total_bytes: u64,
            loaded_bytes: &'a AtomicU64,
        }

        impl<'a> ParallelLoader<'a> {
            fn load<T: serde::de::DeserializeOwned + Send>(
                &self,
                filename: &str,
            ) -> (
                Result<Option<CsvTable<T>>, GtfsInputError>,
                NoticeContainer,
                TableStatus,
            ) {
                // Restore context in the worker thread
                let _g1 = crate::validation_context::set_thorough_mode_enabled(self.thorough);
                let _g2 = crate::validation_context::set_google_rules_enabled(self.google);
                let _g3 =
                    crate::validation_context::set_validation_country_code(self.country.clone());
                let _g4 = crate::validation_context::set_validation_date(Some(self.date));

                let pool_for_intern = self.pool.clone();
                let pool_for_resolve = self.pool.clone();
                gtfs_guru_model::set_thread_local_interner(move |s| pool_for_intern.intern(s));
                gtfs_guru_model::set_thread_local_resolver(move |id| pool_for_resolve.resolve(id));

                if let Some(p) = self.progress {
                    p.on_start_file_load(filename);
                }

                let start = std::time::Instant::now();
                let mut local_notices = NoticeContainer::new();
                let result = self
                    .reader
                    .read_optional_csv_with_notices(filename, &mut local_notices);

                // Output per-file timing if GTFS_PERF_DEBUG is set
                if std::env::var("GTFS_PERF_DEBUG").is_ok() {
                    let elapsed = start.elapsed();
                    let row_count = result
                        .as_ref()
                        .ok()
                        .and_then(|r| r.as_ref())
                        .map(|t| t.rows.len())
                        .unwrap_or(0);
                    eprintln!(
                        "[PERF] {} loaded: {:?} ({} rows)",
                        filename, elapsed, row_count
                    );
                }

                gtfs_guru_model::clear_thread_local_hooks();

                if let Some(p) = self.progress {
                    if let Some(size) = self.file_sizes.get(filename) {
                        let prev = self.loaded_bytes.fetch_add(*size, Ordering::Relaxed);
                        let current = prev + *size;
                        p.on_progress(
                            current as f32 / self.total_bytes as f32,
                            &format!("Loaded {}", filename),
                        );
                    }
                    p.on_finish_file_load(filename);
                }

                let status = status_from_load_result(&result, &local_notices);
                (result, local_notices, status)
            }
        }

        let file_sizes = reader.get_files_with_sizes()?;
        let total_bytes = file_sizes.values().sum::<u64>().max(1);
        let loaded_bytes = AtomicU64::new(0);

        let loader = ParallelLoader {
            reader,
            progress,
            thorough,
            google,
            country,
            date,
            pool: crate::StringPool::new(),
            file_sizes: &file_sizes,
            total_bytes,
            loaded_bytes: &loaded_bytes,
        };

        // Parallelize loading:
        // Group 1: Stop Times (Index built in parallel)
        // Group 2: Shapes
        // Group 3: Trips, Calendar Dates
        // Group 4: Stops, Routes
        // Group 5: All others (sequential)

        let (
            (stop_times_result, index, n1, s1),
            (
                (shapes_res, n2, s2),
                (
                    ((trips_res, n3, s3), (calendar_dates_res, n4, s4)),
                    (((stops_res, n5, s5), (routes_res, n6, s6)), (others, n7)),
                ),
            ),
        ) = rayon::join(
            || {
                let (res, notices, status) = loader.load(STOP_TIMES_FILE);
                let (res, index) = match res {
                    Ok(Some(table)) => {
                        let idx = Self::build_stop_times_index(&table);
                        (Ok(Some(table)), idx)
                    }
                    r => (r, HashMap::new()),
                };
                (res, index, notices, status)
            },
            || {
                rayon::join(
                    || loader.load(SHAPES_FILE),
                    || {
                        rayon::join(
                            || {
                                rayon::join(
                                    || loader.load(TRIPS_FILE),
                                    || loader.load(CALENDAR_DATES_FILE),
                                )
                            },
                            || {
                                rayon::join(
                                    || {
                                        rayon::join(
                                            || loader.load(STOPS_FILE),
                                            || loader.load(ROUTES_FILE),
                                        )
                                    },
                                    || {
                                        let (
                                            (
                                                (
                                                    (agency, n_a, s_a),
                                                    (calendar, n_b, s_b),
                                                    (fare_attributes, n_c, s_c),
                                                    (fare_rules, n_d, s_d),
                                                    (fare_media, n_e, s_e),
                                                    (fare_products, n_f, s_f),
                                                    (fare_leg_rules, n_g, s_g),
                                                ),
                                                (
                                                    (fare_transfer_rules, n_h, s_h),
                                                    (fare_leg_join_rules, n_i, s_i),
                                                    (areas, n_j, s_j),
                                                    (stop_areas, n_k, s_k),
                                                    (timeframes, n_l, s_l),
                                                    (rider_categories, n_m, s_m),
                                                    (frequencies, n_n, s_n),
                                                ),
                                            ),
                                            (
                                                (
                                                    (transfers, n_o, s_o),
                                                    (location_groups, n_p, s_p),
                                                    (location_group_stops, n_q, s_q),
                                                    (locations, n_r, s_r),
                                                    (booking_rules, n_s, s_s),
                                                    (networks, n_t, s_t),
                                                ),
                                                (
                                                    (route_networks, n_u, s_u),
                                                    (feed_info, n_v, s_v),
                                                    (attributions, n_w, s_w),
                                                    (levels, n_x, s_x),
                                                    (pathways, n_y, s_y),
                                                    (translations, n_z, s_z),
                                                ),
                                            ),
                                        ) = rayon::join(
                                            || {
                                                rayon::join(
                                                    || {
                                                        // Batch 1 (7 files)
                                                        let (agency, n1, s1) =
                                                            loader.load(AGENCY_FILE);
                                                        let (calendar, n2, s2) =
                                                            loader.load(CALENDAR_FILE);
                                                        let (fare_attributes, n3, s3) =
                                                            loader.load(FARE_ATTRIBUTES_FILE);
                                                        let (fare_rules, n4, s4) =
                                                            loader.load(FARE_RULES_FILE);
                                                        let (fare_media, n5, s5) =
                                                            loader.load(FARE_MEDIA_FILE);
                                                        let (fare_products, n6, s6) =
                                                            loader.load(FARE_PRODUCTS_FILE);
                                                        let (fare_leg_rules, n7, s7) =
                                                            loader.load(FARE_LEG_RULES_FILE);
                                                        (
                                                            (agency, n1, s1),
                                                            (calendar, n2, s2),
                                                            (fare_attributes, n3, s3),
                                                            (fare_rules, n4, s4),
                                                            (fare_media, n5, s5),
                                                            (fare_products, n6, s6),
                                                            (fare_leg_rules, n7, s7),
                                                        )
                                                    },
                                                    || {
                                                        // Batch 2 (7 files)
                                                        let (fare_transfer_rules, n1, s1) =
                                                            loader.load(FARE_TRANSFER_RULES_FILE);
                                                        let (fare_leg_join_rules, n2, s2) =
                                                            loader.load(FARE_LEG_JOIN_RULES_FILE);
                                                        let (areas, n3, s3) =
                                                            loader.load(AREAS_FILE);
                                                        let (stop_areas, n4, s4) =
                                                            loader.load(STOP_AREAS_FILE);
                                                        let (timeframes, n5, s5) =
                                                            loader.load(TIMEFRAMES_FILE);
                                                        let (rider_categories, n6, s6) =
                                                            loader.load(RIDER_CATEGORIES_FILE);
                                                        let (frequencies, n7, s7) =
                                                            loader.load(FREQUENCIES_FILE);
                                                        (
                                                            (fare_transfer_rules, n1, s1),
                                                            (fare_leg_join_rules, n2, s2),
                                                            (areas, n3, s3),
                                                            (stop_areas, n4, s4),
                                                            (timeframes, n5, s5),
                                                            (rider_categories, n6, s6),
                                                            (frequencies, n7, s7),
                                                        )
                                                    },
                                                )
                                            },
                                            || {
                                                rayon::join(
                                                    || {
                                                        // Batch 3 (6 files)
                                                        let (transfers, n1, s1) =
                                                            loader.load(TRANSFERS_FILE);
                                                        let (location_groups, n2, s2) =
                                                            loader.load(LOCATION_GROUPS_FILE);
                                                        let (location_group_stops, n3, s3) =
                                                            loader.load(LOCATION_GROUP_STOPS_FILE);
                                                        if let Some(p) = loader.progress {
                                                            p.on_start_file_load(
                                                                LOCATIONS_GEOJSON_FILE,
                                                            );
                                                        }
                                                        let (locations, n4) = match reader
                                                            .read_optional_json::<GeoJsonFeatureCollection>(
                                                                LOCATIONS_GEOJSON_FILE,
                                                            ) {
                                                            Ok(data) => (Ok(data.map(|d| LocationsGeoJson::new(d, &loader.pool))), NoticeContainer::new()),
                                                            Err(GtfsInputError::Json { file, source })
                                                                if file == LOCATIONS_GEOJSON_FILE =>
                                                            {
                                                                let n = NoticeContainer::new();
                                                                (Ok(Some(LocationsGeoJson::malformed_json(
                                                                    source.to_string(),
                                                                ))), n)
                                                            }
                                                            Err(err) => (Err(err), NoticeContainer::new()),
                                                        };
                                                        if let Some(p) = loader.progress {
                                                            p.on_finish_file_load(
                                                                LOCATIONS_GEOJSON_FILE,
                                                            );
                                                        }

                                                        let locations_status = match &locations {
                                                            Ok(Some(locations)) => {
                                                                if locations.has_fatal_errors() {
                                                                    TableStatus::ParseError
                                                                } else {
                                                                    TableStatus::Ok
                                                                }
                                                            }
                                                            Ok(None) => TableStatus::MissingFile,
                                                            Err(_) => TableStatus::ParseError,
                                                        };

                                                        let (booking_rules, n5, s5) =
                                                            loader.load(BOOKING_RULES_FILE);
                                                        let (networks, n6, s6) =
                                                            loader.load(NETWORKS_FILE);

                                                        (
                                                            (transfers, n1, s1),
                                                            (location_groups, n2, s2),
                                                            (location_group_stops, n3, s3),
                                                            (locations, n4, locations_status),
                                                            (booking_rules, n5, s5),
                                                            (networks, n6, s6),
                                                        )
                                                    },
                                                    || {
                                                        // Batch 4 (6 files)
                                                        let (route_networks, n1, s1) =
                                                            loader.load(ROUTE_NETWORKS_FILE);
                                                        let (feed_info, n2, s2) =
                                                            loader.load(FEED_INFO_FILE);
                                                        let (attributions, n3, s3) =
                                                            loader.load(ATTRIBUTIONS_FILE);
                                                        let (levels, n4, s4) =
                                                            loader.load(LEVELS_FILE);
                                                        let (pathways, n5, s5) =
                                                            loader.load(PATHWAYS_FILE);
                                                        let (translations, n6, s6) =
                                                            loader.load(TRANSLATIONS_FILE);

                                                        (
                                                            (route_networks, n1, s1),
                                                            (feed_info, n2, s2),
                                                            (attributions, n3, s3),
                                                            (levels, n4, s4),
                                                            (pathways, n5, s5),
                                                            (translations, n6, s6),
                                                        )
                                                    },
                                                )
                                            },
                                        );

                                        let mut n = NoticeContainer::new();
                                        // Merge all notices
                                        for container in [
                                            n_a, n_b, n_c, n_d, n_e, n_f, n_g, n_h, n_i, n_j, n_k,
                                            n_l, n_m, n_n, n_o, n_p, n_q, n_r, n_s, n_t, n_u, n_v,
                                            n_w, n_x, n_y, n_z,
                                        ] {
                                            n.merge(container);
                                        }

                                        (
                                            (
                                                (
                                                    agency,
                                                    calendar,
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
                                                ),
                                                (
                                                    s_a, s_b, s_c, s_d, s_e, s_f, s_g, s_h, s_i,
                                                    s_j, s_k, s_l, s_m, s_n, s_o, s_p, s_q, s_r,
                                                    s_s, s_t, s_u, s_v, s_w, s_x, s_y, s_z,
                                                ),
                                            ),
                                            n,
                                        )
                                    },
                                )
                            },
                        )
                    },
                )
            },
        );

        // Merge all notices
        notices.merge(n1);
        notices.merge(n2);
        notices.merge(n3);
        notices.merge(n4);
        notices.merge(n5);
        notices.merge(n6);
        notices.merge(n7);

        // Unpack "others"
        let (
            (
                agency,
                calendar,
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
            ),
            (
                s_a,
                s_b,
                s_c,
                s_d,
                s_e,
                s_f,
                s_g,
                s_h,
                s_i,
                s_j,
                s_k,
                s_l,
                s_m,
                s_n,
                s_o,
                s_p,
                s_q,
                s_r,
                s_s,
                s_t,
                s_u,
                s_v,
                s_w,
                s_x,
                s_y,
                s_z,
            ),
        ) = others;

        let mut table_statuses = HashMap::new();
        table_statuses.insert(STOP_TIMES_FILE, s1);
        table_statuses.insert(SHAPES_FILE, s2);
        table_statuses.insert(TRIPS_FILE, s3);
        table_statuses.insert(CALENDAR_DATES_FILE, s4);
        table_statuses.insert(STOPS_FILE, s5);
        table_statuses.insert(ROUTES_FILE, s6);
        table_statuses.insert(AGENCY_FILE, s_a);
        table_statuses.insert(CALENDAR_FILE, s_b);
        table_statuses.insert(FARE_ATTRIBUTES_FILE, s_c);
        table_statuses.insert(FARE_RULES_FILE, s_d);
        table_statuses.insert(FARE_MEDIA_FILE, s_e);
        table_statuses.insert(FARE_PRODUCTS_FILE, s_f);
        table_statuses.insert(FARE_LEG_RULES_FILE, s_g);
        table_statuses.insert(FARE_TRANSFER_RULES_FILE, s_h);
        table_statuses.insert(FARE_LEG_JOIN_RULES_FILE, s_i);
        table_statuses.insert(AREAS_FILE, s_j);
        table_statuses.insert(STOP_AREAS_FILE, s_k);
        table_statuses.insert(TIMEFRAMES_FILE, s_l);
        table_statuses.insert(RIDER_CATEGORIES_FILE, s_m);
        table_statuses.insert(FREQUENCIES_FILE, s_n);
        table_statuses.insert(TRANSFERS_FILE, s_o);
        table_statuses.insert(LOCATION_GROUPS_FILE, s_p);
        table_statuses.insert(LOCATION_GROUP_STOPS_FILE, s_q);
        table_statuses.insert(LOCATIONS_GEOJSON_FILE, s_r);
        table_statuses.insert(BOOKING_RULES_FILE, s_s);
        table_statuses.insert(NETWORKS_FILE, s_t);
        table_statuses.insert(ROUTE_NETWORKS_FILE, s_u);
        table_statuses.insert(FEED_INFO_FILE, s_v);
        table_statuses.insert(ATTRIBUTIONS_FILE, s_w);
        table_statuses.insert(LEVELS_FILE, s_x);
        table_statuses.insert(PATHWAYS_FILE, s_y);
        table_statuses.insert(TRANSLATIONS_FILE, s_z);

        // Helper to unwrap optional tables or return default if missing + add notice
        fn unwrap_required<T: Default>(
            res: Result<Option<T>, GtfsInputError>,
            filename: &str,
            notices: &mut NoticeContainer,
        ) -> Result<T, GtfsInputError> {
            match res {
                Ok(Some(v)) => Ok(v),
                Ok(None) => {
                    notices.push_missing_file(filename);
                    Ok(T::default())
                }
                Err(e) => Err(e),
            }
        }

        // Required files
        let agency = unwrap_required(agency, AGENCY_FILE, notices)?;
        let stops = unwrap_required(stops_res, STOPS_FILE, notices)?;
        let routes = unwrap_required(routes_res, ROUTES_FILE, notices)?;
        let trips = unwrap_required(trips_res, TRIPS_FILE, notices)?;
        let stop_times = unwrap_required(stop_times_result, STOP_TIMES_FILE, notices)?;

        // Optional files (propagate errors)
        let calendar = calendar?;
        let calendar_dates = calendar_dates_res?;
        let fare_attributes = fare_attributes?;
        let fare_rules = fare_rules?;
        let fare_media = fare_media?;
        let fare_products = fare_products?;
        let fare_leg_rules = fare_leg_rules?;
        let fare_transfer_rules = fare_transfer_rules?;
        let fare_leg_join_rules = fare_leg_join_rules?;
        let areas = areas?;
        let stop_areas = stop_areas?;
        let timeframes = timeframes?;
        let rider_categories = rider_categories?;
        let shapes = shapes_res?;
        let frequencies = frequencies?;
        let transfers = transfers?;
        let location_groups = location_groups?;
        let location_group_stops = location_group_stops?;
        let locations = locations?;
        let booking_rules = booking_rules?;
        let networks = networks?;
        let route_networks = route_networks?;
        let feed_info = feed_info?;
        let attributions = attributions?;
        let levels = levels?;
        let pathways = pathways?;
        let translations = translations?;
        let pool = loader.pool;

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
            stop_times_by_trip: index,
            pool,
            table_statuses,
        })
    }

    fn build_stop_times_index(
        stop_times: &CsvTable<StopTime>,
    ) -> HashMap<gtfs_guru_model::StringId, Vec<usize>> {
        let mut index: HashMap<gtfs_guru_model::StringId, Vec<usize>> = HashMap::new();
        for (i, st) in stop_times.rows.iter().enumerate() {
            let trip_id = st.trip_id;
            if trip_id.0 != 0 {
                index.entry(trip_id).or_default().push(i);
            }
        }
        // Sort each trip's stop_times by stop_sequence
        for indices in index.values_mut() {
            indices.sort_by_key(|&i| stop_times.rows[i].stop_sequence);
        }
        index
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
        Self::from_bytes_reader_with_notices_and_progress(reader, notices, None)
    }

    /// Load GTFS feed from in-memory bytes with notice collection and progress reporting
    pub fn from_bytes_reader_with_notices_and_progress(
        reader: &GtfsBytesReader,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) -> Result<Self, GtfsInputError> {
        if let Some(p) = progress {
            p.set_total_files(GTFS_FILE_NAMES.len());
        }

        let pool = crate::StringPool::new();
        let pool_for_intern = pool.clone();
        let pool_for_resolve = pool.clone();

        gtfs_guru_model::set_thread_local_interner(move |s| pool_for_intern.intern(s));
        gtfs_guru_model::set_thread_local_resolver(move |id| pool_for_resolve.resolve(id));

        let mut table_statuses = HashMap::new();

        macro_rules! load_file {
            ($file:expr) => {{
                if let Some(p) = progress {
                    p.on_start_file_load($file);
                }
                let mut local_notices = NoticeContainer::new();
                let res = reader.read_optional_csv_with_notices($file, &mut local_notices);
                record_table_status(&mut table_statuses, $file, &res, &local_notices);
                notices.merge(local_notices);
                if let Some(p) = progress {
                    p.on_finish_file_load($file);
                }
                res
            }};
        }

        let agency = load_file!(AGENCY_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(AGENCY_FILE);
            CsvTable::default()
        });
        let stops = load_file!(STOPS_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(STOPS_FILE);
            CsvTable::default()
        });
        let routes = load_file!(ROUTES_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(ROUTES_FILE);
            CsvTable::default()
        });
        let trips = load_file!(TRIPS_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(TRIPS_FILE);
            CsvTable::default()
        });
        let stop_times = load_file!(STOP_TIMES_FILE)?.unwrap_or_else(|| {
            notices.push_missing_file(STOP_TIMES_FILE);
            CsvTable::default()
        });

        let calendar = load_file!(CALENDAR_FILE)?;
        let calendar_dates = load_file!(CALENDAR_DATES_FILE)?;
        let fare_attributes = load_file!(FARE_ATTRIBUTES_FILE)?;
        let fare_rules = load_file!(FARE_RULES_FILE)?;
        let fare_media = load_file!(FARE_MEDIA_FILE)?;
        let fare_products = load_file!(FARE_PRODUCTS_FILE)?;
        let fare_leg_rules = load_file!(FARE_LEG_RULES_FILE)?;
        let fare_transfer_rules = load_file!(FARE_TRANSFER_RULES_FILE)?;
        let fare_leg_join_rules = load_file!(FARE_LEG_JOIN_RULES_FILE)?;
        let areas = load_file!(AREAS_FILE)?;
        let stop_areas = load_file!(STOP_AREAS_FILE)?;
        let timeframes = load_file!(TIMEFRAMES_FILE)?;
        let rider_categories = load_file!(RIDER_CATEGORIES_FILE)?;
        let shapes = load_file!(SHAPES_FILE)?;
        let frequencies = load_file!(FREQUENCIES_FILE)?;
        let transfers = load_file!(TRANSFERS_FILE)?;
        let location_groups = load_file!(LOCATION_GROUPS_FILE)?;
        let location_group_stops = load_file!(LOCATION_GROUP_STOPS_FILE)?;
        let locations =
            match reader.read_optional_json::<GeoJsonFeatureCollection>(LOCATIONS_GEOJSON_FILE) {
                Ok(data) => data.map(|c| LocationsGeoJson::new(c, &pool)),
                Err(GtfsInputError::Json { file, source }) if file == LOCATIONS_GEOJSON_FILE => {
                    Some(LocationsGeoJson::malformed_json(source.to_string()))
                }
                Err(err) => return Err(err),
            };
        let locations_status = match &locations {
            Some(locations) => {
                if locations.has_fatal_errors() {
                    TableStatus::ParseError
                } else {
                    TableStatus::Ok
                }
            }
            None => TableStatus::MissingFile,
        };
        table_statuses.insert(LOCATIONS_GEOJSON_FILE, locations_status);
        let booking_rules = load_file!(BOOKING_RULES_FILE)?;
        let networks = load_file!(NETWORKS_FILE)?;
        let route_networks = load_file!(ROUTE_NETWORKS_FILE)?;
        let feed_info = load_file!(FEED_INFO_FILE)?;
        let attributions = load_file!(ATTRIBUTIONS_FILE)?;
        let levels = load_file!(LEVELS_FILE)?;
        let pathways = load_file!(PATHWAYS_FILE)?;
        let translations = load_file!(TRANSLATIONS_FILE)?;

        let stop_times_by_trip = Self::build_stop_times_index(&stop_times);

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
            stop_times_by_trip,
            pool,
            table_statuses,
        })
    }
}

fn status_from_load_result<T>(
    result: &Result<Option<T>, GtfsInputError>,
    notices: &NoticeContainer,
) -> TableStatus {
    match result {
        Ok(Some(_)) => {
            if notices
                .iter()
                .any(|notice| notice.severity == NoticeSeverity::Error)
            {
                TableStatus::ParseError
            } else {
                TableStatus::Ok
            }
        }
        Ok(None) => TableStatus::MissingFile,
        Err(_) => TableStatus::ParseError,
    }
}

fn record_table_status<T>(
    table_statuses: &mut HashMap<&'static str, TableStatus>,
    file_name: &'static str,
    result: &Result<Option<T>, GtfsInputError>,
    notices: &NoticeContainer,
) {
    table_statuses.insert(file_name, status_from_load_result(result, notices));
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

    #[test]
    fn triggers_progress_handler_callbacks() {
        use crate::progress::ProgressHandler;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        #[derive(Default)]
        struct MockHandler {
            started: AtomicUsize,
            finished: AtomicUsize,
            total_files: AtomicUsize,
        }

        impl ProgressHandler for MockHandler {
            fn on_start_file_load(&self, _file: &str) {
                self.started.fetch_add(1, Ordering::SeqCst);
            }
            fn on_finish_file_load(&self, _file: &str) {
                self.finished.fetch_add(1, Ordering::SeqCst);
            }
            fn on_start_validation(&self, _validator_name: &str) {}
            fn on_finish_validation(&self, _validator_name: &str) {}
            fn set_total_validators(&self, _count: usize) {}
            fn increment_validator_progress(&self) {}
            fn set_total_files(&self, count: usize) {
                self.total_files.store(count, Ordering::SeqCst);
            }
        }

        let dir = temp_dir("gtfs_progress");
        fs::create_dir_all(&dir).expect("create dir");
        write_file(
            &dir,
            AGENCY_FILE,
            "agency_name,agency_url,agency_timezone\nTest,https://example.com,UTC\n",
        );
        write_file(&dir, STOPS_FILE, "stop_id\nSTOP1\n");
        write_file(&dir, ROUTES_FILE, "route_id,route_type\nR1,3\n");
        write_file(
            &dir,
            TRIPS_FILE,
            "route_id,service_id,trip_id\nR1,SVC1,T1\n",
        );
        write_file(&dir, STOP_TIMES_FILE, "trip_id,stop_id,stop_sequence,arrival_time,departure_time\nT1,STOP1,1,08:00:00,08:00:00\n");

        let input = GtfsInput::from_path(&dir).expect("input");
        let handler = Arc::new(MockHandler::default());
        let mut notices = NoticeContainer::new();

        GtfsFeed::from_input_with_notices_and_progress(
            &input,
            &mut notices,
            Some(handler.as_ref()),
        )
        .expect("load");

        // We load many files (defined in GTFS_FILE_NAMES), some exist some don't.
        // Each attempt to load a file should trigger start/finish.
        assert!(handler.total_files.load(Ordering::SeqCst) > 0);
        assert!(handler.started.load(Ordering::SeqCst) >= 5);
        assert_eq!(
            handler.started.load(Ordering::SeqCst),
            handler.finished.load(Ordering::SeqCst)
        );

        fs::remove_dir_all(&dir).ok();
    }
}
