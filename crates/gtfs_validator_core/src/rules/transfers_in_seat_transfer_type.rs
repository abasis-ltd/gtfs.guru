use std::collections::HashMap;

use crate::feed::TRANSFERS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{LocationType, TransferType};

const CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE: &str =
    "transfer_with_invalid_stop_location_type";
const CODE_TRANSFER_WITH_SUSPICIOUS_MID_TRIP_IN_SEAT: &str =
    "transfer_with_suspicious_mid_trip_in_seat";
const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";

#[derive(Debug, Default)]
pub struct TransfersInSeatTransferTypeValidator;

impl Validator for TransfersInSeatTransferTypeValidator {
    fn name(&self) -> &'static str {
        "transfers_in_seat_transfer_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(transfers) = &feed.transfers else {
            return;
        };

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        let mut stop_times_by_trip: HashMap<
            gtfs_guru_model::StringId,
            Vec<&gtfs_guru_model::StopTime>,
        > = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            stop_times_by_trip
                .entry(trip_id)
                .or_default()
                .push(stop_time);
        }
        for stop_times in stop_times_by_trip.values_mut() {
            stop_times.sort_by_key(|stop_time| stop_time.stop_sequence);
        }

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            if !is_in_seat_transfer(transfer.transfer_type) {
                continue;
            }
            for side in [TransferSide::From, TransferSide::To] {
                let trip_id = side.trip_id(transfer).filter(|id| id.0 != 0);
                if trip_id.is_none() {
                    notices.push(missing_required_field_notice(
                        side.trip_field_name(),
                        row_number,
                    ));
                }
                validate_stop(
                    transfer,
                    side,
                    trip_id,
                    &stops_by_id,
                    &stop_times_by_trip,
                    row_number,
                    notices,
                    feed,
                );
            }
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

    fn stop_id(&self, transfer: &gtfs_guru_model::Transfer) -> Option<gtfs_guru_model::StringId> {
        match self {
            TransferSide::From => transfer.from_stop_id,
            TransferSide::To => transfer.to_stop_id,
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

fn validate_stop(
    transfer: &gtfs_guru_model::Transfer,
    side: TransferSide,
    trip_id: Option<gtfs_guru_model::StringId>,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    stop_times_by_trip: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::StopTime>>,
    row_number: u64,
    notices: &mut NoticeContainer,
    feed: &GtfsFeed,
) {
    let stop_id = match side.stop_id(transfer).filter(|id| id.0 != 0) {
        Some(stop_id) => stop_id,
        None => return,
    };
    let stop = match stops_by_id.get(&stop_id) {
        Some(stop) => *stop,
        None => return,
    };

    if stop.location_type == Some(LocationType::Station) {
        let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
        let stop_id_value = feed.pool.resolve(stop_id);
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE,
            NoticeSeverity::Error,
            "in-seat transfers cannot reference stations",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("locationTypeName", location_type_name(location_type));
        notice.insert_context_field("locationTypeValue", location_type_value(location_type));
        notice.insert_context_field("stopId", stop_id_value.as_str());
        notice.insert_context_field("stopIdFieldName", side.stop_field_name());
        notice.field_order = vec![
            "csvRowNumber".into(),
            "locationTypeName".into(),
            "locationTypeValue".into(),
            "stopId".into(),
            "stopIdFieldName".into(),
        ];
        notices.push(notice);
    }

    let Some(trip_id) = trip_id else {
        return;
    };
    let stop_times = match stop_times_by_trip.get(&trip_id) {
        Some(stop_times) => stop_times,
        None => return,
    };
    if stop_times.is_empty()
        || !stop_times
            .iter()
            .any(|stop_time| stop_time.stop_id == stop_id)
    {
        return;
    }
    let expected_stop_time = match side {
        TransferSide::From => stop_times[stop_times.len() - 1],
        TransferSide::To => stop_times[0],
    };
    if expected_stop_time.stop_id != stop_id {
        let stop_id_value = feed.pool.resolve(stop_id);
        let trip_id_value = feed.pool.resolve(trip_id);
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_SUSPICIOUS_MID_TRIP_IN_SEAT,
            NoticeSeverity::Warning,
            "in-seat transfer stop is not at expected trip edge",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("stopId", stop_id_value.as_str());
        notice.insert_context_field("stopIdFieldName", side.stop_field_name());
        notice.insert_context_field("tripId", trip_id_value.as_str());
        notice.insert_context_field("tripIdFieldName", side.trip_field_name());
        notice.field_order = vec![
            "csvRowNumber".into(),
            "stopId".into(),
            "stopIdFieldName".into(),
            "tripId".into(),
            "tripIdFieldName".into(),
        ];
        notices.push(notice);
    }
}

fn location_type_value(location_type: LocationType) -> i32 {
    match location_type {
        LocationType::StopOrPlatform => 0,
        LocationType::Station => 1,
        LocationType::EntranceOrExit => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Other => -1,
    }
}

fn location_type_name(location_type: LocationType) -> &'static str {
    match location_type {
        LocationType::StopOrPlatform => "STOP",
        LocationType::Station => "STATION",
        LocationType::EntranceOrExit => "ENTRANCE",
        LocationType::GenericNode => "GENERIC_NODE",
        LocationType::BoardingArea => "BOARDING_AREA",
        LocationType::Other => "UNRECOGNIZED",
    }
}

fn is_in_seat_transfer(transfer_type: Option<TransferType>) -> bool {
    matches!(
        transfer_type,
        Some(TransferType::InSeat) | Some(TransferType::InSeatNotAllowed)
    )
}

fn missing_required_field_notice(field: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_REQUIRED_FIELD,
        NoticeSeverity::Error,
        "required field is missing",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("filename", TRANSFERS_FILE);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationType, Stop, StopTime, Transfer, TransferType};

    #[test]
    fn detects_missing_required_trip_ids() {
        let mut feed = GtfsFeed::default();
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
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "transfer_type".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                transfer_type: Some(TransferType::InSeat),
                from_trip_id: None, // Missing
                to_trip_id: None,   // Missing
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersInSeatTransferTypeValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_MISSING_REQUIRED_FIELD)
                .count(),
            2
        );
    }

    #[test]
    fn detects_station_in_transfer() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "transfer_type".into(),
                "from_trip_id".into(),
                "to_trip_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                transfer_type: Some(TransferType::InSeat),
                from_trip_id: Some(feed.pool.intern("T1")),
                to_trip_id: Some(feed.pool.intern("T2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersInSeatTransferTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE));
    }

    #[test]
    fn detects_suspicious_mid_trip_transfer() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S1_END"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1_END"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "transfer_type".into(),
                "from_trip_id".into(),
                "to_trip_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")), // Not the end of T1
                to_stop_id: Some(feed.pool.intern("S2")),
                transfer_type: Some(TransferType::InSeat),
                from_trip_id: Some(feed.pool.intern("T1")),
                to_trip_id: Some(feed.pool.intern("T2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersInSeatTransferTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_WITH_SUSPICIOUS_MID_TRIP_IN_SEAT));
    }

    #[test]
    fn passes_valid_in_seat_transfer() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S1_END"),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_id: feed.pool.intern("S1_END"),
                    stop_sequence: 10,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T2"),
                    stop_id: feed.pool.intern("S1_END"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T2"),
                    stop_id: feed.pool.intern("S2"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4, 5],
        };
        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "transfer_type".into(),
                "from_trip_id".into(),
                "to_trip_id".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1_END")),
                to_stop_id: Some(feed.pool.intern("S1_END")),
                transfer_type: Some(TransferType::InSeat),
                from_trip_id: Some(feed.pool.intern("T1")),
                to_trip_id: Some(feed.pool.intern("T2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersInSeatTransferTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
