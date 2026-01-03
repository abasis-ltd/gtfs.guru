use std::collections::HashSet;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::{BikesAllowed, RouteType};

const CODE_MISSING_BIKE_ALLOWANCE: &str = "missing_bike_allowance";

#[derive(Debug, Default)]
pub struct BikesAllowanceValidator;

impl Validator for BikesAllowanceValidator {
    fn name(&self) -> &'static str {
        "bikes_allowance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let has_bikes_allowed_column = feed.trips.headers.iter().any(|h| h == "bikes_allowed");
        if !has_bikes_allowed_column {
            return;
        }

        let ferry_routes: HashSet<&str> = feed
            .routes
            .rows
            .iter()
            .filter(|route| route.route_type == RouteType::Ferry)
            .map(|route| route.route_id.trim())
            .filter(|value| !value.is_empty())
            .collect();

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let route_id = trip.route_id.trim();
            let trip_id = trip.trip_id.trim();
            let is_ferry = ferry_routes.contains(route_id);
            if !is_ferry && !route_id.is_empty() {
                // For non-ferries, we only warn if the column is present but empty.
                // Weight it less? No, the code is the same.
            }
            if has_bike_allowance(trip.bikes_allowed) {
                continue;
            }
            let mut notice = ValidationNotice::new(
                CODE_MISSING_BIKE_ALLOWANCE,
                NoticeSeverity::Warning,
                if ferry_routes.contains(route_id) {
                    "ferry trips should define bikes_allowed"
                } else {
                    "trip has bikes_allowed column but no value specified"
                },
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("routeId", route_id);
            notice.insert_context_field("tripId", trip_id);
            notice.field_order = vec![
                "csvRowNumber".to_string(),
                "routeId".to_string(),
                "tripId".to_string(),
            ];
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
    use gtfs_model::StopTime;

    #[test]
    fn emits_notice_for_missing_bike_allowance() {
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
    fn skips_non_ferry_routes() {
        let feed = base_feed(RouteType::Bus, None);

        let mut notices = NoticeContainer::new();
        BikesAllowanceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn base_feed(route_type: RouteType, bikes_allowed: Option<BikesAllowed>) -> GtfsFeed {
        GtfsFeed {
            agency: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_model::Agency {
                    agency_id: None,
                    agency_name: "Agency".to_string(),
                    agency_url: "https://example.com".to_string(),
                    agency_timezone: "UTC".to_string(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: Vec::new(),
            },
            stops: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_model::Stop {
                    stop_id: "STOP1".to_string(),
                    stop_name: Some("Stop".to_string()),
                    stop_lat: Some(10.0),
                    stop_lon: Some(20.0),
                    ..Default::default()
                }],
                row_numbers: Vec::new(),
            },
            routes: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_model::Route {
                    route_id: "R1".to_string(),
                    route_short_name: Some("R1".to_string()),
                    route_type,
                    ..Default::default()
                }],
                row_numbers: Vec::new(),
            },
            trips: CsvTable {
                headers: vec!["bikes_allowed".to_string()],
                rows: vec![gtfs_model::Trip {
                    route_id: "R1".to_string(),
                    service_id: "SVC1".to_string(),
                    trip_id: "T1".to_string(),
                    bikes_allowed,
                    ..Default::default()
                }],
                row_numbers: Vec::new(),
            },
            stop_times: CsvTable::default(),
            calendar: None,
            calendar_dates: None,
            fare_attributes: None,
            fare_rules: None,
            fare_media: None,
            fare_products: None,
            fare_leg_rules: None,
            fare_transfer_rules: None,
            fare_leg_join_rules: None,
            areas: None,
            stop_areas: None,
            timeframes: None,
            rider_categories: None,
            shapes: None,
            frequencies: None,
            transfers: None,
            location_groups: None,
            location_group_stops: None,
            locations: None,
            booking_rules: None,
            feed_info: None,
            attributions: None,
            levels: None,
            pathways: None,
            translations: None,
            networks: None,
            route_networks: None,
        }
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
