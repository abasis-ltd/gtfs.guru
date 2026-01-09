use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{BikesAllowed, RouteType};

const CODE_MISSING_BIKE_ALLOWANCE: &str = "missing_bike_allowance";

#[derive(Debug, Default)]
pub struct BikesAllowanceValidator;

impl Validator for BikesAllowanceValidator {
    fn name(&self) -> &'static str {
        "bikes_allowance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let route_types: HashMap<gtfs_guru_model::StringId, RouteType> = feed
            .routes
            .rows
            .iter()
            .map(|r| (r.route_id, r.route_type))
            .collect();

        let has_bikes_allowed_column = feed.trips.headers.iter().any(|h| h == "bikes_allowed");

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let route_id = trip.route_id;

            // Java only checks ferry routes.
            let Some(&route_type) = route_types.get(&route_id) else {
                continue;
            };
            if route_type != RouteType::Ferry {
                continue;
            }

            if has_bikes_allowed_column && has_bike_allowance(trip.bikes_allowed) {
                continue;
            }

            let trip_id = trip.trip_id;
            let route_id_value = feed.pool.resolve(route_id);
            let trip_id_value = feed.pool.resolve(trip_id);
            let mut notice = ValidationNotice::new(
                CODE_MISSING_BIKE_ALLOWANCE,
                NoticeSeverity::Warning,
                "trips should define bikes_allowed",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("routeId", route_id_value.as_str());
            notice.insert_context_field("tripId", trip_id_value.as_str());
            notice.field_order = vec!["csvRowNumber".into(), "routeId".into(), "tripId".into()];
            notices.push(notice);
        }
    }
}

fn has_bike_allowance(value: Option<BikesAllowed>) -> bool {
    matches!(
        value,
        Some(BikesAllowed::Allowed | BikesAllowed::NotAllowed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn emits_notice_for_missing_bike_allowance() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let feed = base_feed(RouteType::Ferry, None);

        let mut notices = NoticeContainer::new();
        BikesAllowanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_MISSING_BIKE_ALLOWANCE);
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "routeId"), "R1");
        assert_eq!(context_str(notice, "tripId"), "T1");
    }

    #[test]
    fn passes_when_bike_allowance_present() {
        let feed = base_feed(RouteType::Ferry, Some(BikesAllowed::Allowed));

        let mut notices = NoticeContainer::new();
        BikesAllowanceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn ignores_non_ferry_routes() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        // Create a feed without the bikes_allowed header - non-ferry routes are ignored.
        let mut feed = base_feed(RouteType::Bus, None);
        feed.trips.headers = vec![]; // Remove bikes_allowed header

        let mut notices = NoticeContainer::new();
        BikesAllowanceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn base_feed(route_type: RouteType, bikes_allowed: Option<BikesAllowed>) -> GtfsFeed {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Agency {
                agency_id: None,
                agency_name: "Agency".into(),
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                agency_lang: None,
                agency_phone: None,
                agency_fare_url: None,
                agency_email: None,
            }],
            row_numbers: Vec::new(),
        };
        feed.stops = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Stop {
                stop_id: feed.pool.intern("STOP1"),
                stop_name: Some("Stop".into()),
                stop_lat: Some(10.0),
                stop_lon: Some(20.0),
                ..Default::default()
            }],
            row_numbers: Vec::new(),
        };
        feed.routes = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Route {
                route_id: feed.pool.intern("R1"),
                route_short_name: Some("R1".into()),
                route_type,
                ..Default::default()
            }],
            row_numbers: Vec::new(),
        };
        feed.trips = CsvTable {
            headers: vec!["bikes_allowed".into()],
            rows: vec![gtfs_guru_model::Trip {
                route_id: feed.pool.intern("R1"),
                service_id: feed.pool.intern("SVC1"),
                trip_id: feed.pool.intern("T1"),
                bikes_allowed,
                ..Default::default()
            }],
            row_numbers: Vec::new(),
        };
        feed.stop_times = CsvTable::default();
        feed
    }

    fn context_str<'a>(notice: &'a ValidationNotice, key: &str) -> &'a str {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("")
    }

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }
}
