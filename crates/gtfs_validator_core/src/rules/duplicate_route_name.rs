use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::RouteType;

const CODE_DUPLICATE_ROUTE_NAME: &str = "duplicate_route_name";

#[derive(Debug, Default)]
pub struct DuplicateRouteNameValidator;

impl Validator for DuplicateRouteNameValidator {
    fn name(&self) -> &'static str {
        "duplicate_route_name"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut seen: HashMap<RouteKey, RouteEntry> = HashMap::new();
        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            let key = RouteKey::new(route);
            let entry = RouteEntry::new(route, row_number);
            if let Some(prev) = seen.get(&key) {
                let mut notice = ValidationNotice::new(
                    CODE_DUPLICATE_ROUTE_NAME,
                    NoticeSeverity::Warning,
                    "duplicate route_short_name/route_long_name for same agency and route_type",
                );
                notice.insert_context_field("csvRowNumber1", prev.row_number);
                notice.insert_context_field("routeId1", prev.route_id.as_str());
                notice.insert_context_field("csvRowNumber2", entry.row_number);
                notice.insert_context_field("routeId2", entry.route_id.as_str());
                notice.insert_context_field("routeShortName", prev.route_short_name.as_str());
                notice.insert_context_field("routeLongName", prev.route_long_name.as_str());
                notice.insert_context_field("routeTypeValue", prev.route_type);
                notice.insert_context_field("agencyId", prev.agency_id.as_str());
                notice.field_order = vec![
                    "agencyId".to_string(),
                    "csvRowNumber1".to_string(),
                    "csvRowNumber2".to_string(),
                    "routeId1".to_string(),
                    "routeId2".to_string(),
                    "routeLongName".to_string(),
                    "routeShortName".to_string(),
                    "routeTypeValue".to_string(),
                ];
                notices.push(notice);
            } else {
                seen.insert(key, entry);
            }
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct RouteKey {
    route_short_name: String,
    route_long_name: String,
    route_type: i32,
    agency_id: String,
}

#[derive(Debug)]
struct RouteEntry {
    row_number: u64,
    route_id: String,
    route_short_name: String,
    route_long_name: String,
    route_type: i32,
    agency_id: String,
}

impl RouteEntry {
    fn new(route: &gtfs_model::Route, row_number: u64) -> Self {
        Self {
            row_number,
            route_id: route.route_id.clone(),
            route_short_name: route
                .route_short_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            route_long_name: route
                .route_long_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            route_type: route_type_value(route.route_type),
            agency_id: route.agency_id.as_deref().unwrap_or("").trim().to_string(),
        }
    }
}

impl RouteKey {
    fn new(route: &gtfs_model::Route) -> Self {
        Self {
            route_short_name: route
                .route_short_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            route_long_name: route
                .route_long_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            route_type: route_type_value(route.route_type),
            agency_id: route.agency_id.as_deref().unwrap_or("").trim().to_string(),
        }
    }
}

fn route_type_value(route_type: RouteType) -> i32 {
    match route_type {
        RouteType::Tram => 0,
        RouteType::Subway => 1,
        RouteType::Rail => 2,
        RouteType::Bus => 3,
        RouteType::Ferry => 4,
        RouteType::CableCar => 5,
        RouteType::Gondola => 6,
        RouteType::Funicular => 7,
        RouteType::Trolleybus => 11,
        RouteType::Monorail => 12,
        RouteType::Extended(value) => value as i32,
        RouteType::Unknown => -1,
    }
}

