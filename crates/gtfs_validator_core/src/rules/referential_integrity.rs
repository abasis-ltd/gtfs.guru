use std::collections::HashSet;

use crate::{
    feed::{
        AREAS_FILE, BOOKING_RULES_FILE, FARE_LEG_JOIN_RULES_FILE, FARE_LEG_RULES_FILE,
        FARE_MEDIA_FILE, FARE_PRODUCTS_FILE, FARE_TRANSFER_RULES_FILE, LOCATION_GROUPS_FILE,
        LOCATION_GROUP_STOPS_FILE, NETWORKS_FILE, RIDER_CATEGORIES_FILE, ROUTES_FILE,
        ROUTE_NETWORKS_FILE, STOPS_FILE, STOP_AREAS_FILE, STOP_TIMES_FILE, TIMEFRAMES_FILE,
        TRIPS_FILE,
    },
    GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator,
};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

#[derive(Debug, Default)]
pub struct ReferentialIntegrityValidator;

impl Validator for ReferentialIntegrityValidator {
    fn name(&self) -> &'static str {
        "referential_integrity"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let stop_ids: HashSet<&str> = feed
            .stops
            .rows
            .iter()
            .map(|stop| stop.stop_id.trim())
            .filter(|value| !value.is_empty())
            .collect();
        let trip_ids: HashSet<&str> = feed
            .trips
            .rows
            .iter()
            .map(|trip| trip.trip_id.trim())
            .filter(|value| !value.is_empty())
            .collect();
        let route_ids: HashSet<&str> = feed
            .routes
            .rows
            .iter()
            .map(|route| route.route_id.trim())
            .filter(|value| !value.is_empty())
            .collect();
        let network_ids: HashSet<&str> = feed
            .networks
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|network| network.network_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let area_ids: HashSet<&str> = feed
            .areas
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|area| area.area_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let fare_media_ids: HashSet<&str> = feed
            .fare_media
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|fare_media| fare_media.fare_media_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let fare_product_ids: HashSet<&str> = feed
            .fare_products
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|fare_product| fare_product.fare_product_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let rider_category_ids: HashSet<&str> = feed
            .rider_categories
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|category| category.rider_category_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let timeframe_group_ids: HashSet<&str> = feed
            .timeframes
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .filter_map(|timeframe| {
                        timeframe
                            .timeframe_group_id
                            .as_deref()
                            .map(|value| value.trim())
                            .filter(|value| !value.is_empty())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fare_leg_group_ids: HashSet<&str> = feed
            .fare_leg_rules
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .filter_map(|rule| {
                        rule.leg_group_id
                            .as_deref()
                            .map(|value| value.trim())
                            .filter(|value| !value.is_empty())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let location_group_ids: HashSet<&str> = feed
            .location_groups
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|group| group.location_group_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let booking_rule_ids: HashSet<&str> = feed
            .booking_rules
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .map(|rule| rule.booking_rule_id.trim())
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let has_booking_rules = feed.booking_rules.is_some();

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if !trip_id.is_empty() && !trip_ids.contains(trip_id) {
                notices.push(missing_ref_notice(
                    CODE_FOREIGN_KEY_VIOLATION,
                    STOP_TIMES_FILE,
                    "trip_id",
                    TRIPS_FILE,
                    "trip_id",
                    trip_id,
                    row_number,
                ));
            }
            let stop_id = stop_time.stop_id.trim();
            if !stop_id.is_empty() && !stop_ids.contains(stop_id) {
                notices.push(missing_ref_notice(
                    CODE_FOREIGN_KEY_VIOLATION,
                    STOP_TIMES_FILE,
                    "stop_id",
                    STOPS_FILE,
                    "stop_id",
                    stop_id,
                    row_number,
                ));
            }
            if let Some(group_id) = stop_time
                .location_group_id
                .as_deref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                if !location_group_ids.contains(group_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        STOP_TIMES_FILE,
                        "location_group_id",
                        LOCATION_GROUPS_FILE,
                        "location_group_id",
                        group_id,
                        row_number,
                    ));
                }
            }
            if has_booking_rules {
                if let Some(booking_rule_id) = stop_time
                    .pickup_booking_rule_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !booking_rule_ids.contains(booking_rule_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            STOP_TIMES_FILE,
                            "pickup_booking_rule_id",
                            BOOKING_RULES_FILE,
                            "booking_rule_id",
                            booking_rule_id,
                            row_number,
                        ));
                    }
                }
                if let Some(booking_rule_id) = stop_time
                    .drop_off_booking_rule_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !booking_rule_ids.contains(booking_rule_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            STOP_TIMES_FILE,
                            "drop_off_booking_rule_id",
                            BOOKING_RULES_FILE,
                            "booking_rule_id",
                            booking_rule_id,
                            row_number,
                        ));
                    }
                }
            }
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let route_id = trip.route_id.trim();
            if !route_id.is_empty() && !route_ids.contains(route_id) {
                notices.push(missing_ref_notice(
                    CODE_FOREIGN_KEY_VIOLATION,
                    TRIPS_FILE,
                    "route_id",
                    ROUTES_FILE,
                    "route_id",
                    route_id,
                    row_number,
                ));
            }
        }

        if let Some(fare_products) = &feed.fare_products {
            for (index, product) in fare_products.rows.iter().enumerate() {
                let row_number = fare_products.row_number(index);
                if let Some(fare_media_id) = product
                    .fare_media_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !fare_media_ids.contains(fare_media_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_PRODUCTS_FILE,
                            "fare_media_id",
                            FARE_MEDIA_FILE,
                            "fare_media_id",
                            fare_media_id,
                            row_number,
                        ));
                    }
                }
                if let Some(rider_category_id) = product
                    .rider_category_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !rider_category_ids.contains(rider_category_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_PRODUCTS_FILE,
                            "rider_category_id",
                            RIDER_CATEGORIES_FILE,
                            "rider_category_id",
                            rider_category_id,
                            row_number,
                        ));
                    }
                }
            }
        }

        if let Some(fare_leg_rules) = &feed.fare_leg_rules {
            for (index, rule) in fare_leg_rules.rows.iter().enumerate() {
                let row_number = fare_leg_rules.row_number(index);
                let fare_product_id = rule.fare_product_id.trim();
                if !fare_product_ids.contains(fare_product_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        FARE_LEG_RULES_FILE,
                        "fare_product_id",
                        FARE_PRODUCTS_FILE,
                        "fare_product_id",
                        fare_product_id,
                        row_number,
                    ));
                }
                if let Some(area_id) = rule
                    .from_area_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !area_ids.contains(area_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_RULES_FILE,
                            "from_area_id",
                            AREAS_FILE,
                            "area_id",
                            area_id,
                            row_number,
                        ));
                    }
                }
                if let Some(area_id) = rule
                    .to_area_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !area_ids.contains(area_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_RULES_FILE,
                            "to_area_id",
                            AREAS_FILE,
                            "area_id",
                            area_id,
                            row_number,
                        ));
                    }
                }
                if let Some(timeframe_id) = rule
                    .from_timeframe_group_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !timeframe_group_ids.contains(timeframe_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_RULES_FILE,
                            "from_timeframe_group_id",
                            TIMEFRAMES_FILE,
                            "timeframe_group_id",
                            timeframe_id,
                            row_number,
                        ));
                    }
                }
                if let Some(timeframe_id) = rule
                    .to_timeframe_group_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !timeframe_group_ids.contains(timeframe_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_RULES_FILE,
                            "to_timeframe_group_id",
                            TIMEFRAMES_FILE,
                            "timeframe_group_id",
                            timeframe_id,
                            row_number,
                        ));
                    }
                }
            }
        }

        if let Some(fare_transfer_rules) = &feed.fare_transfer_rules {
            for (index, rule) in fare_transfer_rules.rows.iter().enumerate() {
                let row_number = fare_transfer_rules.row_number(index);
                if let Some(group_id) = rule
                    .from_leg_group_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !fare_leg_group_ids.contains(group_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_TRANSFER_RULES_FILE,
                            "from_leg_group_id",
                            FARE_LEG_RULES_FILE,
                            "leg_group_id",
                            group_id,
                            row_number,
                        ));
                    }
                }
                if let Some(group_id) = rule
                    .to_leg_group_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !fare_leg_group_ids.contains(group_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_TRANSFER_RULES_FILE,
                            "to_leg_group_id",
                            FARE_LEG_RULES_FILE,
                            "leg_group_id",
                            group_id,
                            row_number,
                        ));
                    }
                }
                if let Some(fare_product_id) = rule
                    .fare_product_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !fare_product_ids.contains(fare_product_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_TRANSFER_RULES_FILE,
                            "fare_product_id",
                            FARE_PRODUCTS_FILE,
                            "fare_product_id",
                            fare_product_id,
                            row_number,
                        ));
                    }
                }
            }
        }

        if let Some(stop_areas) = &feed.stop_areas {
            for (index, row) in stop_areas.rows.iter().enumerate() {
                let row_number = stop_areas.row_number(index);
                let area_id = row.area_id.trim();
                if !area_ids.contains(area_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        STOP_AREAS_FILE,
                        "area_id",
                        AREAS_FILE,
                        "area_id",
                        area_id,
                        row_number,
                    ));
                }
                let stop_id = row.stop_id.trim();
                if !stop_ids.contains(stop_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        STOP_AREAS_FILE,
                        "stop_id",
                        STOPS_FILE,
                        "stop_id",
                        stop_id,
                        row_number,
                    ));
                }
            }
        }

        if let Some(fare_leg_join_rules) = &feed.fare_leg_join_rules {
            for (index, row) in fare_leg_join_rules.rows.iter().enumerate() {
                let row_number = fare_leg_join_rules.row_number(index);
                if let Some(stop_id) = row
                    .from_stop_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !stop_ids.contains(stop_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_JOIN_RULES_FILE,
                            "from_stop_id",
                            STOPS_FILE,
                            "stop_id",
                            stop_id,
                            row_number,
                        ));
                    }
                }
                if let Some(stop_id) = row
                    .to_stop_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !stop_ids.contains(stop_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_JOIN_RULES_FILE,
                            "to_stop_id",
                            STOPS_FILE,
                            "stop_id",
                            stop_id,
                            row_number,
                        ));
                    }
                }
                if let Some(area_id) = row
                    .from_area_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !area_ids.contains(area_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_JOIN_RULES_FILE,
                            "from_area_id",
                            AREAS_FILE,
                            "area_id",
                            area_id,
                            row_number,
                        ));
                    }
                }
                if let Some(area_id) = row
                    .to_area_id
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    if !area_ids.contains(area_id) {
                        notices.push(missing_ref_notice(
                            CODE_FOREIGN_KEY_VIOLATION,
                            FARE_LEG_JOIN_RULES_FILE,
                            "to_area_id",
                            AREAS_FILE,
                            "area_id",
                            area_id,
                            row_number,
                        ));
                    }
                }
            }
        }

        if let Some(route_networks) = &feed.route_networks {
            for (index, row) in route_networks.rows.iter().enumerate() {
                let row_number = route_networks.row_number(index);
                let route_id = row.route_id.trim();
                if !route_ids.contains(route_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        ROUTE_NETWORKS_FILE,
                        "route_id",
                        ROUTES_FILE,
                        "route_id",
                        route_id,
                        row_number,
                    ));
                }
                let network_id = row.network_id.trim();
                if !network_ids.is_empty() && !network_ids.contains(network_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        ROUTE_NETWORKS_FILE,
                        "network_id",
                        NETWORKS_FILE,
                        "network_id",
                        network_id,
                        row_number,
                    ));
                }
            }
        }

        if let Some(location_group_stops) = &feed.location_group_stops {
            for (index, row) in location_group_stops.rows.iter().enumerate() {
                let row_number = location_group_stops.row_number(index);
                let location_group_id = row.location_group_id.trim();
                if !location_group_ids.contains(location_group_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        LOCATION_GROUP_STOPS_FILE,
                        "location_group_id",
                        LOCATION_GROUPS_FILE,
                        "location_group_id",
                        location_group_id,
                        row_number,
                    ));
                }
                let stop_id = row.stop_id.trim();
                if !stop_ids.contains(stop_id) {
                    notices.push(missing_ref_notice(
                        CODE_FOREIGN_KEY_VIOLATION,
                        LOCATION_GROUP_STOPS_FILE,
                        "stop_id",
                        STOPS_FILE,
                        "stop_id",
                        stop_id,
                        row_number,
                    ));
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
                trip_id: "T1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: "NONEXISTENT".into(),
                stop_id: "S1".into(),
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
                trip_id: "T1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: "T1".into(),
                stop_id: "NONEXISTENT".into(),
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
                route_id: "R1".into(),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: "T1".into(),
                route_id: "NONEXISTENT".into(),
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
                route_id: "R1".into(),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: "T1".into(),
                route_id: "R1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: "T1".into(),
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        ReferentialIntegrityValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
