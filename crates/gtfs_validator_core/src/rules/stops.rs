use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_MISSING_STOP_NAME: &str = "missing_stop_name";
const CODE_STOP_WITHOUT_LOCATION: &str = "stop_without_location";
const CODE_SAME_NAME_AND_DESCRIPTION: &str = "same_name_and_description_for_stop";

#[derive(Debug, Default)]
pub struct StopsValidator;

impl Validator for StopsValidator {
    fn name(&self) -> &'static str {
        "stops_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            if requires_name_and_location(stop.location_type)
                && stop
                    .stop_name
                    .as_ref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
            {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_STOP_NAME,
                    NoticeSeverity::Error,
                    "stop_name is required for stop locations",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice
                    .insert_context_field("locationType", location_type_label(stop.location_type));
                notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "locationType".into(),
                    "stopId".into(),
                ];
                notices.push(notice);
            }

            if requires_name_and_location(stop.location_type) && !stop.has_coordinates() {
                let mut notice = ValidationNotice::new(
                    CODE_STOP_WITHOUT_LOCATION,
                    NoticeSeverity::Error,
                    "stop_lat and stop_lon are required for stop locations",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice
                    .insert_context_field("locationType", location_type_label(stop.location_type));
                notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "locationType".into(),
                    "stopId".into(),
                ];
                notices.push(notice);
            }

            if let (Some(name), Some(desc)) = (stop.stop_name.as_ref(), stop.stop_desc.as_ref()) {
                if name.eq_ignore_ascii_case(desc) {
                    let mut notice = ValidationNotice::new(
                        CODE_SAME_NAME_AND_DESCRIPTION,
                        NoticeSeverity::Warning,
                        "stop_desc should not duplicate stop_name",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice.insert_context_field("stopDesc", desc.as_str());
                    notice.field_order =
                        vec!["csvRowNumber".into(), "stopDesc".into(), "stopId".into()];
                    notices.push(notice);
                }
            }
        }
    }
}

fn requires_name_and_location(location_type: Option<LocationType>) -> bool {
    match location_type {
        None
        | Some(LocationType::StopOrPlatform)
        | Some(LocationType::Station)
        | Some(LocationType::EntranceOrExit) => true,
        Some(LocationType::GenericNode)
        | Some(LocationType::BoardingArea)
        | Some(LocationType::Other) => false,
    }
}

fn location_type_label(location_type: Option<LocationType>) -> &'static str {
    match location_type.unwrap_or(LocationType::StopOrPlatform) {
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
    use gtfs_guru_model::Stop;

    #[test]
    fn detects_missing_stop_name() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_name: None,
                stop_lat: Some(40.0),
                stop_lon: Some(-74.0),
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_MISSING_STOP_NAME);
    }

    #[test]
    fn detects_stop_without_location() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_name: Some("Main St".into()),
                stop_lat: None,
                stop_lon: None,
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_STOP_WITHOUT_LOCATION
        );
    }

    #[test]
    fn detects_same_name_and_description() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_name: Some("Main St".into()),
                stop_desc: Some("Main St".into()),
                stop_lat: Some(40.0),
                stop_lon: Some(-74.0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_SAME_NAME_AND_DESCRIPTION
        );
    }

    #[test]
    fn passes_with_valid_stop() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_name: Some("Main St".into()),
                stop_lat: Some(40.0),
                stop_lon: Some(-74.0),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn generic_node_does_not_require_name() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_name: None,
                stop_lat: None,
                stop_lon: None,
                location_type: Some(LocationType::GenericNode),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopsValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
