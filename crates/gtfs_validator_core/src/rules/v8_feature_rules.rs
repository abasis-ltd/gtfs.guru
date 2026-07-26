use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{LocationType, RouteType, StopAccess, TransferType};

#[derive(Debug, Default)]
pub struct StopAccessValidator;

impl Validator for StopAccessValidator {
    fn name(&self) -> &'static str {
        "stop_access"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !feed
            .stops
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("stop_access"))
        {
            return;
        }
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let Some(stop_access) = stop.stop_access else {
                continue;
            };
            if stop_access == StopAccess::Other {
                continue;
            }
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            let mut notice = if location_type == LocationType::StopOrPlatform {
                if stop.parent_station.filter(|id| id.0 != 0).is_some() {
                    continue;
                }
                ValidationNotice::new(
                    "stop_access_specified_for_stop_with_no_parent_station",
                    NoticeSeverity::Error,
                    "stop_access is specified for a stop with no parent station",
                )
            } else {
                ValidationNotice::new(
                    "stop_access_specified_for_incorrect_location",
                    NoticeSeverity::Error,
                    "stop_access is specified for an incompatible location type",
                )
            };
            notice.insert_context_field("csvRowNumber", feed.stops.row_number(index));
            notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
            notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or_default());
            notice.insert_context_field("stopAccess", stop_access_value(stop_access));
            notice.insert_context_field("locationType", location_type_value(location_type));
            notice.field_order = vec![
                "csvRowNumber".into(),
                "stopId".into(),
                "stopName".into(),
                "stopAccess".into(),
                "locationType".into(),
            ];
            notices.push(notice);
        }
    }
}

#[derive(Debug, Default)]
pub struct PathwayStopAccessValidator;

