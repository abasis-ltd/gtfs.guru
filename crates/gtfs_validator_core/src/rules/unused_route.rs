use std::collections::HashSet;

use crate::feed::ROUTES_FILE;
use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_ROUTE: &str = "unused_route";

#[derive(Debug, Default)]
pub struct UnusedRouteValidator;

impl Validator for UnusedRouteValidator {
    fn name(&self) -> &'static str {
        "unused_route"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Only run in thorough mode to match Java default behavior
        if !thorough_mode_enabled() {
            return;
        }

        let mut used_route_ids: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        for trip in &feed.trips.rows {
            let route_id = trip.route_id;
            if route_id.0 != 0 {
                used_route_ids.insert(route_id);
            }
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let route_id = route.route_id;
            if route_id.0 == 0 {
                continue;
            }

            if !used_route_ids.contains(&route_id) {
                let route_id_value = feed.pool.resolve(route_id);
                let mut notice = ValidationNotice::new(
                    CODE_UNUSED_ROUTE,
                    NoticeSeverity::Warning,
                    "route has no trips",
                );
                notice.file = Some(ROUTES_FILE.to_string());
                notice.insert_context_field("csvRowNumber", feed.routes.row_number(index));
                notice.insert_context_field("routeId", route_id_value.as_str());
                notice.insert_context_field(
                    "routeShortName",
                    route.route_short_name.as_deref().unwrap_or(""),
                );
                notice.insert_context_field(
                    "routeLongName",
                    route.route_long_name.as_deref().unwrap_or(""),
                );
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "routeId".into(),
                    "routeShortName".into(),
                    "routeLongName".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Route, Trip};

    #[test]
    fn detects_unused_route() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_short_name".into()],
            rows: vec![
                Route {
                    route_id: feed.pool.intern("R1"),
                    route_short_name: Some("Route 1".into()),
                    ..Default::default()
                },
                Route {
                    route_id: feed.pool.intern("R2"),
                    route_short_name: Some("Route 2".into()),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
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

        let mut notices = NoticeContainer::new();
        UnusedRouteValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_UNUSED_ROUTE)
                .count(),
            1
        );
        let notice = notices
            .iter()
            .find(|n| n.code == CODE_UNUSED_ROUTE)
            .unwrap();
        assert_eq!(
            notice.context.get("routeId").unwrap().as_str().unwrap(),
            "R2"
        );
    }

    #[test]
    fn passes_when_all_routes_used() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_short_name".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_short_name: Some("Route 1".into()),
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

        let mut notices = NoticeContainer::new();
        UnusedRouteValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
