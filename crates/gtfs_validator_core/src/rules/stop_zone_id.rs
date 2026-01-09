use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;
use gtfs_guru_model::StringId;

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

        let route_ids_with_zones: HashSet<gtfs_guru_model::StringId> = fare_rules
            .rows
            .iter()
            .filter(|rule| fare_rule_has_zone_fields(rule))
            .filter_map(|rule| rule.route_id)
            .filter(|route_id| route_id.0 != 0)
            .collect();
        if route_ids_with_zones.is_empty() {
            return;
        }

        let mut trip_route_ids: HashMap<gtfs_guru_model::StringId, gtfs_guru_model::StringId> =
            HashMap::new();
        for trip in &feed.trips.rows {
            let trip_id = trip.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            let route_id = trip.route_id;
            if route_id.0 == 0 {
                continue;
            }
            trip_route_ids.insert(trip_id, route_id);
        }

        let mut stop_routes: HashMap<
            gtfs_guru_model::StringId,
            HashSet<gtfs_guru_model::StringId>,
        > = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let stop_id = stop_time.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            let route_id = match trip_route_ids.get(&trip_id) {
                Some(route_id) => *route_id,
                None => continue,
            };
            stop_routes.entry(stop_id).or_default().insert(route_id);
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            if location_type != LocationType::StopOrPlatform {
                continue;
            }
            if stop.zone_id.unwrap_or(StringId(0)).0 != 0 {
                continue;
            }
            let Some(routes_for_stop) = stop_routes.get(&stop_id) else {
                continue;
            };
            if routes_for_stop
                .iter()
                .any(|route_id| route_ids_with_zones.contains(route_id))
            {
                let stop_id_value = feed.pool.resolve(stop_id);
                let mut notice = ValidationNotice::new(
                    CODE_STOP_WITHOUT_ZONE_ID,
                    NoticeSeverity::Info,
                    "stop is missing zone_id required by fare rules",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopId", stop_id_value.as_str());
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                notice.field_order =
                    vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
                notices.push(notice);
            }
        }
    }
}

fn fare_rules_use_zones(fare_rules: &crate::CsvTable<gtfs_guru_model::FareRule>) -> bool {
    fare_rules.rows.iter().any(fare_rule_has_zone_fields)
}

fn fare_rule_has_zone_fields(rule: &gtfs_guru_model::FareRule) -> bool {
    rule.origin_id.unwrap_or(StringId(0)).0 != 0
        || rule.destination_id.unwrap_or(StringId(0)).0 != 0
        || rule.contains_id.unwrap_or(StringId(0)).0 != 0
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
            headers: vec!["fare_id".into(), "route_id".into(), "origin_id".into()],
            rows: vec![FareRule {
                fare_id: feed.pool.intern("F1"),
                route_id: Some(feed.pool.intern("R1")),
                origin_id: Some(feed.pool.intern("Z1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
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
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "zone_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
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
            headers: vec!["fare_id".into(), "route_id".into(), "origin_id".into()],
            rows: vec![FareRule {
                fare_id: feed.pool.intern("F1"),
                route_id: Some(feed.pool.intern("R1")),
                origin_id: Some(feed.pool.intern("Z1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
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
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "zone_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                zone_id: Some(feed.pool.intern("Z1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopZoneIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
