use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::LocationType;

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

        let mut trips_by_id: HashMap<&str, &gtfs_model::Trip> = HashMap::new();
        for trip in &feed.trips.rows {
            let trip_id = trip.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            trips_by_id.insert(trip_id, trip);
        }

        let mut stop_times_by_trip: HashMap<&str, HashSet<&str>> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let stop_id = stop_time.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .insert(stop_id);
        }

        let mut stops_by_id: HashMap<&str, &gtfs_model::Stop> = HashMap::new();
        let mut stops_by_parent: HashMap<&str, Vec<&gtfs_model::Stop>> = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            if let Some(parent_station) = stop.parent_station.as_deref() {
                let parent_station = parent_station.trim();
                if !parent_station.is_empty() {
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
                &stop_times_by_trip,
                &stops_by_id,
                &stops_by_parent,
                row_number,
                notices,
            );
            validate_trip_side(
                transfer,
                TransferSide::To,
                &trips_by_id,
                &stop_times_by_trip,
                &stops_by_id,
                &stops_by_parent,
                row_number,
                notices,
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
    fn trip_id<'a>(&self, transfer: &'a gtfs_model::Transfer) -> Option<&'a str> {
        match self {
            TransferSide::From => transfer.from_trip_id.as_deref(),
            TransferSide::To => transfer.to_trip_id.as_deref(),
        }
    }

    fn route_id<'a>(&self, transfer: &'a gtfs_model::Transfer) -> Option<&'a str> {
        match self {
            TransferSide::From => transfer.from_route_id.as_deref(),
            TransferSide::To => transfer.to_route_id.as_deref(),
        }
    }

    fn stop_id<'a>(&self, transfer: &'a gtfs_model::Transfer) -> Option<&'a str> {
        match self {
            TransferSide::From => transfer.from_stop_id.as_ref().map(|value| value.as_str()),
            TransferSide::To => transfer.to_stop_id.as_ref().map(|value| value.as_str()),
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
    transfer: &gtfs_model::Transfer,
    side: TransferSide,
    trips_by_id: &HashMap<&str, &gtfs_model::Trip>,
    stop_times_by_trip: &HashMap<&str, HashSet<&str>>,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stops_by_parent: &HashMap<&str, Vec<&gtfs_model::Stop>>,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let trip_id = match side
        .trip_id(transfer)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(trip_id) => trip_id,
        None => return,
    };
    let trip = match trips_by_id.get(trip_id) {
        Some(trip) => *trip,
        None => return,
    };

    if let Some(route_id) = side
        .route_id(transfer)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if route_id != trip.route_id.trim() {
            let mut notice = ValidationNotice::new(
                CODE_TRANSFER_WITH_INVALID_TRIP_AND_ROUTE,
                NoticeSeverity::Error,
                "transfer route_id does not match trip route_id",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("expectedRouteId", trip.route_id.trim());
            notice.insert_context_field("routeFieldName", side.route_field_name());
            notice.insert_context_field("routeId", route_id);
            notice.insert_context_field("tripFieldName", side.trip_field_name());
            notice.insert_context_field("tripId", trip_id);
            notice.field_order = vec![
                "csvRowNumber".to_string(),
                "expectedRouteId".to_string(),
                "routeFieldName".to_string(),
                "routeId".to_string(),
                "tripFieldName".to_string(),
                "tripId".to_string(),
            ];
            notices.push(notice);
        }
    }

    let stop_id = match side
        .stop_id(transfer)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(stop_id) => stop_id,
        None => return,
    };
    let stop = match stops_by_id.get(stop_id) {
        Some(stop) => *stop,
        None => return,
    };

    let stop_ids = expand_stop_ids(stop, stops_by_parent);
    if stop_ids.is_empty() {
        return;
    }

    let trip_stop_ids = stop_times_by_trip.get(trip_id);
    let has_match = match trip_stop_ids {
        Some(trip_stop_ids) => stop_ids.iter().any(|id| trip_stop_ids.contains(*id)),
        None => false,
    };
    if !has_match {
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_INVALID_TRIP_AND_STOP,
            NoticeSeverity::Error,
            "transfer stop_id is not included in trip stop_times",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("stopFieldName", side.stop_field_name());
        notice.insert_context_field("stopId", stop_id);
        notice.insert_context_field("tripFieldName", side.trip_field_name());
        notice.insert_context_field("tripId", trip_id);
        notice.field_order = vec![
            "csvRowNumber".to_string(),
            "stopFieldName".to_string(),
            "stopId".to_string(),
            "tripFieldName".to_string(),
            "tripId".to_string(),
        ];
        notices.push(notice);
    }
}

fn expand_stop_ids<'a>(
    stop: &'a gtfs_model::Stop,
    stops_by_parent: &'a HashMap<&str, Vec<&'a gtfs_model::Stop>>,
) -> Vec<&'a str> {
    let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
    match location_type {
        LocationType::StopOrPlatform => vec![stop.stop_id.trim()],
        LocationType::Station => stops_by_parent
            .get(stop.stop_id.trim())
            .map(|stops| stops.iter().map(|child| child.stop_id.trim()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::{Stop, StopTime, Transfer, Trip};

    #[test]
    fn detects_mismatched_trip_and_route() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".to_string(), "route_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".to_string(),
                "to_stop_id".to_string(),
                "from_trip_id".to_string(),
                "from_route_id".to_string(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some("S1".to_string()),
                to_stop_id: Some("S2".to_string()),
                from_trip_id: Some("T1".to_string()),
                from_route_id: Some("R2".to_string()), // Mismatch
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
            headers: vec!["trip_id".to_string(), "route_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
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
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".to_string(),
                "to_stop_id".to_string(),
                "from_trip_id".to_string(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some("S2".to_string()), // T1 does not stop at S2
                to_stop_id: Some("S1".to_string()),
                from_trip_id: Some("T1".to_string()),
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
            headers: vec!["trip_id".to_string(), "route_id".to_string()],
            rows: vec![Trip {
                trip_id: "T1".to_string(),
                route_id: "R1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".to_string(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
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
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".to_string(),
                "to_stop_id".to_string(),
                "from_trip_id".to_string(),
                "from_route_id".to_string(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some("S1".to_string()),
                to_stop_id: Some("S2".to_string()),
                from_trip_id: Some("T1".to_string()),
                from_route_id: Some("R1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersTripReferenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
