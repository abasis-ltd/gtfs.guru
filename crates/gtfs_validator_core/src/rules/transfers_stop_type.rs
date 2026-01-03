use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::LocationType;

const CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE: &str =
    "transfer_with_invalid_stop_location_type";

#[derive(Debug, Default)]
pub struct TransfersStopTypeValidator;

impl Validator for TransfersStopTypeValidator {
    fn name(&self) -> &'static str {
        "transfers_stop_type"
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

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            validate_stop_type(
                transfer.from_stop_id.as_deref(),
                "from_stop_id",
                &stops_by_id,
                row_number,
                notices,
            );
            validate_stop_type(
                transfer.to_stop_id.as_deref(),
                "to_stop_id",
                &stops_by_id,
                row_number,
                notices,
            );
        }
    }
}

fn validate_stop_type(
    stop_id: Option<&str>,
    field_name: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let stop_id = match stop_id
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
    let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
    if !is_valid_transfer_stop_type(location_type) {
        let mut notice = ValidationNotice::new(
            CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE,
            NoticeSeverity::Error,
            "transfer references stop with invalid location_type",
        );
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("locationTypeName", location_type_name(location_type));
        notice.insert_context_field("locationTypeValue", location_type_value(location_type));
        notice.insert_context_field("stopId", stop_id);
        notice.insert_context_field("stopIdFieldName", field_name);
        notice.field_order = vec![
            "csvRowNumber".to_string(),
            "locationTypeName".to_string(),
            "locationTypeValue".to_string(),
            "stopId".to_string(),
            "stopIdFieldName".to_string(),
        ];
        notices.push(notice);
    }
}

fn is_valid_transfer_stop_type(location_type: LocationType) -> bool {
    matches!(
        location_type,
        LocationType::StopOrPlatform | LocationType::Station
    )
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

