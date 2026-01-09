use crate::feed::{
    AGENCY_FILE, AREAS_FILE, ATTRIBUTIONS_FILE, BOOKING_RULES_FILE, CALENDAR_DATES_FILE,
    CALENDAR_FILE, FARE_ATTRIBUTES_FILE, FARE_LEG_JOIN_RULES_FILE, FARE_LEG_RULES_FILE,
    FARE_MEDIA_FILE, FARE_PRODUCTS_FILE, FARE_RULES_FILE, FARE_TRANSFER_RULES_FILE, FEED_INFO_FILE,
    FREQUENCIES_FILE, LEVELS_FILE, LOCATION_GROUPS_FILE, LOCATION_GROUP_STOPS_FILE, NETWORKS_FILE,
    PATHWAYS_FILE, RIDER_CATEGORIES_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE, SHAPES_FILE,
    STOPS_FILE, STOP_AREAS_FILE, STOP_TIMES_FILE, TIMEFRAMES_FILE, TRANSFERS_FILE,
    TRANSLATIONS_FILE, TRIPS_FILE,
};

pub struct CsvSchema {
    pub fields: &'static [&'static str],
    pub required_fields: &'static [&'static str],
    pub recommended_fields: &'static [&'static str],
}

const NO_RECOMMENDED_FIELDS: &[&str] = &[];

const AGENCY_RECOMMENDED_FIELDS: &[&str] = &[
    "agency_lang",
    "agency_phone",
    "agency_fare_url",
    "agency_email",
];
const AGENCY_FIELDS: &[&str] = &[
    "agency_id",
    "agency_name",
    "agency_url",
    "agency_timezone",
    "agency_lang",
    "agency_phone",
    "agency_fare_url",
    "agency_email",
];
const AGENCY_REQUIRED_FIELDS: &[&str] = &["agency_name", "agency_url", "agency_timezone"];
const STOPS_FIELDS: &[&str] = &[
    "stop_id",
    "stop_code",
    "stop_name",
    "tts_stop_name",
    "stop_desc",
    "stop_lat",
    "stop_lon",
    "zone_id",
    "stop_url",
    "location_type",
    "parent_station",
    "stop_timezone",
    "wheelchair_boarding",
    "level_id",
    "platform_code",
    "stop_address",
    "stop_city",
    "stop_region",
    "stop_postcode",
    "stop_country",
    "stop_phone",
];
const STOPS_REQUIRED_FIELDS: &[&str] = &["stop_id"];
const STOPS_RECOMMENDED_FIELDS: &[&str] = &[
    "stop_name",
    "stop_lat",
    "stop_lon",
    "stop_desc",
    "stop_url",
    "wheelchair_boarding",
    "level_id",
    "platform_code",
];
const ROUTES_FIELDS: &[&str] = &[
    "route_id",
    "agency_id",
    "route_short_name",
    "route_long_name",
    "route_desc",
    "route_type",
    "route_url",
    "route_color",
    "route_text_color",
    "route_sort_order",
    "continuous_pickup",
    "continuous_drop_off",
    "network_id",
    "route_branding_url",
];
const ROUTES_REQUIRED_FIELDS: &[&str] = &["route_id", "route_type"];
const ROUTES_RECOMMENDED_FIELDS: &[&str] = &[
    "route_short_name",
    "route_long_name",
    "route_desc",
    "route_url",
    "route_color",
    "route_text_color",
];
const TRIPS_FIELDS: &[&str] = &[
    "route_id",
    "service_id",
    "trip_id",
    "trip_headsign",
    "trip_short_name",
    "direction_id",
    "block_id",
    "shape_id",
    "wheelchair_accessible",
    "bikes_allowed",
    "continuous_pickup",
    "continuous_drop_off",
];
const TRIPS_REQUIRED_FIELDS: &[&str] = &["route_id", "service_id", "trip_id"];
const TRIPS_RECOMMENDED_FIELDS: &[&str] = &[
    "trip_headsign",
    "trip_short_name",
    "direction_id",
    "block_id",
    "shape_id",
    "wheelchair_accessible",
    "bikes_allowed",
];
const STOP_TIMES_FIELDS: &[&str] = &[
    "trip_id",
    "arrival_time",
    "departure_time",
    "stop_id",
    "location_group_id",
    "location_id",
    "stop_sequence",
    "stop_headsign",
    "pickup_type",
    "drop_off_type",
    "pickup_booking_rule_id",
    "drop_off_booking_rule_id",
    "continuous_pickup",
    "continuous_drop_off",
    "shape_dist_traveled",
    "timepoint",
    "start_pickup_drop_off_window",
    "end_pickup_drop_off_window",
];
const STOP_TIMES_REQUIRED_FIELDS: &[&str] = &["trip_id", "stop_sequence"];
const STOP_TIMES_RECOMMENDED_FIELDS: &[&str] = &[
    "arrival_time",
    "departure_time",
    "stop_headsign",
    "pickup_type",
    "drop_off_type",
    "shape_dist_traveled",
    "timepoint",
];
const CALENDAR_FIELDS: &[&str] = &[
    "service_id",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "start_date",
    "end_date",
];
const CALENDAR_REQUIRED_FIELDS: &[&str] = &[
    "service_id",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "start_date",
    "end_date",
];
const CALENDAR_DATES_FIELDS: &[&str] = &["service_id", "date", "exception_type"];
const CALENDAR_DATES_REQUIRED_FIELDS: &[&str] = &["service_id", "date", "exception_type"];
const FARE_ATTRIBUTES_FIELDS: &[&str] = &[
    "fare_id",
    "price",
    "currency_type",
    "payment_method",
    "transfers",
    "agency_id",
    "transfer_duration",
];
const FARE_ATTRIBUTES_REQUIRED_FIELDS: &[&str] =
    &["fare_id", "price", "currency_type", "payment_method"];
