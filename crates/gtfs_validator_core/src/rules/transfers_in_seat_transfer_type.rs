use std::collections::HashMap;

use crate::feed::TRANSFERS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::{LocationType, TransferType};

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

        let mut stops_by_id: HashMap<&str, &gtfs_model::Stop> = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        let mut stop_times_by_trip: HashMap<&str, Vec<&gtfs_model::StopTime>> = HashMap::new();
        for stop_time in &feed.stop_times.rows {
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
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
                let trip_id = side
                    .trip_id(transfer)
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty());
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
    fn trip_id<'a>(&self, transfer: &'a gtfs_model::Transfer) -> Option<&'a str> {
        match self {
            TransferSide::From => transfer.from_trip_id.as_deref(),
            TransferSide::To => transfer.to_trip_id.as_deref(),
        }
    }

    fn stop_id<'a>(&self, transfer: &'a gtfs_model::Transfer) -> Option<&'a str> {
        match self {
            TransferSide::From => transfer.from_stop_id.as_deref(),
            TransferSide::To => transfer.to_stop_id.as_deref(),
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
    transfer: &gtfs_model::Transfer,
    side: TransferSide,
    trip_id: Option<&str>,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    stop_times_by_trip: &HashMap<&str, Vec<&gtfs_model::StopTime>>,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
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

    if stop.location_type == Some(LocationType::Station) {
        let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE,
            NoticeSeverity::Error,
            "in-seat transfers cannot reference stations",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("locationTypeName", location_type_name(location_type));
        notice.insert_context_field("locationTypeValue", location_type_value(location_type));
        notice.insert_context_field("stopId", stop_id);
        notice.insert_context_field("stopIdFieldName", side.stop_field_name());
        notice.field_order = vec![
            "csvRowNumber".to_string(),
            "locationTypeName".to_string(),
            "locationTypeValue".to_string(),
            "stopId".to_string(),
            "stopIdFieldName".to_string(),
        ];
        notices.push(notice);
    }

    let Some(trip_id) = trip_id else {
        return;
    };
    let stop_times = match stop_times_by_trip.get(trip_id) {
        Some(stop_times) => stop_times,
        None => return,
    };
    if stop_times.is_empty()
        || !stop_times
            .iter()
            .any(|stop_time| stop_time.stop_id.trim() == stop_id)
    {
        return;
    }
    let expected_stop_time = match side {
        TransferSide::From => stop_times[stop_times.len() - 1],
        TransferSide::To => stop_times[0],
    };
    if expected_stop_time.stop_id.trim() != stop_id {
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_SUSPICIOUS_MID_TRIP_IN_SEAT,
            NoticeSeverity::Warning,
            "in-seat transfer stop is not at expected trip edge",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("stopId", stop_id);
        notice.insert_context_field("stopIdFieldName", side.stop_field_name());
        notice.insert_context_field("tripId", trip_id);
        notice.insert_context_field("tripIdFieldName", side.trip_field_name());
        notice.field_order = vec![
            "csvRowNumber".to_string(),
            "stopId".to_string(),
            "stopIdFieldName".to_string(),
            "tripId".to_string(),
            "tripIdFieldName".to_string(),
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
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "filename".to_string(),
    ];
    notice
}

