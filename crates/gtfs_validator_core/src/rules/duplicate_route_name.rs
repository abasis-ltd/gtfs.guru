use compact_str::CompactString;
use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::RouteType;
use gtfs_guru_model::StringId;

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
                notice.insert_context_field("routeId1", feed.pool.resolve(prev.route_id).as_str());
                notice.insert_context_field("csvRowNumber2", entry.row_number);
                notice.insert_context_field("routeId2", feed.pool.resolve(entry.route_id).as_str());
                notice.insert_context_field("routeShortName", prev.route_short_name.as_str());
                notice.insert_context_field("routeLongName", prev.route_long_name.as_str());
                notice.insert_context_field("routeTypeValue", prev.route_type);
                notice.insert_context_field("agencyId", feed.pool.resolve(prev.agency_id).as_str());
                notice.field_order = vec![
                    "agencyId".into(),
                    "csvRowNumber1".into(),
                    "csvRowNumber2".into(),
                    "routeId1".into(),
                    "routeId2".into(),
                    "routeLongName".into(),
                    "routeShortName".into(),
                    "routeTypeValue".into(),
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
    route_short_name: CompactString,
    route_long_name: CompactString,
    route_type: i32,
    agency_id: StringId,
}

#[derive(Debug)]
struct RouteEntry {
    row_number: u64,
    route_id: StringId,
    route_short_name: CompactString,
    route_long_name: CompactString,
    route_type: i32,
    agency_id: StringId,
}

impl RouteEntry {
    fn new(route: &gtfs_guru_model::Route, row_number: u64) -> Self {
        Self {
            row_number,
            route_id: route.route_id,
            route_short_name: route
                .route_short_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .into(),
            route_long_name: route.route_long_name.as_deref().unwrap_or("").trim().into(),
            route_type: route_type_value(route.route_type),
            agency_id: route.agency_id.unwrap_or(StringId(0)),
        }
    }
}

impl RouteKey {
    fn new(route: &gtfs_guru_model::Route) -> Self {
        Self {
            route_short_name: route
                .route_short_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .into(),
            route_long_name: route.route_long_name.as_deref().unwrap_or("").trim().into(),
            route_type: route_type_value(route.route_type),
            agency_id: route.agency_id.unwrap_or(StringId(0)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn test_duplicate_route_name() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![],
            rows: vec![
                gtfs_guru_model::Route {
                    route_id: feed.pool.intern("R1"),
                    route_short_name: Some("1".into()),
                    route_long_name: Some("Route One".into()),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
                gtfs_guru_model::Route {
                    route_id: feed.pool.intern("R2"),
                    route_short_name: Some("1".into()),
                    route_long_name: Some("Route One".into()),
                    route_type: RouteType::Bus,
                    ..Default::default()
                },
            ],
            row_numbers: vec![1, 2],
        };

        let mut notices = NoticeContainer::new();
        DuplicateRouteNameValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "duplicate_route_name");
    }
}