const FARE_RULES_FIELDS: &[&str] = &[
    "fare_id",
    "route_id",
    "origin_id",
    "destination_id",
    "contains_id",
];
const FARE_RULES_REQUIRED_FIELDS: &[&str] = &["fare_id"];
const SHAPES_FIELDS: &[&str] = &[
    "shape_id",
    "shape_pt_lat",
    "shape_pt_lon",
    "shape_pt_sequence",
    "shape_dist_traveled",
];
const SHAPES_REQUIRED_FIELDS: &[&str] = &[
    "shape_id",
    "shape_pt_lat",
    "shape_pt_lon",
    "shape_pt_sequence",
];
const FREQUENCIES_FIELDS: &[&str] = &[
    "trip_id",
    "start_time",
    "end_time",
    "headway_secs",
    "exact_times",
];
const FREQUENCIES_REQUIRED_FIELDS: &[&str] = &["trip_id", "start_time", "end_time", "headway_secs"];
const TRANSFERS_FIELDS: &[&str] = &[
    "from_stop_id",
    "to_stop_id",
    "transfer_type",
    "min_transfer_time",
    "from_route_id",
    "to_route_id",
    "from_trip_id",
    "to_trip_id",
];
const TRANSFERS_REQUIRED_FIELDS: &[&str] = &[];
const PATHWAYS_FIELDS: &[&str] = &[
    "pathway_id",
    "from_stop_id",
    "to_stop_id",
    "pathway_mode",
    "is_bidirectional",
    "length",
    "traversal_time",
    "stair_count",
    "max_slope",
    "min_width",
    "signposted_as",
    "reversed_signposted_as",
];
const PATHWAYS_REQUIRED_FIELDS: &[&str] = &[
    "pathway_id",
    "from_stop_id",
    "to_stop_id",
    "pathway_mode",
    "is_bidirectional",
];
const FEED_INFO_FIELDS: &[&str] = &[
    "feed_publisher_name",
    "feed_publisher_url",
    "feed_lang",
    "feed_start_date",
    "feed_end_date",
    "feed_version",
    "feed_contact_email",
    "feed_contact_url",
    "default_lang",
];
const FEED_INFO_RECOMMENDED_FIELDS: &[&str] = &["feed_start_date", "feed_end_date", "feed_version"];
const FEED_INFO_REQUIRED_FIELDS: &[&str] =
    &["feed_publisher_name", "feed_publisher_url", "feed_lang"];
const ATTRIBUTIONS_FIELDS: &[&str] = &[
    "attribution_id",
    "agency_id",
    "route_id",
    "trip_id",
    "organization_name",
    "is_producer",
    "is_operator",
    "is_authority",
    "attribution_url",
    "attribution_email",
    "attribution_phone",
];
const ATTRIBUTIONS_REQUIRED_FIELDS: &[&str] = &["organization_name"];
const LEVELS_FIELDS: &[&str] = &["level_id", "level_index", "level_name"];
const LEVELS_REQUIRED_FIELDS: &[&str] = &["level_id", "level_index"];
const TRANSLATIONS_FIELDS: &[&str] = &[
    "table_name",
    "field_name",
    "language",
    "translation",
    "record_id",
    "record_sub_id",
    "field_value",
];
const TRANSLATIONS_REQUIRED_FIELDS: &[&str] =
    &["table_name", "field_name", "language", "translation"];
