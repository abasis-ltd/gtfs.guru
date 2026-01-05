use std::collections::HashMap;

use crate::feed::{
    AGENCY_FILE, AREAS_FILE, BOOKING_RULES_FILE, FARE_ATTRIBUTES_FILE, FARE_MEDIA_FILE,
    FARE_PRODUCTS_FILE, LEVELS_FILE, LOCATION_GROUPS_FILE, NETWORKS_FILE, PATHWAYS_FILE,
    RIDER_CATEGORIES_FILE, ROUTES_FILE, STOPS_FILE, TRIPS_FILE,
};
use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in feed.agency.rows.iter().enumerate() {
                let row_number = feed.agency.row_number(index);
                if let Some(ref agency_id) = row.agency_id {
                    let id = agency_id.trim();
                    if !id.is_empty() {
                        if let Some(prev_row) = seen.get(id) {
                            notices.push(duplicate_key_notice(
                                AGENCY_FILE,
                                row_number,
                                "agency_id",
                                id,
                                *prev_row,
                            ));
                        } else {
                            seen.insert(id, row_number);
                        }
                    }
                }
            }
        }

        // Stops: stop_id
        {
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in feed.stops.rows.iter().enumerate() {
                let row_number = feed.stops.row_number(index);
                let id = row.stop_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            STOPS_FILE, row_number, "stop_id", id, *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Routes: route_id
        {
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in feed.routes.rows.iter().enumerate() {
                let row_number = feed.routes.row_number(index);
                let id = row.route_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            ROUTES_FILE,
                            row_number,
                            "route_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in feed.trips.rows.iter().enumerate() {
                let row_number = feed.trips.row_number(index);
                let id = row.trip_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            TRIPS_FILE, row_number, "trip_id", id, *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Fare attributes: fare_id
        if let Some(ref fare_attributes) = feed.fare_attributes {
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in fare_attributes.rows.iter().enumerate() {
                let row_number = fare_attributes.row_number(index);
                let id = row.fare_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            FARE_ATTRIBUTES_FILE,
                            row_number,
                            "fare_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in fare_media.rows.iter().enumerate() {
                let row_number = fare_media.row_number(index);
                let id = row.fare_media_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            FARE_MEDIA_FILE,
                            row_number,
                            "fare_media_id",
                            id,
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Fare products: fare_product_id
        if let Some(ref fare_products) = feed.fare_products {
            let mut seen: HashMap<String, u64> = HashMap::new();
            for (index, row) in fare_products.rows.iter().enumerate() {
                let row_number = fare_products.row_number(index);
                let id = row.fare_product_id.trim();
                let media_id = row.fare_media_id.as_deref().unwrap_or("").trim();

                if !id.is_empty() {
                    // In strict mode (thorough), fare_product_id must be unique globally.
                    // In compatibility mode (Java), it seems they might allow duplicates if fare_media_id differs
                    // or simply don't enforce strictly. MBTA feed has duplicates with different media.
                    // We will use composite key in non-thorough mode if media_id is present.
                    let key = if !thorough_mode_enabled() && !media_id.is_empty() {
                        format!("{}:{}", id, media_id)
                    } else {
                        id.to_string()
                    };

                    if let Some(prev_row) = seen.get(&key) {
                        notices.push(duplicate_key_notice(
                            FARE_PRODUCTS_FILE,
                            row_number,
                            "fare_product_id",
                            id,
                            *prev_row,
                        ));
                    } else {
                        seen.insert(key, row_number);
                    }
                }
            }
        }

        // Areas: area_id
        if let Some(ref areas) = feed.areas {
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in areas.rows.iter().enumerate() {
                let row_number = areas.row_number(index);
                let id = row.area_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            AREAS_FILE, row_number, "area_id", id, *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }

        // Levels: level_id
        if let Some(ref levels) = feed.levels {
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in levels.rows.iter().enumerate() {
                let row_number = levels.row_number(index);
                let id = row.level_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            LEVELS_FILE,
                            row_number,
                            "level_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in pathways.rows.iter().enumerate() {
                let row_number = pathways.row_number(index);
                let id = row.pathway_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            PATHWAYS_FILE,
                            row_number,
                            "pathway_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in location_groups.rows.iter().enumerate() {
                let row_number = location_groups.row_number(index);
                let id = row.location_group_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            LOCATION_GROUPS_FILE,
                            row_number,
                            "location_group_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in booking_rules.rows.iter().enumerate() {
                let row_number = booking_rules.row_number(index);
                let id = row.booking_rule_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            BOOKING_RULES_FILE,
                            row_number,
                            "booking_rule_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in networks.rows.iter().enumerate() {
                let row_number = networks.row_number(index);
                let id = row.network_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            NETWORKS_FILE,
                            row_number,
                            "network_id",
                            id,
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
            let mut seen: HashMap<&str, u64> = HashMap::new();
            for (index, row) in rider_categories.rows.iter().enumerate() {
                let row_number = rider_categories.row_number(index);
                let id = row.rider_category_id.trim();
                if !id.is_empty() {
                    if let Some(prev_row) = seen.get(id) {
                        notices.push(duplicate_key_notice(
                            RIDER_CATEGORIES_FILE,
                            row_number,
                            "rider_category_id",
                            id,
                            *prev_row,
                        ));
                    } else {
                        seen.insert(id, row_number);
                    }
                }
            }
        }
    }
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
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("fieldValue", field_value);
    notice.insert_context_field("filename", filename);
    notice.insert_context_field("prevCsvRowNumber", prev_row_number);
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
        "prevCsvRowNumber".to_string(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Route, RouteType, Stop, Trip};

    #[test]
    fn detects_duplicate_stop_id() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S1".to_string(),
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
            notice.context.get("fieldName").unwrap().as_str().unwrap(),
            "stop_id"
        );
        assert_eq!(
            notice.context.get("fieldValue").unwrap().as_str().unwrap(),
            "S1"
        );
        assert_eq!(
            notice
                .context
                .get("csvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            3
        );
        assert_eq!(
            notice
                .context
                .get("prevCsvRowNumber")
                .unwrap()
                .as_u64()
                .unwrap(),
            2
        );
    }

    #[test]
    fn detects_duplicate_route_id() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string(), "route_type".to_string()],
            rows: vec![
                Route {
                    route_id: "R1".to_string(),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
                Route {
                    route_id: "R1".to_string(),
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
            notice.context.get("fieldName").unwrap().as_str().unwrap(),
            "route_id"
        );
    }

    #[test]
    fn detects_duplicate_trip_id() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string()],
            rows: vec![
                Trip {
                    trip_id: "T1".to_string(),
                    ..Default::default()
                },
                Trip {
                    trip_id: "T1".to_string(),
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
            notice.context.get("fieldName").unwrap().as_str().unwrap(),
            "trip_id"
        );
    }

    #[test]
    fn passes_with_unique_ids() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string()],
            rows: vec![
                Route {
                    route_id: "R1".to_string(),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
                Route {
                    route_id: "R2".to_string(),
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
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "".to_string(),
                    ..Default::default()
                },
                Stop {
                    stop_id: "".to_string(),
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
