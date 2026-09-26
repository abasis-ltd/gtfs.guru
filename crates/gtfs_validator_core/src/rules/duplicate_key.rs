use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;

use crate::feed::{
    AGENCY_FILE, AREAS_FILE, ATTRIBUTIONS_FILE, BOOKING_RULES_FILE, CALENDAR_FILE,
    FARE_ATTRIBUTES_FILE, FARE_LEG_RULES_FILE, FARE_MEDIA_FILE, FARE_PRODUCTS_FILE,
    FARE_RULES_FILE, FARE_TRANSFER_RULES_FILE, FREQUENCIES_FILE, LEVELS_FILE, LOCATION_GROUPS_FILE,
    NETWORKS_FILE, PATHWAYS_FILE, RIDER_CATEGORIES_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE,
    SHAPES_FILE, STOPS_FILE, STOP_AREAS_FILE, TIMEFRAMES_FILE, TRANSFERS_FILE, TRANSLATIONS_FILE,
    TRIPS_FILE,
};
use crate::validation_context::thorough_mode_enabled;
use crate::{
    CsvTable, GtfsFeed, NoticeContainer, NoticeSeverity, StringPool, ValidationNotice, Validator,
};
use gtfs_guru_model::StringId;

const CODE_DUPLICATE_KEY: &str = "duplicate_key";

#[derive(Debug, Default)]
pub struct DuplicateKeyValidator;

impl Validator for DuplicateKeyValidator {
    fn name(&self) -> &'static str {
        "duplicate_key"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Agency: agency_id (only when multiple agencies exist)
        if feed.agency.rows.len() > 1 {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in feed.agency.rows.iter().enumerate() {
                let row_number = feed.agency.row_number(index);
                if let Some(agency_id) = row.agency_id {
                    if agency_id.0 != 0 {
                        if let Some(prev_row) = seen.get(&agency_id) {
                            let id_value = feed.pool.resolve(agency_id);
                            notices.push(duplicate_key_notice(
                                AGENCY_FILE,
                                row_number,
                                "agency_id",
                                id_value.as_str(),
                                *prev_row,
                            ));
                        } else {
                            seen.insert(agency_id, row_number);
                        }
                    }
                }
            }
        }