const FARE_MEDIA_FIELDS: &[&str] = &["fare_media_id", "fare_media_name", "fare_media_type"];
const FARE_MEDIA_REQUIRED_FIELDS: &[&str] = &["fare_media_id", "fare_media_type"];
const FARE_PRODUCTS_FIELDS: &[&str] = &[
    "fare_product_id",
    "fare_product_name",
    "amount",
    "currency",
    "fare_media_id",
    "rider_category_id",
];
const FARE_PRODUCTS_REQUIRED_FIELDS: &[&str] = &["fare_product_id", "amount", "currency"];
const FARE_LEG_RULES_FIELDS: &[&str] = &[
    "leg_group_id",
    "network_id",
    "from_area_id",
    "to_area_id",
    "from_timeframe_group_id",
    "to_timeframe_group_id",
    "fare_product_id",
    "rule_priority",
];
const FARE_LEG_RULES_REQUIRED_FIELDS: &[&str] = &["fare_product_id"];
const FARE_TRANSFER_RULES_FIELDS: &[&str] = &[
    "from_leg_group_id",
    "to_leg_group_id",
    "duration_limit",
    "duration_limit_type",
    "fare_transfer_type",
    "transfer_count",
    "fare_product_id",
];
const FARE_TRANSFER_RULES_REQUIRED_FIELDS: &[&str] = &["fare_transfer_type"];
const FARE_LEG_JOIN_RULES_FIELDS: &[&str] = &[
    "from_network_id",
    "to_network_id",
    "from_stop_id",
    "to_stop_id",
    "from_area_id",
    "to_area_id",
];
const FARE_LEG_JOIN_RULES_REQUIRED_FIELDS: &[&str] = &["from_network_id", "to_network_id"];
const AREAS_FIELDS: &[&str] = &["area_id", "area_name"];
const AREAS_REQUIRED_FIELDS: &[&str] = &["area_id"];
const STOP_AREAS_FIELDS: &[&str] = &["area_id", "stop_id"];
const STOP_AREAS_REQUIRED_FIELDS: &[&str] = &["area_id", "stop_id"];
const TIMEFRAMES_FIELDS: &[&str] = &["timeframe_group_id", "start_time", "end_time", "service_id"];
const TIMEFRAMES_REQUIRED_FIELDS: &[&str] = &["service_id"];
const RIDER_CATEGORIES_FIELDS: &[&str] = &[
    "rider_category_id",
    "rider_category_name",
    "is_default_fare_category",
    "eligibility_url",
];
const RIDER_CATEGORIES_REQUIRED_FIELDS: &[&str] = &["rider_category_id", "rider_category_name"];
const LOCATION_GROUPS_FIELDS: &[&str] = &[
    "location_group_id",
    "location_group_name",
    "location_group_desc",
];
const LOCATION_GROUPS_REQUIRED_FIELDS: &[&str] = &["location_group_id"];
const LOCATION_GROUP_STOPS_FIELDS: &[&str] = &["location_group_id", "stop_id"];
const LOCATION_GROUP_STOPS_REQUIRED_FIELDS: &[&str] = &["location_group_id", "stop_id"];
const NETWORKS_FIELDS: &[&str] = &["network_id", "network_name"];
const NETWORKS_REQUIRED_FIELDS: &[&str] = &["network_id"];
const ROUTE_NETWORKS_FIELDS: &[&str] = &["route_id", "network_id"];
const ROUTE_NETWORKS_REQUIRED_FIELDS: &[&str] = &["route_id", "network_id"];
const BOOKING_RULES_FIELDS: &[&str] = &[
    "booking_rule_id",
    "booking_type",
    "prior_notice_duration_min",
    "prior_notice_duration_max",
    "prior_notice_start_day",
    "prior_notice_start_time",
    "prior_notice_last_day",
    "prior_notice_last_time",
    "prior_notice_service_id",
    "message",
    "pickup_message",
    "drop_off_message",
    "phone_number",
    "info_url",
    "booking_url",
];
const BOOKING_RULES_REQUIRED_FIELDS: &[&str] = &["booking_rule_id", "booking_type"];

