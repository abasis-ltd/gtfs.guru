use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

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

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            validate_stop_type(
                transfer.from_stop_id,
                "from_stop_id",
                &stops_by_id,
                row_number,
                feed,
                notices,
            );
            validate_stop_type(
                transfer.to_stop_id,
                "to_stop_id",
                &stops_by_id,
                row_number,
                feed,
                notices,
            );
        }
    }
}

fn validate_stop_type(
    stop_id: Option<gtfs_guru_model::StringId>,
    field_name: &str,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    row_number: u64,
    feed: &GtfsFeed,
    notices: &mut NoticeContainer,
) {
    let Some(stop_id) = stop_id.filter(|id| id.0 != 0) else {
        return;
    };
    let stop = match stops_by_id.get(&stop_id) {
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
        notice.insert_context_field("stopId", feed.pool.resolve(stop_id).as_str());
        notice.insert_context_field("stopIdFieldName", field_name);
        notice.field_order = vec![
            "csvRowNumber".into(),
            "locationTypeName".into(),
            "locationTypeValue".into(),
            "stopId".into(),
            "stopIdFieldName".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationType, Stop, Transfer};

    #[test]
    fn detects_invalid_stop_type_in_transfer() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    location_type: Some(LocationType::EntranceOrExit), // Invalid for transfer
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersStopTypeValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_WITH_INVALID_STOP_LOCATION_TYPE));
    }

    #[test]
    fn passes_valid_stop_types_in_transfer() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransfersStopTypeValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
