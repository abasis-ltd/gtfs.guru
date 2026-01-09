use std::collections::HashSet;

use crate::{
    feed::{
        AREAS_FILE, ATTRIBUTIONS_FILE, BOOKING_RULES_FILE, FARE_LEG_JOIN_RULES_FILE,
        FARE_LEG_RULES_FILE, FARE_MEDIA_FILE, FARE_PRODUCTS_FILE, FARE_TRANSFER_RULES_FILE,
        FREQUENCIES_FILE, LEVELS_FILE, LOCATION_GROUPS_FILE, LOCATION_GROUP_STOPS_FILE,
        NETWORKS_FILE, PATHWAYS_FILE, RIDER_CATEGORIES_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE,
        STOPS_FILE, STOP_AREAS_FILE, STOP_TIMES_FILE, TIMEFRAMES_FILE, TRIPS_FILE,
    },
    validation_context::thorough_mode_enabled,
    GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator,
};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

const TRANSFERS_FILE: &str = "transfers.txt";
const FARE_RULES_FILE: &str = "fare_rules.txt";
const FARE_ATTRIBUTES_FILE: &str = "fare_attributes.txt";
const AGENCY_FILE: &str = "agency.txt";

#[derive(Debug, Default)]
pub struct ReferentialIntegrityValidator;

impl Validator for ReferentialIntegrityValidator {
    fn name(&self) -> &'static str {
        "referential_integrity"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let agency_ok = !feed.table_has_errors(AGENCY_FILE);
        let stops_ok = !feed.table_has_errors(STOPS_FILE);
        let routes_ok = !feed.table_has_errors(ROUTES_FILE);
        let trips_ok = !feed.table_has_errors(TRIPS_FILE);
        let stop_times_ok = !feed.table_has_errors(STOP_TIMES_FILE);
        let networks_ok = !feed.table_has_errors(NETWORKS_FILE);
        let areas_ok = !feed.table_has_errors(AREAS_FILE);
        let fare_media_ok = !feed.table_has_errors(FARE_MEDIA_FILE);
        let fare_products_ok = !feed.table_has_errors(FARE_PRODUCTS_FILE);
        let rider_categories_ok = !feed.table_has_errors(RIDER_CATEGORIES_FILE);
        let timeframes_ok = !feed.table_has_errors(TIMEFRAMES_FILE);
        let fare_leg_rules_ok = !feed.table_has_errors(FARE_LEG_RULES_FILE);
        let fare_transfer_rules_ok = !feed.table_has_errors(FARE_TRANSFER_RULES_FILE);
        let stop_areas_ok = !feed.table_has_errors(STOP_AREAS_FILE);
        let fare_leg_join_rules_ok = !feed.table_has_errors(FARE_LEG_JOIN_RULES_FILE);
        let location_groups_ok = !feed.table_has_errors(LOCATION_GROUPS_FILE);
        let location_group_stops_ok = !feed.table_has_errors(LOCATION_GROUP_STOPS_FILE);
        let booking_rules_ok = !feed.table_has_errors(BOOKING_RULES_FILE);
        let transfers_ok = !feed.table_has_errors(TRANSFERS_FILE);
        let route_networks_ok = !feed.table_has_errors(ROUTE_NETWORKS_FILE);
        let fare_rules_ok = !feed.table_has_errors(FARE_RULES_FILE);
        let fare_attributes_ok = !feed.table_has_errors(FARE_ATTRIBUTES_FILE);
        let levels_ok = !feed.table_has_errors(LEVELS_FILE);
        let pathways_ok = !feed.table_has_errors(PATHWAYS_FILE);
        let frequencies_ok = !feed.table_has_errors(FREQUENCIES_FILE);
        let attributions_ok = !feed.table_has_errors(ATTRIBUTIONS_FILE);