impl Validator for PathwayStopAccessValidator {
    fn name(&self) -> &'static str {
        "pathway_stop_access"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };
        let inaccessible_stops: HashMap<_, _> = feed
            .stops
            .rows
            .iter()
            .filter(|stop| stop.stop_access == Some(StopAccess::NotAccessibleViaPathways))
            .map(|stop| {
                (
                    stop.stop_id,
                    stop.platform_code.as_deref().unwrap_or_default(),
                )
            })
            .collect();
        if inaccessible_stops.is_empty() {
            return;
        }
        for (index, pathway) in pathways.rows.iter().enumerate() {
            let mut emitted = HashSet::new();
            for stop_id in [pathway.from_stop_id, pathway.to_stop_id] {
                let Some(platform_code) = inaccessible_stops.get(&stop_id) else {
                    continue;
                };
                if !emitted.insert(stop_id) {
                    continue;
                }
                let mut notice = ValidationNotice::new(
                    "pathway_to_stop_with_access_outside_of_station_pathways",
                    NoticeSeverity::Error,
                    "a pathway references a stop that is not accessible via station pathways",
                );
                notice.insert_context_field("csvRowNumber", pathways.row_number(index));
                notice.insert_context_field("platformCode", *platform_code);
                notice.insert_context_field(
                    "pathwayId",
                    feed.pool.resolve(pathway.pathway_id).as_str(),
                );
                notice.insert_context_field("stopId", feed.pool.resolve(stop_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "platformCode".into(),
                    "pathwayId".into(),
                    "stopId".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct InconsistentRouteTypeForBlockIdValidator;

impl Validator for InconsistentRouteTypeForBlockIdValidator {
    fn name(&self) -> &'static str {
        "inconsistent_route_type_for_block_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let route_types: HashMap<_, _> = feed
            .routes
            .rows
            .iter()
            .filter_map(|route| {
                route_type_value(route.route_type).map(|value| (route.route_id, value))
            })
            .collect();
        let mut blocks: HashMap<_, Vec<_>> = HashMap::new();
        for trip in &feed.trips.rows {
            if let Some(block_id) = trip.block_id.filter(|id| id.0 != 0) {
                if let Some(route_type) = route_types.get(&trip.route_id).copied() {
                    blocks
                        .entry(block_id)
                        .or_default()
                        .push((trip.route_id, route_type));
                }
            }
        }
        for (block_id, entries) in blocks {
            let distinct_types: Vec<_> = entries.iter().map(|(_, route_type)| *route_type).fold(
                Vec::new(),
                |mut values, value| {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                    values
                },
            );
            if distinct_types.len() <= 1 {
                continue;
            }
            let route_ids: Vec<_> = entries.iter().map(|(route_id, _)| *route_id).fold(
                Vec::new(),
                |mut values, value| {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                    values
                },
            );
            let mut notice = ValidationNotice::new(
                "inconsistent_route_type_for_block_id",
                NoticeSeverity::Warning,
                "a block contains trips with different route modes",
            );
            notice.insert_context_field("blockId", feed.pool.resolve(block_id).as_str());
            notice.insert_context_field(
                "routeIds",
                route_ids
                    .iter()
                    .map(|id| feed.pool.resolve(*id).to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            notice.insert_context_field(
                "routeTypes",
                distinct_types
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            notice.field_order = vec!["blockId".into(), "routeIds".into(), "routeTypes".into()];
            notices.push(notice);
        }
    }
}

#[derive(Debug, Default)]
pub struct InconsistentRouteTypeForInSeatTransferValidator;

impl Validator for InconsistentRouteTypeForInSeatTransferValidator {
    fn name(&self) -> &'static str {
        "inconsistent_route_type_for_in_seat_transfer"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(transfers) = &feed.transfers else {
            return;
        };
        let route_types: HashMap<_, _> = feed
            .routes
            .rows
            .iter()
            .filter_map(|route| {
                route_type_value(route.route_type).map(|value| (route.route_id, value))
            })
            .collect();
        for (index, transfer) in transfers.rows.iter().enumerate() {
            if transfer.transfer_type != Some(TransferType::InSeat) {
                continue;
            }
            let (Some(from_route_id), Some(to_route_id)) =
                (transfer.from_route_id, transfer.to_route_id)
            else {
                continue;
            };
            let (Some(from_type), Some(to_type)) = (
                route_types.get(&from_route_id).copied(),
                route_types.get(&to_route_id).copied(),
            ) else {
                continue;
            };
            if from_type == to_type {
                continue;
            }
            let mut notice = ValidationNotice::new(
                "inconsistent_route_type_for_in_seat_transfer",
                NoticeSeverity::Warning,
                "an in-seat transfer connects routes with different modes",
            );
            notice.insert_context_field("csvRowNumber", transfers.row_number(index));
            notice.insert_context_field("fromRouteId", feed.pool.resolve(from_route_id).as_str());
            notice.insert_context_field("toRouteId", feed.pool.resolve(to_route_id).as_str());
            notice.insert_context_field("fromRouteType", from_type);
            notice.insert_context_field("toRouteType", to_type);
            notice.field_order = vec![
                "csvRowNumber".into(),
                "fromRouteId".into(),
                "toRouteId".into(),
                "fromRouteType".into(),
                "toRouteType".into(),
            ];
            notices.push(notice);
        }
    }
}

fn stop_access_value(value: StopAccess) -> i32 {
    match value {
        StopAccess::AccessibleViaPathways => 0,
        StopAccess::NotAccessibleViaPathways => 1,
        StopAccess::Other => -1,
    }
}

fn location_type_value(value: LocationType) -> i32 {
    match value {
        LocationType::StopOrPlatform => 0,
        LocationType::Station => 1,
        LocationType::EntranceOrExit => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Other => -1,
    }
}

fn route_type_value(value: RouteType) -> Option<i32> {
    match value {
        RouteType::Tram => Some(0),
        RouteType::Subway => Some(1),
        RouteType::Rail => Some(2),
        RouteType::Bus => Some(3),
        RouteType::Ferry => Some(4),
        RouteType::CableCar => Some(5),
        RouteType::Gondola => Some(6),
        RouteType::Funicular => Some(7),
        RouteType::Trolleybus => Some(11),
        RouteType::Monorail => Some(12),
        // MobilityData's canonical typed route table rejects extended HVT
        // values, so validators that consume that table never compare them.
        RouteType::Extended(_) | RouteType::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Pathway, Route, Stop, Transfer, Trip};

    #[test]
    fn validates_stop_and_pathway_access() {
        let mut feed = GtfsFeed::default();
        let stop_id = feed.pool.intern("S1");
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_access".into()],
            rows: vec![Stop {
                stop_id,
                stop_access: Some(StopAccess::NotAccessibleViaPathways),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec!["pathway_id".into(), "from_stop_id".into()],
            rows: vec![Pathway {
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: stop_id,
                to_stop_id: feed.pool.intern("S2"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        StopAccessValidator.validate(&feed, &mut notices);
        PathwayStopAccessValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|notice| {
            notice.code == "stop_access_specified_for_stop_with_no_parent_station"
        }));
        assert!(notices.iter().any(|notice| {
            notice.code == "pathway_to_stop_with_access_outside_of_station_pathways"
        }));
    }

    #[test]
    fn validates_route_type_consistency() {
        let mut feed = GtfsFeed::default();
        let route_1 = feed.pool.intern("R1");
        let route_2 = feed.pool.intern("R2");
        let block_id = feed.pool.intern("B1");
        feed.routes.rows = vec![
            Route {
                route_id: route_1,
                route_type: RouteType::Bus,
                ..Default::default()
            },
            Route {
                route_id: route_2,
                route_type: RouteType::Ferry,
                ..Default::default()
            },
        ];
        feed.trips.rows = vec![
            Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: route_1,
                block_id: Some(block_id),
                ..Default::default()
            },
            Trip {
                trip_id: feed.pool.intern("T2"),
                route_id: route_2,
                block_id: Some(block_id),
                ..Default::default()
            },
        ];
        feed.transfers = Some(CsvTable {
            headers: vec![
                "transfer_type".into(),
                "from_route_id".into(),
                "to_route_id".into(),
            ],
            rows: vec![Transfer {
                transfer_type: Some(TransferType::InSeat),
                from_route_id: Some(route_1),
                to_route_id: Some(route_2),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        InconsistentRouteTypeForBlockIdValidator.validate(&feed, &mut notices);
        InconsistentRouteTypeForInSeatTransferValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == "inconsistent_route_type_for_block_id"));
        assert!(notices
            .iter()
            .any(|notice| { notice.code == "inconsistent_route_type_for_in_seat_transfer" }));
    }

    #[test]
    fn extended_route_types_are_absent_from_the_canonical_typed_table() {
        let mut feed = GtfsFeed::default();
        let route_1 = feed.pool.intern("R1");
        let route_2 = feed.pool.intern("R2");
        let block_id = feed.pool.intern("B1");
        feed.routes.rows = vec![
            Route {
                route_id: route_1,
                route_type: RouteType::Bus,
                ..Default::default()
            },
            Route {
                route_id: route_2,
                route_type: RouteType::Extended(700),
                ..Default::default()
            },
        ];
        feed.trips.rows = vec![
            Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: route_1,
                block_id: Some(block_id),
                ..Default::default()
            },
            Trip {
                trip_id: feed.pool.intern("T2"),
                route_id: route_2,
                block_id: Some(block_id),
                ..Default::default()
            },
        ];

        let mut notices = NoticeContainer::new();
        InconsistentRouteTypeForBlockIdValidator.validate(&feed, &mut notices);

        assert!(
            notices.is_empty(),
            "extended route types are absent from the canonical validator's typed route table"
        );
    }
}
