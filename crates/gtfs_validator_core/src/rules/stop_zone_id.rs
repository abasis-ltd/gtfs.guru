use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_STOP_WITHOUT_ZONE_ID: &str = "stop_without_zone_id";

#[derive(Debug, Default)]
pub struct StopZoneIdValidator;

impl Validator for StopZoneIdValidator {
    fn name(&self) -> &'static str {
        "stop_zone_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let fare_rules = match &feed.fare_rules {
            Some(fare_rules) if !fare_rules.rows.is_empty() => fare_rules,
            _ => return,
        };
        if !fare_rules_use_zones(fare_rules) {
            return;
        }

        let route_ids_with_zones: HashSet<&str> = fare_rules
            .rows
            .iter()
            .filter(|rule| fare_rule_has_zone_fields(rule))
            .filter_map(|rule| rule.route_id.as_deref())
            .filter(|route_id| !route_id.trim().is_empty())
            .collect();
        if route_ids_with_zones.is_empty() {
            return;
        }

        let mut trip_route_ids: HashMap<&str, &str> = HashMap::new();
        for trip in &feed.trips.rows {
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let route_id = trip.route_id.trim();
            if route_id.is_empty() {
                continue;
            }
            trip_route_ids.insert(trip_id, route_id);
        }

        let mut stop_routes: HashMap<&str, HashSet<&str>> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let stop_id = stop_time.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let route_id = match trip_route_ids.get(trip_id) {
                Some(route_id) => *route_id,
                None => continue,
            };
            stop_routes.entry(stop_id).or_default().insert(route_id);
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            if location_type != LocationType::StopOrPlatform {
                continue;
            }
            if has_value(stop.zone_id.as_deref()) {
                continue;
            }
            let Some(routes_for_stop) = stop_routes.get(stop_id) else {
                continue;
            };
            if routes_for_stop
                .iter()
                .any(|route_id| route_ids_with_zones.contains(route_id))
            {
                let mut notice = ValidationNotice::new(
                    CODE_STOP_WITHOUT_ZONE_ID,
                    NoticeSeverity::Info,
                    "stop is missing zone_id required by fare rules",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopId", stop_id);
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "stopId".to_string(),
                    "stopName".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn fare_rules_use_zones(fare_rules: &crate::CsvTable<gtfs_guru_model::FareRule>) -> bool {
    fare_rules.rows.iter().any(fare_rule_has_zone_fields)
}

fn fare_rule_has_zone_fields(rule: &gtfs_guru_model::FareRule) -> bool {
    has_value(rule.origin_id.as_deref())
        || has_value(rule.destination_id.as_deref())
        || has_value(rule.contains_id.as_deref())
}

fn has_value(value: Option<&str>) -> bool {
    value.map(|val| !val.trim().is_empty()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{FareRule, Stop, StopTime, Trip};

    #[test]
    fn detects_missing_zone_id() {
        let mut feed = GtfsFeed::default();
        feed.fare_rules = Some(CsvTable {
            headers: vec![
                "fare_id".to_string(),
                "route_id".to_string(),
                "origin_id".to_string(),
            ],
            rows: vec![FareRule {
                fare_id: "F1".to_string(),
                route_id: Some("R1".to_string()),
                origin_id: Some("Z1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "route_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_id".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "zone_id".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                zone_id: None, // Missing
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopZoneIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_STOP_WITHOUT_ZONE_ID
        );
    }

    #[test]
    fn passes_when_zone_id_present() {
        let mut feed = GtfsFeed::default();
        feed.fare_rules = Some(CsvTable {
            headers: vec![
                "fare_id".to_string(),
                "route_id".to_string(),
                "origin_id".to_string(),
            ],
            rows: vec![FareRule {
                fare_id: "F1".to_string(),
                route_id: Some("R1".to_string()),
                origin_id: Some("Z1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "route_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_id".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "zone_id".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                zone_id: Some("Z1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopZoneIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