        let stop_ids: HashSet<gtfs_guru_model::StringId> = if stops_ok {
            feed.stops
                .rows
                .iter()
                .map(|stop| stop.stop_id)
                .filter(|id| id.0 != 0)
                .collect()
        } else {
            HashSet::new()
        };
        let trip_ids: HashSet<gtfs_guru_model::StringId> = if trips_ok {
            feed.trips
                .rows
                .iter()
                .map(|trip| trip.trip_id)
                .filter(|id| id.0 != 0)
                .collect()
        } else {
            HashSet::new()
        };
        let route_ids: HashSet<gtfs_guru_model::StringId> = if routes_ok {
            feed.routes
                .rows
                .iter()
                .map(|route| route.route_id)
                .filter(|id| id.0 != 0)
                .collect()
        } else {
            HashSet::new()
        };
        let network_ids: HashSet<gtfs_guru_model::StringId> = if networks_ok || routes_ok {
            let mut ids: HashSet<gtfs_guru_model::StringId> = feed
                .routes
                .rows
                .iter()
                .filter_map(|route| route.network_id)
                .filter(|id| id.0 != 0)
                .collect();
            if let Some(networks) = &feed.networks {
                for network in &networks.rows {
                    if network.network_id.0 != 0 {
                        ids.insert(network.network_id);
                    }
                }
            }
            ids
        } else {
            HashSet::new()
        };
        let area_ids: HashSet<gtfs_guru_model::StringId> = if areas_ok {
            feed.areas
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|area| area.area_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let fare_media_ids: HashSet<gtfs_guru_model::StringId> = if fare_media_ok {
            feed.fare_media
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|fare_media| fare_media.fare_media_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let fare_product_ids: HashSet<gtfs_guru_model::StringId> = if fare_products_ok {
            feed.fare_products
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|fare_product| fare_product.fare_product_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let rider_category_ids: HashSet<gtfs_guru_model::StringId> = if rider_categories_ok {
            feed.rider_categories
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|category| category.rider_category_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let timeframe_group_ids: HashSet<gtfs_guru_model::StringId> = if timeframes_ok {
            feed.timeframes
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .filter_map(|timeframe| timeframe.timeframe_group_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let fare_leg_group_ids: HashSet<gtfs_guru_model::StringId> = if fare_leg_rules_ok {
            feed.fare_leg_rules
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .filter_map(|rule| rule.leg_group_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let location_group_ids: HashSet<gtfs_guru_model::StringId> = if location_groups_ok {
            feed.location_groups
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|group| group.location_group_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let booking_rule_ids: HashSet<gtfs_guru_model::StringId> = if booking_rules_ok {
            feed.booking_rules
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|rule| rule.booking_rule_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let has_booking_rules = booking_rules_ok && feed.booking_rules.is_some();

        let fare_ids: HashSet<gtfs_guru_model::StringId> = if fare_attributes_ok {
            feed.fare_attributes
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|fare| fare.fare_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };

        let zone_ids: HashSet<gtfs_guru_model::StringId> = if stops_ok {
            feed.stops
                .rows
                .iter()
                .filter_map(|stop| stop.zone_id)
                .filter(|id| id.0 != 0)
                .collect()
        } else {
            HashSet::new()
        };

        let agency_ids: HashSet<gtfs_guru_model::StringId> = if agency_ok {
            feed.agency
                .rows
                .iter()
                .filter_map(|agency| agency.agency_id)
                .filter(|id| id.0 != 0)
                .collect()
        } else {
            HashSet::new()
        };
        let has_multiple_agencies = feed.agency.rows.len() > 1;

        let level_ids: HashSet<gtfs_guru_model::StringId> = if levels_ok {
            feed.levels
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|level| level.level_id)
                        .filter(|id| id.0 != 0)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let has_levels = levels_ok && feed.levels.is_some();

        if stop_times_ok {
            for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
                let row_number = feed.stop_times.row_number(index);
                if trips_ok {
                    let trip_id = stop_time.trip_id;
                    if trip_id.0 != 0 && !trip_ids.contains(&trip_id) {
                        let trip_id_value = feed.pool.resolve(trip_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            STOP_TIMES_FILE,
                            "trip_id",
                            TRIPS_FILE,
                            "trip_id",
                            trip_id_value.as_str(),
                            row_number,
                        ));
                    }
                }
                if stops_ok {
                    let stop_id = stop_time.stop_id;
                    if stop_id.0 != 0 && !stop_ids.contains(&stop_id) {
                        let stop_id_value = feed.pool.resolve(stop_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            STOP_TIMES_FILE,
                            "stop_id",
                            STOPS_FILE,
                            "stop_id",
                            stop_id_value.as_str(),
                            row_number,
                        ));
                    }
                }
                if location_groups_ok {
                    if let Some(group_id) = stop_time.location_group_id.filter(|id| id.0 != 0) {
                        if !location_group_ids.contains(&group_id) {
                            let group_id_value = feed.pool.resolve(group_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOP_TIMES_FILE,
                                "location_group_id",
                                LOCATION_GROUPS_FILE,
                                "location_group_id",
                                group_id_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
                if has_booking_rules {
                    if let Some(booking_rule_id) =
                        stop_time.pickup_booking_rule_id.filter(|id| id.0 != 0)
                    {
                        if !booking_rule_ids.contains(&booking_rule_id) {
                            let booking_rule_value = feed.pool.resolve(booking_rule_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOP_TIMES_FILE,
                                "pickup_booking_rule_id",
                                BOOKING_RULES_FILE,
                                "booking_rule_id",
                                booking_rule_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if let Some(booking_rule_id) =
                        stop_time.drop_off_booking_rule_id.filter(|id| id.0 != 0)
                    {
                        if !booking_rule_ids.contains(&booking_rule_id) {
                            let booking_rule_value = feed.pool.resolve(booking_rule_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOP_TIMES_FILE,
                                "drop_off_booking_rule_id",
                                BOOKING_RULES_FILE,
                                "booking_rule_id",
                                booking_rule_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
            }
        }

        if trips_ok && routes_ok {
            for (index, trip) in feed.trips.rows.iter().enumerate() {
                let row_number = feed.trips.row_number(index);
                let route_id = trip.route_id;
                if route_id.0 != 0 && !route_ids.contains(&route_id) {
                    let route_id_value = feed.pool.resolve(route_id);
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        TRIPS_FILE,
                        "route_id",
                        ROUTES_FILE,
                        "route_id",
                        route_id_value.as_str(),
                        row_number,
                    ));
                }
            }
        }

        // routes.txt → agency_id (only if multiple agencies)
        if routes_ok && agency_ok && has_multiple_agencies {
            for (index, route) in feed.routes.rows.iter().enumerate() {
                let row_number = feed.routes.row_number(index);
                if let Some(agency_id) = route.agency_id.filter(|id| id.0 != 0) {
                    if !agency_ids.contains(&agency_id) {
                        let val = feed.pool.resolve(agency_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            ROUTES_FILE,
                            "agency_id",
                            AGENCY_FILE,
                            "agency_id",
                            val.as_str(),
                            row_number,
                        ));
                    }
                }
            }
        }

        if stops_ok {
            for (index, stop) in feed.stops.rows.iter().enumerate() {
                let row_number = feed.stops.row_number(index);
                if let Some(parent_id) = stop.parent_station.filter(|id| id.0 != 0) {
                    if !stop_ids.contains(&parent_id) {
                        let parent_id_value = feed.pool.resolve(parent_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            STOPS_FILE,
                            "parent_station",
                            STOPS_FILE,
                            "stop_id",
                            parent_id_value.as_str(),
                            row_number,
                        ));
                    }
                }
                // stops.txt → level_id
                if has_levels {
                    if let Some(level_id) = stop.level_id.filter(|id| id.0 != 0) {
                        if !level_ids.contains(&level_id) {
                            let val = feed.pool.resolve(level_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOPS_FILE,
                                "level_id",
                                LEVELS_FILE,
                                "level_id",
                                val.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
            }
        }

        if fare_products_ok {
            if let Some(fare_products) = &feed.fare_products {
                for (index, product) in fare_products.rows.iter().enumerate() {
                    let row_number = fare_products.row_number(index);
                    if fare_media_ok {
                        if let Some(fare_media_id) = product.fare_media_id.filter(|id| id.0 != 0) {
                            if !fare_media_ids.contains(&fare_media_id) {
                                let fare_media_value = feed.pool.resolve(fare_media_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_PRODUCTS_FILE,
                                    "fare_media_id",
                                    FARE_MEDIA_FILE,
                                    "fare_media_id",
                                    fare_media_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if rider_categories_ok {
                        if let Some(rider_category_id) =
                            product.rider_category_id.filter(|id| id.0 != 0)
                        {
                            if !rider_category_ids.contains(&rider_category_id) {
                                let rider_category_value = feed.pool.resolve(rider_category_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_PRODUCTS_FILE,
                                    "rider_category_id",
                                    RIDER_CATEGORIES_FILE,
                                    "rider_category_id",
                                    rider_category_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if fare_leg_rules_ok {
            if let Some(fare_leg_rules) = &feed.fare_leg_rules {
                for (index, rule) in fare_leg_rules.rows.iter().enumerate() {
                    let row_number = fare_leg_rules.row_number(index);
                    if fare_products_ok {
                        let fare_product_id = rule.fare_product_id;
                        if !fare_product_ids.contains(&fare_product_id) {
                            let fare_product_value = feed.pool.resolve(fare_product_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                FARE_LEG_RULES_FILE,
                                "fare_product_id",
                                FARE_PRODUCTS_FILE,
                                "fare_product_id",
                                fare_product_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if areas_ok {
                        if let Some(area_id) = rule.from_area_id.filter(|id| id.0 != 0) {
                            if !area_ids.contains(&area_id) {
                                let area_value = feed.pool.resolve(area_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_RULES_FILE,
                                    "from_area_id",
                                    AREAS_FILE,
                                    "area_id",
                                    area_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(area_id) = rule.to_area_id.filter(|id| id.0 != 0) {
                            if !area_ids.contains(&area_id) {
                                let area_value = feed.pool.resolve(area_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_RULES_FILE,
                                    "to_area_id",
                                    AREAS_FILE,
                                    "area_id",
                                    area_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if timeframes_ok {
                        if let Some(timeframe_id) =
                            rule.from_timeframe_group_id.filter(|id| id.0 != 0)
                        {
                            if !timeframe_group_ids.contains(&timeframe_id) {
                                let timeframe_value = feed.pool.resolve(timeframe_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_RULES_FILE,
                                    "from_timeframe_group_id",
                                    TIMEFRAMES_FILE,
                                    "timeframe_group_id",
                                    timeframe_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(timeframe_id) =
                            rule.to_timeframe_group_id.filter(|id| id.0 != 0)
                        {
                            if !timeframe_group_ids.contains(&timeframe_id) {
                                let timeframe_value = feed.pool.resolve(timeframe_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_RULES_FILE,
                                    "to_timeframe_group_id",
                                    TIMEFRAMES_FILE,
                                    "timeframe_group_id",
                                    timeframe_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if fare_transfer_rules_ok {
            if let Some(fare_transfer_rules) = &feed.fare_transfer_rules {
                for (index, rule) in fare_transfer_rules.rows.iter().enumerate() {
                    let row_number = fare_transfer_rules.row_number(index);
                    if fare_leg_rules_ok {
                        if let Some(group_id) = rule.from_leg_group_id.filter(|id| id.0 != 0) {
                            if !fare_leg_group_ids.contains(&group_id) {
                                let group_value = feed.pool.resolve(group_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_TRANSFER_RULES_FILE,
                                    "from_leg_group_id",
                                    FARE_LEG_RULES_FILE,
                                    "leg_group_id",
                                    group_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(group_id) = rule.to_leg_group_id.filter(|id| id.0 != 0) {
                            if !fare_leg_group_ids.contains(&group_id) {
                                let group_value = feed.pool.resolve(group_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_TRANSFER_RULES_FILE,
                                    "to_leg_group_id",
                                    FARE_LEG_RULES_FILE,
                                    "leg_group_id",
                                    group_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if fare_products_ok {
                        if let Some(fare_product_id) = rule.fare_product_id.filter(|id| id.0 != 0) {
                            if !fare_product_ids.contains(&fare_product_id) {
                                let fare_product_value = feed.pool.resolve(fare_product_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_TRANSFER_RULES_FILE,
                                    "fare_product_id",
                                    FARE_PRODUCTS_FILE,
                                    "fare_product_id",
                                    fare_product_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if stop_areas_ok {
            if let Some(stop_areas) = &feed.stop_areas {
                for (index, row) in stop_areas.rows.iter().enumerate() {
                    let row_number = stop_areas.row_number(index);
                    if areas_ok {
                        let area_id = row.area_id;
                        if !area_ids.contains(&area_id) {
                            let area_value = feed.pool.resolve(area_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOP_AREAS_FILE,
                                "area_id",
                                AREAS_FILE,
                                "area_id",
                                area_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if stops_ok {
                        let stop_id = row.stop_id;
                        if !stop_ids.contains(&stop_id) {
                            let stop_value = feed.pool.resolve(stop_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                STOP_AREAS_FILE,
                                "stop_id",
                                STOPS_FILE,
                                "stop_id",
                                stop_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
            }
        }

        if fare_leg_join_rules_ok {
            if let Some(fare_leg_join_rules) = &feed.fare_leg_join_rules {
                for (index, row) in fare_leg_join_rules.rows.iter().enumerate() {
                    let row_number = fare_leg_join_rules.row_number(index);
                    if stops_ok {
                        if let Some(stop_id) = row.from_stop_id.filter(|id| id.0 != 0) {
                            if !stop_ids.contains(&stop_id) {
                                let stop_value = feed.pool.resolve(stop_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_JOIN_RULES_FILE,
                                    "from_stop_id",
                                    STOPS_FILE,
                                    "stop_id",
                                    stop_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(stop_id) = row.to_stop_id.filter(|id| id.0 != 0) {
                            if !stop_ids.contains(&stop_id) {
                                let stop_value = feed.pool.resolve(stop_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_JOIN_RULES_FILE,
                                    "to_stop_id",
                                    STOPS_FILE,
                                    "stop_id",
                                    stop_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    // area_id FK checks only in thorough mode (Java doesn't validate these)
                    if areas_ok && thorough_mode_enabled() {
                        if let Some(area_id) = row.from_area_id.filter(|id| id.0 != 0) {
                            if !area_ids.contains(&area_id) {
                                let area_value = feed.pool.resolve(area_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_JOIN_RULES_FILE,
                                    "from_area_id",
                                    AREAS_FILE,
                                    "area_id",
                                    area_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(area_id) = row.to_area_id.filter(|id| id.0 != 0) {
                            if !area_ids.contains(&area_id) {
                                let area_value = feed.pool.resolve(area_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_LEG_JOIN_RULES_FILE,
                                    "to_area_id",
                                    AREAS_FILE,
                                    "area_id",
                                    area_value.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if route_networks_ok {
            if let Some(route_networks) = &feed.route_networks {
                for (index, row) in route_networks.rows.iter().enumerate() {
                    let row_number = route_networks.row_number(index);
                    if routes_ok {
                        let route_id = row.route_id;
                        if !route_ids.contains(&route_id) {
                            let route_value = feed.pool.resolve(route_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                ROUTE_NETWORKS_FILE,
                                "route_id",
                                ROUTES_FILE,
                                "route_id",
                                route_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if networks_ok {
                        let network_id = row.network_id;
                        if !network_ids.is_empty() && !network_ids.contains(&network_id) {
                            let network_value = feed.pool.resolve(network_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                ROUTE_NETWORKS_FILE,
                                "network_id",
                                NETWORKS_FILE,
                                "network_id",
                                network_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
            }
        }

        if location_group_stops_ok {
            if let Some(location_group_stops) = &feed.location_group_stops {
                for (index, row) in location_group_stops.rows.iter().enumerate() {
                    let row_number = location_group_stops.row_number(index);
                    if location_groups_ok {
                        let location_group_id = row.location_group_id;
                        if !location_group_ids.contains(&location_group_id) {
                            let group_value = feed.pool.resolve(location_group_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                LOCATION_GROUP_STOPS_FILE,
                                "location_group_id",
                                LOCATION_GROUPS_FILE,
                                "location_group_id",
                                group_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if stops_ok {
                        let stop_id = row.stop_id;
                        if !stop_ids.contains(&stop_id) {
                            let stop_value = feed.pool.resolve(stop_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                LOCATION_GROUP_STOPS_FILE,
                                "stop_id",
                                STOPS_FILE,
                                "stop_id",
                                stop_value.as_str(),
                                row_number,
                            ));
                        }
                    }
                }
            }
        }

        if transfers_ok {
            if let Some(transfers) = &feed.transfers {
                for (index, transfer) in transfers.rows.iter().enumerate() {
                    let row_number = transfers.row_number(index);
                    if stops_ok {
                        if let Some(from_stop_id) = transfer.from_stop_id.filter(|id| id.0 != 0) {
                            if !stop_ids.contains(&from_stop_id) {
                                let val = feed.pool.resolve(from_stop_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "from_stop_id",
                                    STOPS_FILE,
                                    "stop_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(to_stop_id) = transfer.to_stop_id.filter(|id| id.0 != 0) {
                            if !stop_ids.contains(&to_stop_id) {
                                let val = feed.pool.resolve(to_stop_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "to_stop_id",
                                    STOPS_FILE,
                                    "stop_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if routes_ok {
                        if let Some(from_route_id) = transfer.from_route_id.filter(|id| id.0 != 0) {
                            if !route_ids.contains(&from_route_id) {
                                let val = feed.pool.resolve(from_route_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "from_route_id",
                                    ROUTES_FILE,
                                    "route_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(to_route_id) = transfer.to_route_id.filter(|id| id.0 != 0) {
                            if !route_ids.contains(&to_route_id) {
                                let val = feed.pool.resolve(to_route_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "to_route_id",
                                    ROUTES_FILE,
                                    "route_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if trips_ok {
                        if let Some(from_trip_id) = transfer.from_trip_id.filter(|id| id.0 != 0) {
                            if !trip_ids.contains(&from_trip_id) {
                                let val = feed.pool.resolve(from_trip_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "from_trip_id",
                                    TRIPS_FILE,
                                    "trip_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(to_trip_id) = transfer.to_trip_id.filter(|id| id.0 != 0) {
                            if !trip_ids.contains(&to_trip_id) {
                                let val = feed.pool.resolve(to_trip_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    TRANSFERS_FILE,
                                    "to_trip_id",
                                    TRIPS_FILE,
                                    "trip_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if fare_rules_ok {
            if let Some(fare_rules) = &feed.fare_rules {
                for (index, rule) in fare_rules.rows.iter().enumerate() {
                    let row_number = fare_rules.row_number(index);
                    if fare_attributes_ok {
                        let fare_id = rule.fare_id;
                        if !fare_ids.contains(&fare_id) {
                            let val = feed.pool.resolve(fare_id);
                            notices.push(missing_ref_notice(
                                CODE_FOREIGN_KEY_VIOLATION,
                                FARE_RULES_FILE,
                                "fare_id",
                                FARE_ATTRIBUTES_FILE,
                                "fare_id",
                                val.as_str(),
                                row_number,
                            ));
                        }
                    }
                    if routes_ok {
                        if let Some(route_id) = rule.route_id.filter(|id| id.0 != 0) {
                            if !route_ids.contains(&route_id) {
                                let val = feed.pool.resolve(route_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_RULES_FILE,
                                    "route_id",
                                    ROUTES_FILE,
                                    "route_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if stops_ok {
                        if let Some(origin_id) = rule.origin_id.filter(|id| id.0 != 0) {
                            if !zone_ids.contains(&origin_id) {
                                let val = feed.pool.resolve(origin_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_RULES_FILE,
                                    "origin_id",
                                    STOPS_FILE,
                                    "zone_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(destination_id) = rule.destination_id.filter(|id| id.0 != 0) {
                            if !zone_ids.contains(&destination_id) {
                                let val = feed.pool.resolve(destination_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_RULES_FILE,
                                    "destination_id",
                                    STOPS_FILE,
                                    "zone_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                        if let Some(contains_id) = rule.contains_id.filter(|id| id.0 != 0) {
                            if !zone_ids.contains(&contains_id) {
                                let val = feed.pool.resolve(contains_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_RULES_FILE,
                                    "contains_id",
                                    STOPS_FILE,
                                    "zone_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        if fare_attributes_ok {
            if let Some(fare_attrs) = &feed.fare_attributes {
                for (index, attr) in fare_attrs.rows.iter().enumerate() {
                    let row_number = fare_attrs.row_number(index);
                    if agency_ok {
                        if let Some(agency_id) = attr.agency_id.filter(|id| id.0 != 0) {
                            if !agency_ids.contains(&agency_id) {
                                let val = feed.pool.resolve(agency_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    FARE_ATTRIBUTES_FILE,
                                    "agency_id",
                                    AGENCY_FILE,
                                    "agency_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }

        // frequencies.txt → trip_id
        if frequencies_ok && trips_ok {
            if let Some(frequencies) = &feed.frequencies {
                for (index, freq) in frequencies.rows.iter().enumerate() {
                    let row_number = frequencies.row_number(index);
                    let trip_id = freq.trip_id;
                    if trip_id.0 != 0 && !trip_ids.contains(&trip_id) {
                        let val = feed.pool.resolve(trip_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FREQUENCIES_FILE,
                            "trip_id",
                            TRIPS_FILE,
                            "trip_id",
                            val.as_str(),
                            row_number,
                        ));
                    }
                }
            }
        }

        // pathways.txt → from_stop_id, to_stop_id
        if pathways_ok && stops_ok {
            if let Some(pathways) = &feed.pathways {
                for (index, pathway) in pathways.rows.iter().enumerate() {
                    let row_number = pathways.row_number(index);
                    let from_stop_id = pathway.from_stop_id;
                    if from_stop_id.0 != 0 && !stop_ids.contains(&from_stop_id) {
                        let val = feed.pool.resolve(from_stop_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            PATHWAYS_FILE,
                            "from_stop_id",
                            STOPS_FILE,
                            "stop_id",
                            val.as_str(),
                            row_number,
                        ));
                    }
                    let to_stop_id = pathway.to_stop_id;
                    if to_stop_id.0 != 0 && !stop_ids.contains(&to_stop_id) {
                        let val = feed.pool.resolve(to_stop_id);
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            PATHWAYS_FILE,
                            "to_stop_id",
                            STOPS_FILE,
                            "stop_id",
                            val.as_str(),
                            row_number,
                        ));
                    }
                }
            }
        }

        // attributions.txt → agency_id, route_id, trip_id
        if attributions_ok {
            if let Some(attributions) = &feed.attributions {
                for (index, attr) in attributions.rows.iter().enumerate() {
                    let row_number = attributions.row_number(index);
                    if agency_ok {
                        if let Some(agency_id) = attr.agency_id.filter(|id| id.0 != 0) {
                            if !agency_ids.contains(&agency_id) {
                                let val = feed.pool.resolve(agency_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    ATTRIBUTIONS_FILE,
                                    "agency_id",
                                    AGENCY_FILE,
                                    "agency_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if routes_ok {
                        if let Some(route_id) = attr.route_id.filter(|id| id.0 != 0) {
                            if !route_ids.contains(&route_id) {
                                let val = feed.pool.resolve(route_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    ATTRIBUTIONS_FILE,
                                    "route_id",
                                    ROUTES_FILE,
                                    "route_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                    if trips_ok {
                        if let Some(trip_id) = attr.trip_id.filter(|id| id.0 != 0) {
                            if !trip_ids.contains(&trip_id) {
                                let val = feed.pool.resolve(trip_id);
                                notices.push(missing_ref_notice(
                                    CODE_FOREIGN_KEY_VIOLATION,
                                    ATTRIBUTIONS_FILE,
                                    "trip_id",
                                    TRIPS_FILE,
                                    "trip_id",
                                    val.as_str(),
                                    row_number,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn missing_ref_notice(
    code: &str,
    child_file: &str,
    child_field: &str,
    parent_file: &str,
    parent_field: &str,
    id: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        code,
        NoticeSeverity::Error,
        format!("missing referenced id {}", id),
    );
    notice.row = Some(row_number);
    notice.field_order = vec![
        "childFieldName".into(),
        "childFilename".into(),
        "csvRowNumber".into(),
        "fieldValue".into(),
        "parentFieldName".into(),
        "parentFilename".into(),
    ];
    notice.insert_context_field("childFieldName", child_field);
    notice.insert_context_field("childFilename", child_file);
    notice.insert_context_field("parentFieldName", parent_field);
    notice.insert_context_field("parentFilename", parent_file);
    notice.insert_context_field("fieldValue", id);
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Route, RouteType, Stop, StopTime, Trip};

    #[test]
    fn detects_missing_trip_id_in_stop_times() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("NONEXISTENT"),
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ReferentialIntegrityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_FOREIGN_KEY_VIOLATION);
        assert_eq!(
            notice
                .context
                .get("childFieldName")
                .unwrap()
                .as_str()
                .unwrap(),
            "trip_id"
        );
        assert_eq!(
            notice.context.get("fieldValue").unwrap().as_str().unwrap(),
            "NONEXISTENT"
        );
    }

    #[test]
    fn detects_missing_stop_id_in_stop_times() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_id: feed.pool.intern("NONEXISTENT"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ReferentialIntegrityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_FOREIGN_KEY_VIOLATION);
        assert_eq!(
            notice
                .context
                .get("childFieldName")
                .unwrap()
                .as_str()
                .unwrap(),
            "stop_id"
        );
    }

    #[test]
    fn detects_missing_route_id_in_trips() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("NONEXISTENT"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ReferentialIntegrityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_FOREIGN_KEY_VIOLATION);
        assert_eq!(
            notice
                .context
                .get("childFieldName")
                .unwrap()
                .as_str()
                .unwrap(),
            "route_id"
        );
    }

    #[test]
    fn passes_with_valid_references() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: feed.pool.intern("T1"),
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ReferentialIntegrityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