        // Stops: stop_id
        {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in feed.stops.rows.iter().enumerate() {
                let row_number = feed.stops.row_number(index);
                let id = row.stop_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            STOPS_FILE,
                            row_number,
                            "stop_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Routes: route_id
        {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in feed.routes.rows.iter().enumerate() {
                let row_number = feed.routes.row_number(index);
                let id = row.route_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            ROUTES_FILE,
                            row_number,
                            "route_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Trips: trip_id
        {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in feed.trips.rows.iter().enumerate() {
                let row_number = feed.trips.row_number(index);
                let id = row.trip_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            TRIPS_FILE,
                            row_number,
                            "trip_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Calendar: service_id
        if let Some(ref calendar) = feed.calendar {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in calendar.rows.iter().enumerate() {
                let row_number = calendar.row_number(index);
                let id = row.service_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            CALENDAR_FILE,
                            row_number,
                            "service_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Fare attributes: fare_id
        if let Some(ref fare_attributes) = feed.fare_attributes {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in fare_attributes.rows.iter().enumerate() {
                let row_number = fare_attributes.row_number(index);
                let id = row.fare_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            FARE_ATTRIBUTES_FILE,
                            row_number,
                            "fare_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Fare media: fare_media_id
        if let Some(ref fare_media) = feed.fare_media {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in fare_media.rows.iter().enumerate() {
                let row_number = fare_media.row_number(index);
                let id = row.fare_media_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            FARE_MEDIA_FILE,
                            row_number,
                            "fare_media_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Fare products: (fare_product_id, fare_media_id, rider_category_id)
        //
        // The specification's primary key for fare_products.txt is the triple,
        // and the canonical validator keys on the same triple. A product sold
        // to several rider categories, or on several media, repeats its id on
        // purpose. Thorough mode keeps the stricter reading of a globally
        // unique fare_product_id.
        if let Some(ref fare_products) = feed.fare_products {
            if thorough_mode_enabled() {
                let mut seen: HashMap<StringId, u64> = HashMap::new();
                for (index, row) in fare_products.rows.iter().enumerate() {
                    let row_number = fare_products.row_number(index);
                    let id = row.fare_product_id;
                    if id.0 == 0 {
                        continue;
                    }
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            FARE_PRODUCTS_FILE,
                            row_number,
                            "fare_product_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            } else {
                check_composite_key(
                    notices,
                    FARE_PRODUCTS_FILE,
                    fare_products,
                    |row| {
                        (
                            row.fare_product_id,
                            row.fare_media_id.unwrap_or(StringId(0)),
                            row.rider_category_id.unwrap_or(StringId(0)),
                        )
                    },
                    |row| {
                        vec![
                            ("fare_product_id", id_value(&feed.pool, row.fare_product_id)),
                            ("fare_media_id", opt_id_value(&feed.pool, row.fare_media_id)),
                            (
                                "rider_category_id",
                                opt_id_value(&feed.pool, row.rider_category_id),
                            ),
                        ]
                    },
                );
            }
        }

        // Areas: area_id
        if let Some(ref areas) = feed.areas {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in areas.rows.iter().enumerate() {
                let row_number = areas.row_number(index);
                let id = row.area_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            AREAS_FILE,
                            row_number,
                            "area_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Levels: level_id
        if let Some(ref levels) = feed.levels {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in levels.rows.iter().enumerate() {
                let row_number = levels.row_number(index);
                let id = row.level_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            LEVELS_FILE,
                            row_number,
                            "level_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Pathways: pathway_id
        if let Some(ref pathways) = feed.pathways {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in pathways.rows.iter().enumerate() {
                let row_number = pathways.row_number(index);
                let id = row.pathway_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            PATHWAYS_FILE,
                            row_number,
                            "pathway_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Location groups: location_group_id
        if let Some(ref location_groups) = feed.location_groups {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in location_groups.rows.iter().enumerate() {
                let row_number = location_groups.row_number(index);
                let id = row.location_group_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            LOCATION_GROUPS_FILE,
                            row_number,
                            "location_group_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Booking rules: booking_rule_id
        if let Some(ref booking_rules) = feed.booking_rules {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in booking_rules.rows.iter().enumerate() {
                let row_number = booking_rules.row_number(index);
                let id = row.booking_rule_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            BOOKING_RULES_FILE,
                            row_number,
                            "booking_rule_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Networks: network_id
        if let Some(ref networks) = feed.networks {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in networks.rows.iter().enumerate() {
                let row_number = networks.row_number(index);
                let id = row.network_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            NETWORKS_FILE,
                            row_number,
                            "network_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Rider categories: rider_category_id
        if let Some(ref rider_categories) = feed.rider_categories {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in rider_categories.rows.iter().enumerate() {
                let row_number = rider_categories.row_number(index);
                let id = row.rider_category_id;
                if id.0 != 0 {
                    if let Some(prev_row) = seen.get(&id) {
                        let id_value = feed.pool.resolve(id);
                        notices.push(duplicate_key_notice(
                            RIDER_CATEGORIES_FILE,
                            row_number,
                            "rider_category_id",
                            id_value.as_str(),
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // The remaining tables have composite primary keys. The canonical
        // validator compares every key column, empty ones included, and names
        // only the columns the row fills.

        // Transfers: from_stop_id, to_stop_id, from_trip_id, to_trip_id,
        // from_route_id, to_route_id
        if let Some(ref transfers) = feed.transfers {
            check_composite_key(
                notices,
                TRANSFERS_FILE,
                transfers,
                |row| {
                    (
                        row.from_stop_id.unwrap_or(StringId(0)),
                        row.to_stop_id.unwrap_or(StringId(0)),
                        row.from_trip_id.unwrap_or(StringId(0)),
                        row.to_trip_id.unwrap_or(StringId(0)),
                        row.from_route_id.unwrap_or(StringId(0)),
                        row.to_route_id.unwrap_or(StringId(0)),
                    )
                },
                |row| {
                    vec![
                        ("from_stop_id", opt_id_value(&feed.pool, row.from_stop_id)),
                        ("to_stop_id", opt_id_value(&feed.pool, row.to_stop_id)),
                        ("from_trip_id", opt_id_value(&feed.pool, row.from_trip_id)),
                        ("to_trip_id", opt_id_value(&feed.pool, row.to_trip_id)),
                        ("from_route_id", opt_id_value(&feed.pool, row.from_route_id)),
                        ("to_route_id", opt_id_value(&feed.pool, row.to_route_id)),
                    ]
                },
            );
        }

        // Shapes: shape_id, shape_pt_sequence
        if let Some(ref shapes) = feed.shapes {
            check_composite_key(
                notices,
                SHAPES_FILE,
                shapes,
                |row| (row.shape_id, row.shape_pt_sequence),
                |row| {
                    vec![
                        ("shape_id", id_value(&feed.pool, row.shape_id)),
                        ("shape_pt_sequence", Some(row.shape_pt_sequence.to_string())),
                    ]
                },
            );
        }

        // Frequencies: trip_id, start_time
        if let Some(ref frequencies) = feed.frequencies {
            check_composite_key(
                notices,
                FREQUENCIES_FILE,
                frequencies,
                |row| (row.trip_id, row.start_time),
                |row| {
                    vec![
                        ("trip_id", id_value(&feed.pool, row.trip_id)),
                        ("start_time", Some(row.start_time.to_string())),
                    ]
                },
            );
        }

        // Fare rules: fare_id, route_id, origin_id, destination_id, contains_id
        if let Some(ref fare_rules) = feed.fare_rules {
            check_composite_key(
                notices,
                FARE_RULES_FILE,
                fare_rules,
                |row| {
                    (
                        row.fare_id,
                        row.route_id.unwrap_or(StringId(0)),
                        row.origin_id.unwrap_or(StringId(0)),
                        row.destination_id.unwrap_or(StringId(0)),
                        row.contains_id.unwrap_or(StringId(0)),
                    )
                },
                |row| {
                    vec![
                        ("fare_id", id_value(&feed.pool, row.fare_id)),
                        ("route_id", opt_id_value(&feed.pool, row.route_id)),
                        ("origin_id", opt_id_value(&feed.pool, row.origin_id)),
                        (
                            "destination_id",
                            opt_id_value(&feed.pool, row.destination_id),
                        ),
                        ("contains_id", opt_id_value(&feed.pool, row.contains_id)),
                    ]
                },
            );
        }

        // Fare leg rules: network_id, from_area_id, to_area_id,
        // from_timeframe_group_id, to_timeframe_group_id, fare_product_id.
        // leg_group_id is not part of the key.
        if let Some(ref fare_leg_rules) = feed.fare_leg_rules {
            check_composite_key(
                notices,
                FARE_LEG_RULES_FILE,
                fare_leg_rules,
                |row| {
                    (
                        row.network_id.unwrap_or(StringId(0)),
                        row.from_area_id.unwrap_or(StringId(0)),
                        row.to_area_id.unwrap_or(StringId(0)),
                        row.from_timeframe_group_id.unwrap_or(StringId(0)),
                        row.to_timeframe_group_id.unwrap_or(StringId(0)),
                        row.fare_product_id,
                    )
                },
                |row| {
                    vec![
                        ("network_id", opt_id_value(&feed.pool, row.network_id)),
                        ("from_area_id", opt_id_value(&feed.pool, row.from_area_id)),
                        ("to_area_id", opt_id_value(&feed.pool, row.to_area_id)),
                        (
                            "from_timeframe_group_id",
                            opt_id_value(&feed.pool, row.from_timeframe_group_id),
                        ),
                        (
                            "to_timeframe_group_id",
                            opt_id_value(&feed.pool, row.to_timeframe_group_id),
                        ),
                        ("fare_product_id", id_value(&feed.pool, row.fare_product_id)),
                    ]
                },
            );
        }

        // Fare transfer rules: from_leg_group_id, to_leg_group_id,
        // duration_limit, transfer_count, fare_product_id
        if let Some(ref fare_transfer_rules) = feed.fare_transfer_rules {
            check_composite_key(
                notices,
                FARE_TRANSFER_RULES_FILE,
                fare_transfer_rules,
                |row| {
                    (
                        row.from_leg_group_id.unwrap_or(StringId(0)),
                        row.to_leg_group_id.unwrap_or(StringId(0)),
                        row.duration_limit,
                        row.transfer_count,
                        row.fare_product_id.unwrap_or(StringId(0)),
                    )
                },
                |row| {
                    vec![
                        (
                            "from_leg_group_id",
                            opt_id_value(&feed.pool, row.from_leg_group_id),
                        ),
                        (
                            "to_leg_group_id",
                            opt_id_value(&feed.pool, row.to_leg_group_id),
                        ),
                        ("duration_limit", row.duration_limit.map(|v| v.to_string())),
                        ("transfer_count", row.transfer_count.map(|v| v.to_string())),
                        (
                            "fare_product_id",
                            opt_id_value(&feed.pool, row.fare_product_id),
                        ),
                    ]
                },
            );
        }

        // Stop areas: area_id, stop_id
        if let Some(ref stop_areas) = feed.stop_areas {
            check_composite_key(
                notices,
                STOP_AREAS_FILE,
                stop_areas,
                |row| (row.area_id, row.stop_id),
                |row| {
                    vec![
                        ("area_id", id_value(&feed.pool, row.area_id)),
                        ("stop_id", id_value(&feed.pool, row.stop_id)),
                    ]
                },
            );
        }

        // Timeframes: timeframe_group_id, start_time, end_time, service_id
        if let Some(ref timeframes) = feed.timeframes {
            check_composite_key(
                notices,
                TIMEFRAMES_FILE,
                timeframes,
                |row| {
                    (
                        row.timeframe_group_id.unwrap_or(StringId(0)),
                        row.start_time,
                        row.end_time,
                        row.service_id,
                    )
                },
                |row| {
                    vec![
                        (
                            "timeframe_group_id",
                            opt_id_value(&feed.pool, row.timeframe_group_id),
                        ),
                        ("start_time", row.start_time.map(|t| t.to_string())),
                        ("end_time", row.end_time.map(|t| t.to_string())),
                        ("service_id", id_value(&feed.pool, row.service_id)),
                    ]
                },
            );
        }

        // Translations: table_name, field_name, language, record_id,
        // record_sub_id, field_value. A legacy translations.txt without
        // table_name fails the canonical header check and loads no rows.
        if let Some(ref translations) = feed.translations {
            if translations
                .headers
                .iter()
                .any(|header| header.trim() == "table_name")
            {
                check_composite_key(
                    notices,
                    TRANSLATIONS_FILE,
                    translations,
                    |row| {
                        (
                            row.table_name.unwrap_or(StringId(0)),
                            row.field_name.unwrap_or(StringId(0)),
                            row.language,
                            row.record_id.unwrap_or(StringId(0)),
                            row.record_sub_id.unwrap_or(StringId(0)),
                            row.field_value.clone().unwrap_or_default(),
                        )
                    },
                    |row| {
                        vec![
                            ("table_name", opt_id_value(&feed.pool, row.table_name)),
                            ("field_name", opt_id_value(&feed.pool, row.field_name)),
                            ("language", id_value(&feed.pool, row.language)),
                            ("record_id", opt_id_value(&feed.pool, row.record_id)),
                            ("record_sub_id", opt_id_value(&feed.pool, row.record_sub_id)),
                            (
                                "field_value",
                                row.field_value
                                    .as_ref()
                                    .filter(|value| !value.is_empty())
                                    .map(|value| value.to_string()),
                            ),
                        ]
                    },
                );
            }
        }

        // Attributions: attribution_id (optional; only filled ids are keys)
        if let Some(ref attributions) = feed.attributions {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in attributions.rows.iter().enumerate() {
                let row_number = attributions.row_number(index);
                let Some(id) = row.attribution_id.filter(|id| id.0 != 0) else {
                    continue;
                };
                if let Some(prev_row) = seen.get(&id) {
                    let id_value = feed.pool.resolve(id);
                    notices.push(duplicate_key_notice(
                        ATTRIBUTIONS_FILE,
                        row_number,
                        "attribution_id",
                        id_value.as_str(),
                        *prev_row,
                    ));
                } else {
                    seen.insert(id, row_number);
                }
            }
        }

        // Route networks: route_id (a route belongs to at most one network)
        if let Some(ref route_networks) = feed.route_networks {
            let mut seen: HashMap<StringId, u64> = HashMap::new();
            for (index, row) in route_networks.rows.iter().enumerate() {
                let row_number = route_networks.row_number(index);
                let id = row.route_id;
                if id.0 == 0 {
                    continue;
                }
                if let Some(prev_row) = seen.get(&id) {
                    let id_value = feed.pool.resolve(id);
                    notices.push(duplicate_key_notice(
                        ROUTE_NETWORKS_FILE,
                        row_number,
                        "route_id",
                        id_value.as_str(),
                        *prev_row,
                    ));
                } else {
                    seen.insert(id, row_number);
                }
            }
        }
    }
}

fn id_value(pool: &StringPool, id: StringId) -> Option<String> {
    (id.0 != 0).then(|| pool.resolve(id))
}

fn opt_id_value(pool: &StringPool, id: Option<StringId>) -> Option<String> {
    id.and_then(|id| id_value(pool, id))
}

/// Reports rows whose composite primary key repeats an earlier row's, in file
/// order, against the first row that holds the key. Like the canonical
/// `CompositeKey`, equality covers every key column, empty ones included.
fn check_composite_key<T, K, KF, CF>(
    notices: &mut NoticeContainer,
    filename: &str,
    table: &CsvTable<T>,
    key_of: KF,
    columns_of: CF,
) where
    K: Eq + Hash,
    KF: Fn(&T) -> K,
    CF: Fn(&T) -> Vec<(&'static str, Option<String>)>,
{
    let mut seen: HashMap<K, u64> = HashMap::new();
    for (index, row) in table.rows.iter().enumerate() {
        let row_number = table.row_number(index);
        match seen.entry(key_of(row)) {
            Entry::Occupied(prev) => {
                notices.push(composite_duplicate_key_notice(
                    filename,
                    row_number,
                    &columns_of(row),
                    *prev.get(),
                ));
            }
            Entry::Vacant(slot) => {
                slot.insert(row_number);
            }
        }
    }
}

/// The canonical notice names only the key columns the row fills
/// (`getDefinedKeys`) and joins their values with commas
/// (`getDefinedValues`); with no filled column the value is null and drops
/// out of the report.
fn composite_duplicate_key_notice(
    filename: &str,
    row_number: u64,
    columns: &[(&'static str, Option<String>)],
    prev_row_number: u64,
) -> ValidationNotice {
    let defined: Vec<(&str, &str)> = columns
        .iter()
        .filter_map(|(name, value)| value.as_deref().map(|value| (*name, value)))
        .collect();
    let names = defined
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(",");
    let mut notice = duplicate_key_notice(filename, row_number, &names, "", prev_row_number);
    if defined.is_empty() {
        notice.context.remove("fieldValue1");
    } else {
        let values = defined
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>()
            .join(",");
        notice.insert_context_field("fieldValue1", values);
    }
    notice
}

fn duplicate_key_notice(
    filename: &str,
    row_number: u64,
    field_name: &str,
    field_value: &str,
    prev_row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_DUPLICATE_KEY,
        NoticeSeverity::Error,
        "Duplicate primary key value",
    );
    // Field names follow the canonical validator's DuplicateKeyNotice.
    notice.insert_context_field("filename", filename);
    notice.insert_context_field("oldCsvRowNumber", prev_row_number);
    notice.insert_context_field("newCsvRowNumber", row_number);
    notice.insert_context_field("fieldName1", field_name);
    notice.insert_context_field("fieldValue1", field_value);
    notice.field_order = vec![
        "filename".into(),
        "oldCsvRowNumber".into(),
        "newCsvRowNumber".into(),
        "fieldName1".into(),
        "fieldValue1".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{
        FareLegRule, FareProduct, Frequency, GtfsTime, Route, RouteNetwork, RouteType, Shape, Stop,
        Transfer, TransferType, Translation, Trip,
    };

    fn only_notice(notices: &NoticeContainer) -> &ValidationNotice {
        assert_eq!(notices.len(), 1, "{:?}", notices.iter().collect::<Vec<_>>());
        notices.iter().next().unwrap()
    }

    fn key_fields(notice: &ValidationNotice) -> (u64, u64, &str, Option<&str>) {
        (
            notice.context["oldCsvRowNumber"].as_u64().unwrap(),
            notice.context["newCsvRowNumber"].as_u64().unwrap(),
            notice.context["fieldName1"].as_str().unwrap(),
            notice.context.get("fieldValue1").and_then(|v| v.as_str()),
        )
    }

    #[test]
    fn detects_duplicate_shape_point_against_first_row() {
        let mut feed = GtfsFeed::default();
        let shape = feed.pool.intern("shape1");
        let point = |sequence| Shape {
            shape_id: shape,
            shape_pt_sequence: sequence,
            ..Default::default()
        };
        feed.shapes = Some(CsvTable {
            headers: vec!["shape_id".into(), "shape_pt_sequence".into()],
            rows: vec![point(4), point(5), point(4), point(4)],
            row_numbers: vec![5, 6, 9, 10],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        let found: Vec<_> = notices.iter().map(key_fields).collect();
        assert_eq!(
            found,
            vec![
                (5, 9, "shape_id,shape_pt_sequence", Some("shape1,4")),
                (5, 10, "shape_id,shape_pt_sequence", Some("shape1,4")),
            ]
        );
    }

    #[test]
    fn detects_duplicate_frequency_start_time() {
        let mut feed = GtfsFeed::default();
        let trip = feed.pool.intern("trip1");
        let frequency = |end| Frequency {
            trip_id: trip,
            start_time: GtfsTime::from_seconds(8 * 3600),
            end_time: GtfsTime::from_seconds(end),
            headway_secs: 600,
            ..Default::default()
        };
        feed.frequencies = Some(CsvTable {
            headers: vec!["trip_id".into(), "start_time".into()],
            rows: vec![frequency(9 * 3600), frequency(10 * 3600)],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(
            key_fields(only_notice(&notices)),
            (2, 3, "trip_id,start_time", Some("trip1,08:00:00"))
        );
    }

    #[test]
    fn transfer_key_names_only_filled_columns_in_canonical_order() {
        let mut feed = GtfsFeed::default();
        let in_seat = Transfer {
            from_trip_id: Some(feed.pool.intern("trip1")),
            to_trip_id: Some(feed.pool.intern("trip2")),
            transfer_type: Some(TransferType::InSeat),
            ..Default::default()
        };
        let full = Transfer {
            from_stop_id: Some(feed.pool.intern("stop1")),
            to_stop_id: Some(feed.pool.intern("stop2")),
            from_route_id: Some(feed.pool.intern("route1")),
            to_route_id: Some(feed.pool.intern("route1")),
            from_trip_id: Some(feed.pool.intern("trip1")),
            to_trip_id: Some(feed.pool.intern("trip2")),
            ..Default::default()
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_trip_id".into()],
            rows: vec![
                in_seat.clone(),
                in_seat,
                full.clone(),
                full,
                Transfer::default(),
                Transfer::default(),
            ],
            row_numbers: vec![2, 3, 4, 5, 6, 7],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        let found: Vec<_> = notices.iter().map(key_fields).collect();
        assert_eq!(
            found,
            vec![
                (2, 3, "from_trip_id,to_trip_id", Some("trip1,trip2")),
                (
                    4,
                    5,
                    "from_stop_id,to_stop_id,from_trip_id,to_trip_id,from_route_id,to_route_id",
                    Some("stop1,stop2,trip1,trip2,route1,route1"),
                ),
                // Nothing filled: an empty name and no value.
                (6, 7, "", None),
            ]
        );
    }

    #[test]
    fn fare_product_key_names_its_filled_columns() {
        let mut feed = GtfsFeed::default();
        let product = FareProduct {
            fare_product_id: feed.pool.intern("P"),
            fare_media_id: Some(feed.pool.intern("M")),
            rider_category_id: Some(feed.pool.intern("adult")),
            ..Default::default()
        };
        feed.fare_products = Some(CsvTable {
            headers: vec!["fare_product_id".into()],
            rows: vec![product.clone(), product],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(
            key_fields(only_notice(&notices)),
            (
                2,
                3,
                "fare_product_id,fare_media_id,rider_category_id",
                Some("P,M,adult")
            )
        );
    }

    #[test]
    fn fare_leg_rule_key_ignores_leg_group_id() {
        let mut feed = GtfsFeed::default();
        let product = feed.pool.intern("P");
        feed.fare_leg_rules = Some(CsvTable {
            headers: vec!["leg_group_id".into(), "fare_product_id".into()],
            rows: vec![
                FareLegRule {
                    leg_group_id: Some(feed.pool.intern("G1")),
                    fare_product_id: product,
                    ..Default::default()
                },
                FareLegRule {
                    leg_group_id: Some(feed.pool.intern("G2")),
                    fare_product_id: product,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(
            key_fields(only_notice(&notices)),
            (2, 3, "fare_product_id", Some("P"))
        );
    }

    #[test]
    fn detects_duplicate_translation_and_route_network() {
        let mut feed = GtfsFeed::default();
        let translation = |text: &str, pool: &crate::StringPool| Translation {
            table_name: Some(pool.intern("stops")),
            field_name: Some(pool.intern("stop_name")),
            language: pool.intern("fr"),
            translation: text.into(),
            field_value: Some("First Stop".into()),
            ..Default::default()
        };
        feed.translations = Some(CsvTable {
            headers: vec!["table_name".into()],
            rows: vec![
                translation("Arret", &feed.pool),
                translation("Arret2", &feed.pool),
            ],
            row_numbers: vec![2, 3],
        });
        let route = feed.pool.intern("route1");
        feed.route_networks = Some(CsvTable {
            headers: vec!["network_id".into(), "route_id".into()],
            rows: vec![
                RouteNetwork {
                    route_id: route,
                    network_id: feed.pool.intern("N1"),
                },
                RouteNetwork {
                    route_id: route,
                    network_id: feed.pool.intern("N2"),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        let found: Vec<_> = notices.iter().map(key_fields).collect();
        assert_eq!(
            found,
            vec![
                (
                    2,
                    3,
                    "table_name,field_name,language,field_value",
                    Some("stops,stop_name,fr,First Stop")
                ),
                (2, 3, "route_id", Some("route1")),
            ]
        );
    }

    #[test]
    fn detects_duplicate_stop_id() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_DUPLICATE_KEY);
        assert_eq!(
            notice.context.get("fieldName1").unwrap().as_str().unwrap(),
            "stop_id"
        );
        assert_eq!(
            notice.context.get("fieldValue1").unwrap().as_str().unwrap(),
            "S1"
        );
        assert_eq!(
            notice
                .context
                .get("newCsvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            3
        );
        assert_eq!(
            notice
                .context
                .get("oldCsvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            2
        );
    }

    #[test]
    fn fare_product_repeated_per_rider_category_is_not_a_duplicate() {
        let mut feed = GtfsFeed::default();
        let product = feed.pool.intern("TBM_20314");
        let media = feed.pool.intern("TBM_carte");
        let student = feed.pool.intern("TBM_etudiant");
        let pupil = feed.pool.intern("TBM_sco16");
        feed.fare_products = Some(CsvTable {
            headers: vec![
                "fare_product_id".into(),
                "fare_media_id".into(),
                "rider_category_id".into(),
            ],
            rows: vec![
                FareProduct {
                    fare_product_id: product,
                    fare_media_id: Some(media),
                    rider_category_id: Some(student),
                    ..Default::default()
                },
                FareProduct {
                    fare_product_id: product,
                    fare_media_id: Some(media),
                    rider_category_id: Some(pupil),
                    ..Default::default()
                },
                // The same triple again: this one is a duplicate.
                FareProduct {
                    fare_product_id: product,
                    fare_media_id: Some(media),
                    rider_category_id: Some(pupil),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        });

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_DUPLICATE_KEY);
        assert_eq!(
            notice
                .context
                .get("oldCsvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            3
        );
        assert_eq!(
            notice
                .context
                .get("newCsvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            4
        );
    }

    #[test]
    fn detects_duplicate_route_id() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_type".into()],
            rows: vec![
                Route {
                    route_id: feed.pool.intern("R1"),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
                Route {
                    route_id: feed.pool.intern("R1"),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_DUPLICATE_KEY);
        assert_eq!(
            notice.context.get("fieldName1").unwrap().as_str().unwrap(),
            "route_id"
        );
    }

    #[test]
    fn detects_duplicate_trip_id() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![
                Trip {
                    trip_id: feed.pool.intern("T1"),
                    ..Default::default()
                },
                Trip {
                    trip_id: feed.pool.intern("T1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_DUPLICATE_KEY);
        assert_eq!(
            notice.context.get("fieldName1").unwrap().as_str().unwrap(),
            "trip_id"
        );
    }

    #[test]
    fn passes_with_unique_ids() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![
                Route {
                    route_id: feed.pool.intern("R1"),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
                Route {
                    route_id: feed.pool.intern("R2"),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn ignores_empty_ids() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: StringId(0),
                    ..Default::default()
                },
                Stop {
                    stop_id: StringId(0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
