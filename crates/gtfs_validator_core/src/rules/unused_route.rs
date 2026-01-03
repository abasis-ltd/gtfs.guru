use std::collections::HashSet;

use crate::feed::ROUTES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_ROUTE: &str = "unused_route";

#[derive(Debug, Default)]
pub struct UnusedRouteValidator;

impl Validator for UnusedRouteValidator {
    fn name(&self) -> &'static str {
        "unused_route"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut used_route_ids: HashSet<&str> = HashSet::new();
        for trip in &feed.trips.rows {
            let route_id = trip.route_id.trim();
            if !route_id.is_empty() {
                used_route_ids.insert(route_id);
            }
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let route_id = route.route_id.trim();
            if route_id.is_empty() {
                continue;
            }

            if !used_route_ids.contains(route_id) {
                let mut notice = ValidationNotice::new(
                    CODE_UNUSED_ROUTE,
                    NoticeSeverity::Warning,
                    "route has no trips",
                );
                notice.file = Some(ROUTES_FILE.to_string());
                notice.insert_context_field("csvRowNumber", feed.routes.row_number(index));
                notice.insert_context_field("routeId", route_id);
                notice.insert_context_field("routeShortName", route.route_short_name.as_deref().unwrap_or(""));
                notice.insert_context_field("routeLongName", route.route_long_name.as_deref().unwrap_or(""));
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "routeId".to_string(),
                    "routeShortName".to_string(),
                    "routeLongName".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