static AGENCY_SCHEMA: CsvSchema = CsvSchema {
    fields: AGENCY_FIELDS,
    required_fields: AGENCY_REQUIRED_FIELDS,
    recommended_fields: AGENCY_RECOMMENDED_FIELDS,
};
static STOPS_SCHEMA: CsvSchema = CsvSchema {
    fields: STOPS_FIELDS,
    required_fields: STOPS_REQUIRED_FIELDS,
    recommended_fields: STOPS_RECOMMENDED_FIELDS,
};
static ROUTES_SCHEMA: CsvSchema = CsvSchema {
    fields: ROUTES_FIELDS,
    required_fields: ROUTES_REQUIRED_FIELDS,
    recommended_fields: ROUTES_RECOMMENDED_FIELDS,
};
static TRIPS_SCHEMA: CsvSchema = CsvSchema {
    fields: TRIPS_FIELDS,
    required_fields: TRIPS_REQUIRED_FIELDS,
    recommended_fields: TRIPS_RECOMMENDED_FIELDS,
};
static STOP_TIMES_SCHEMA: CsvSchema = CsvSchema {
    fields: STOP_TIMES_FIELDS,
    required_fields: STOP_TIMES_REQUIRED_FIELDS,
    recommended_fields: STOP_TIMES_RECOMMENDED_FIELDS,
};
static CALENDAR_SCHEMA: CsvSchema = CsvSchema {
    fields: CALENDAR_FIELDS,
    required_fields: CALENDAR_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static CALENDAR_DATES_SCHEMA: CsvSchema = CsvSchema {
    fields: CALENDAR_DATES_FIELDS,
    required_fields: CALENDAR_DATES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_ATTRIBUTES_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_ATTRIBUTES_FIELDS,
    required_fields: FARE_ATTRIBUTES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_RULES_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_RULES_FIELDS,
    required_fields: FARE_RULES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static SHAPES_SCHEMA: CsvSchema = CsvSchema {
    fields: SHAPES_FIELDS,
    required_fields: SHAPES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FREQUENCIES_SCHEMA: CsvSchema = CsvSchema {
    fields: FREQUENCIES_FIELDS,
    required_fields: FREQUENCIES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static TRANSFERS_SCHEMA: CsvSchema = CsvSchema {
    fields: TRANSFERS_FIELDS,
    required_fields: TRANSFERS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static PATHWAYS_SCHEMA: CsvSchema = CsvSchema {
    fields: PATHWAYS_FIELDS,
    required_fields: PATHWAYS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FEED_INFO_SCHEMA: CsvSchema = CsvSchema {
    fields: FEED_INFO_FIELDS,
    required_fields: FEED_INFO_REQUIRED_FIELDS,
    recommended_fields: FEED_INFO_RECOMMENDED_FIELDS,
};
static ATTRIBUTIONS_SCHEMA: CsvSchema = CsvSchema {
    fields: ATTRIBUTIONS_FIELDS,
    required_fields: ATTRIBUTIONS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static LEVELS_SCHEMA: CsvSchema = CsvSchema {
    fields: LEVELS_FIELDS,
    required_fields: LEVELS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static TRANSLATIONS_SCHEMA: CsvSchema = CsvSchema {
    fields: TRANSLATIONS_FIELDS,
    required_fields: TRANSLATIONS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_MEDIA_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_MEDIA_FIELDS,
    required_fields: FARE_MEDIA_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_PRODUCTS_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_PRODUCTS_FIELDS,
    required_fields: FARE_PRODUCTS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_LEG_RULES_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_LEG_RULES_FIELDS,
    required_fields: FARE_LEG_RULES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_TRANSFER_RULES_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_TRANSFER_RULES_FIELDS,
    required_fields: FARE_TRANSFER_RULES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static FARE_LEG_JOIN_RULES_SCHEMA: CsvSchema = CsvSchema {
    fields: FARE_LEG_JOIN_RULES_FIELDS,
    required_fields: FARE_LEG_JOIN_RULES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static AREAS_SCHEMA: CsvSchema = CsvSchema {
    fields: AREAS_FIELDS,
    required_fields: AREAS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static STOP_AREAS_SCHEMA: CsvSchema = CsvSchema {
    fields: STOP_AREAS_FIELDS,
    required_fields: STOP_AREAS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static TIMEFRAMES_SCHEMA: CsvSchema = CsvSchema {
    fields: TIMEFRAMES_FIELDS,
    required_fields: TIMEFRAMES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static RIDER_CATEGORIES_SCHEMA: CsvSchema = CsvSchema {
    fields: RIDER_CATEGORIES_FIELDS,
    required_fields: RIDER_CATEGORIES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static LOCATION_GROUPS_SCHEMA: CsvSchema = CsvSchema {
    fields: LOCATION_GROUPS_FIELDS,
    required_fields: LOCATION_GROUPS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static LOCATION_GROUP_STOPS_SCHEMA: CsvSchema = CsvSchema {
    fields: LOCATION_GROUP_STOPS_FIELDS,
    required_fields: LOCATION_GROUP_STOPS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static NETWORKS_SCHEMA: CsvSchema = CsvSchema {
    fields: NETWORKS_FIELDS,
    required_fields: NETWORKS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static ROUTE_NETWORKS_SCHEMA: CsvSchema = CsvSchema {
    fields: ROUTE_NETWORKS_FIELDS,
    required_fields: ROUTE_NETWORKS_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};
static BOOKING_RULES_SCHEMA: CsvSchema = CsvSchema {
    fields: BOOKING_RULES_FIELDS,
    required_fields: BOOKING_RULES_REQUIRED_FIELDS,
    recommended_fields: NO_RECOMMENDED_FIELDS,
};

pub fn schema_for_file(file_name: &str) -> Option<&'static CsvSchema> {
    match file_name {
        AGENCY_FILE => Some(&AGENCY_SCHEMA),
        STOPS_FILE => Some(&STOPS_SCHEMA),
        ROUTES_FILE => Some(&ROUTES_SCHEMA),
        TRIPS_FILE => Some(&TRIPS_SCHEMA),
        STOP_TIMES_FILE => Some(&STOP_TIMES_SCHEMA),
        CALENDAR_FILE => Some(&CALENDAR_SCHEMA),
        CALENDAR_DATES_FILE => Some(&CALENDAR_DATES_SCHEMA),
        FARE_ATTRIBUTES_FILE => Some(&FARE_ATTRIBUTES_SCHEMA),
        FARE_RULES_FILE => Some(&FARE_RULES_SCHEMA),
        SHAPES_FILE => Some(&SHAPES_SCHEMA),
        FREQUENCIES_FILE => Some(&FREQUENCIES_SCHEMA),
        TRANSFERS_FILE => Some(&TRANSFERS_SCHEMA),
        PATHWAYS_FILE => Some(&PATHWAYS_SCHEMA),
        FEED_INFO_FILE => Some(&FEED_INFO_SCHEMA),
        ATTRIBUTIONS_FILE => Some(&ATTRIBUTIONS_SCHEMA),
        LEVELS_FILE => Some(&LEVELS_SCHEMA),
        TRANSLATIONS_FILE => Some(&TRANSLATIONS_SCHEMA),
        FARE_MEDIA_FILE => Some(&FARE_MEDIA_SCHEMA),
        FARE_PRODUCTS_FILE => Some(&FARE_PRODUCTS_SCHEMA),
        FARE_LEG_RULES_FILE => Some(&FARE_LEG_RULES_SCHEMA),
        FARE_TRANSFER_RULES_FILE => Some(&FARE_TRANSFER_RULES_SCHEMA),
        FARE_LEG_JOIN_RULES_FILE => Some(&FARE_LEG_JOIN_RULES_SCHEMA),
        AREAS_FILE => Some(&AREAS_SCHEMA),
        STOP_AREAS_FILE => Some(&STOP_AREAS_SCHEMA),
        TIMEFRAMES_FILE => Some(&TIMEFRAMES_SCHEMA),
        RIDER_CATEGORIES_FILE => Some(&RIDER_CATEGORIES_SCHEMA),
        LOCATION_GROUPS_FILE => Some(&LOCATION_GROUPS_SCHEMA),
        LOCATION_GROUP_STOPS_FILE => Some(&LOCATION_GROUP_STOPS_SCHEMA),
        NETWORKS_FILE => Some(&NETWORKS_SCHEMA),
        ROUTE_NETWORKS_FILE => Some(&ROUTE_NETWORKS_SCHEMA),
        BOOKING_RULES_FILE => Some(&BOOKING_RULES_SCHEMA),
        _ => None,
    }
}
