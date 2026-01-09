use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_TRANSFER_WITH_INVALID_TRIP_AND_ROUTE: &str = "transfer_with_invalid_trip_and_route";
const CODE_TRANSFER_WITH_INVALID_TRIP_AND_STOP: &str = "transfer_with_invalid_trip_and_stop";

#[derive(Debug, Default)]
pub struct TransfersTripReferenceValidator;

impl Validator for TransfersTripReferenceValidator {
    fn name(&self) -> &'static str {
        "transfers_trip_reference"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(transfers) = &feed.transfers else {
            return;
        };

        let mut trips_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Trip> =
            HashMap::new();
        for trip in &feed.trips.rows {
            let trip_id = trip.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            trips_by_id.insert(trip_id, trip);
        }

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        let mut stops_by_parent: HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Stop>> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            if let Some(parent_station) = stop.parent_station {
                if parent_station.0 != 0 {
                    stops_by_parent
                        .entry(parent_station)
                        .or_default()
                        .push(stop);
                }
            }
        }

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            validate_trip_side(
                transfer,
                TransferSide::From,
                &trips_by_id,
                &stops_by_id,
                &stops_by_parent,
                row_number,
                notices,
                feed,
            );
            validate_trip_side(
                transfer,
                TransferSide::To,
                &trips_by_id,
                &stops_by_id,
                &stops_by_parent,
                row_number,
                notices,
                feed,
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TransferSide {
    From,
    To,
}

impl TransferSide {
    fn trip_id(&self, transfer: &gtfs_guru_model::Transfer) -> Option<gtfs_guru_model::StringId> {
        match self {
            TransferSide::From => transfer.from_trip_id,
            TransferSide::To => transfer.to_trip_id,
        }
    }

    fn route_id(&self, transfer: &gtfs_guru_model::Transfer) -> Option<gtfs_guru_model::StringId> {
        match self {
            TransferSide::From => transfer.from_route_id,
            TransferSide::To => transfer.to_route_id,
        }
    }

    fn stop_id(&self, transfer: &gtfs_guru_model::Transfer) -> Option<gtfs_guru_model::StringId> {
        match self {
            TransferSide::From => transfer.from_stop_id,
            TransferSide::To => transfer.to_stop_id,
        }
    }

    fn route_field_name(&self) -> &'static str {
        match self {
            TransferSide::From => "from_route_id",
            TransferSide::To => "to_route_id",
        }
    }

    fn trip_field_name(&self) -> &'static str {
        match self {
            TransferSide::From => "from_trip_id",
            TransferSide::To => "to_trip_id",
        }
    }

    fn stop_field_name(&self) -> &'static str {
        match self {
            TransferSide::From => "from_stop_id",
            TransferSide::To => "to_stop_id",
        }
    }
}

fn validate_trip_side(
    transfer: &gtfs_guru_model::Transfer,
    side: TransferSide,
    trips_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Trip>,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stops_by_parent: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Stop>>,
    row_number: u64,
    notices: &mut NoticeContainer,
    feed: &GtfsFeed,
) {
    let trip_id = match side.trip_id(transfer).filter(|id| id.0 != 0) {
        Some(trip_id) => trip_id,
        None => return,
    };
    let trip = match trips_by_id.get(&trip_id) {
        Some(trip) => *trip,
        None => return,
    };

    if let Some(route_id) = side.route_id(transfer).filter(|id| id.0 != 0) {
        if route_id != trip.route_id {
            let expected_route_id = feed.pool.resolve(trip.route_id);
            let route_id_value = feed.pool.resolve(route_id);
            let trip_id_value = feed.pool.resolve(trip_id);
            let mut notice = ValidationNotice::new(
                CODE_TRANSFER_WITH_INVALID_TRIP_AND_ROUTE,
                NoticeSeverity::Error,
                "transfer route_id does not match trip route_id",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("expectedRouteId", expected_route_id.as_str());
            notice.insert_context_field("routeFieldName", side.route_field_name());
            notice.insert_context_field("routeId", route_id_value.as_str());
            notice.insert_context_field("tripFieldName", side.trip_field_name());
            notice.insert_context_field("tripId", trip_id_value.as_str());
            notice.field_order = vec![
                "csvRowNumber".into(),
                "expectedRouteId".into(),
                "routeFieldName".into(),
                "routeId".into(),
                "tripFieldName".into(),
                "tripId".into(),
            ];
            notices.push(notice);
        }
    }

    let stop_id = match side.stop_id(transfer).filter(|id| id.0 != 0) {
        Some(stop_id) => stop_id,
        None => return,
    };
    let stop = match stops_by_id.get(&stop_id) {
        Some(stop) => *stop,
        None => return,
    };

    let stop_ids = expand_stop_ids(stop, stops_by_parent);
    if stop_ids.is_empty() {
        return;
    }

    let has_match = match feed.stop_times_by_trip.get(&trip_id) {
        Some(indices) => indices.iter().any(|&index| {
            let st = &feed.stop_times.rows[index];
            stop_ids.contains(&st.stop_id)
        }),
        None => false,
    };

    if !has_match {
        let stop_id_value = feed.pool.resolve(stop_id);
        let trip_id_value = feed.pool.resolve(trip_id);
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_INVALID_TRIP_AND_STOP,
            NoticeSeverity::Error,
            "transfer stop_id is not included in trip stop_times",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("stopFieldName", side.stop_field_name());
        notice.insert_context_field("stopId", stop_id_value.as_str());
        notice.insert_context_field("tripFieldName", side.trip_field_name());
        notice.insert_context_field("tripId", trip_id_value.as_str());
        notice.field_order = vec![
            "csvRowNumber".into(),
            "stopFieldName".into(),
            "stopId".into(),
            "tripFieldName".into(),
            "tripId".into(),
        ];
        notices.push(notice);
    }
}

fn expand_stop_ids<'a>(
    stop: &'a gtfs_guru_model::Stop,
    stops_by_parent: &'a HashMap<gtfs_guru_model::StringId, Vec<&'a gtfs_guru_model::Stop>>,
) -> Vec<gtfs_guru_model::StringId> {
    let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
    match location_type {
        LocationType::StopOrPlatform => vec![stop.stop_id],
        LocationType::Station => stops_by_parent
            .get(&stop.stop_id)
            .map(|stops| stops.iter().map(|child| child.stop_id).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Stop, StopTime, Transfer, Trip};

    #[test]
    fn detects_mismatched_trip_and_route() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "from_trip_id".into(),
                "from_route_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                from_trip_id: Some(feed.pool.intern("T1")),
                from_route_id: Some(feed.pool.intern("R2")), // Mismatch
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersTripReferenceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_WITH_INVALID_TRIP_AND_ROUTE));
    }

    #[test]
    fn detects_trip_missing_stop() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
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
        feed.rebuild_stop_times_index();
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "from_trip_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S2")), // T1 does not stop at S2
                to_stop_id: Some(feed.pool.intern("S1")),
                from_trip_id: Some(feed.pool.intern("T1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersTripReferenceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_WITH_INVALID_TRIP_AND_STOP));
    }

    #[test]
    fn passes_valid_trip_reference() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "route_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                route_id: feed.pool.intern("R1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
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
        feed.rebuild_stop_times_index();
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "from_trip_id".into(),
                "from_route_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                from_trip_id: Some(feed.pool.intern("T1")),
                from_route_id: Some(feed.pool.intern("R1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersTripReferenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
