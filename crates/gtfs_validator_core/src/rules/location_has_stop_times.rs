use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::LocationType;

const CODE_STOP_WITHOUT_STOP_TIME: &str = "stop_without_stop_time";
const CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME: &str = "location_with_unexpected_stop_time";

#[derive(Debug, Default)]
pub struct LocationHasStopTimesValidator;

impl Validator for LocationHasStopTimesValidator {
    fn name(&self) -> &'static str {
        "location_has_stop_times"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut stop_ids_in_stop_times: HashSet<&str> = HashSet::new();
        let mut stop_time_row_by_stop_id: HashMap<&str, u64> = HashMap::new();
        let mut location_group_ids: HashSet<&str> = HashSet::new();

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let stop_id = stop_time.stop_id.trim();
            if !stop_id.is_empty() {
                stop_ids_in_stop_times.insert(stop_id);
                stop_time_row_by_stop_id
                    .entry(stop_id)
                    .or_insert(row_number);
            }
            if let Some(group_id) = stop_time
                .location_group_id
                .as_deref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                location_group_ids.insert(group_id);
            }
        }

        let mut stop_ids_in_stop_times_and_groups: HashSet<String> = stop_ids_in_stop_times
            .iter()
            .map(|stop_id| (*stop_id).to_string())
            .collect();

        if let Some(location_group_stops) = &feed.location_group_stops {
            let mut group_to_stops: HashMap<&str, Vec<&str>> = HashMap::new();
            for row in &location_group_stops.rows {
                let group_id = row.location_group_id.trim();
                if group_id.is_empty() {
                    continue;
                }
                let stop_id = row.stop_id.trim();
                if stop_id.is_empty() {
                    continue;
                }
                group_to_stops.entry(group_id).or_default().push(stop_id);
            }
            for group_id in location_group_ids {
                if let Some(stops) = group_to_stops.get(group_id) {
                    for stop_id in stops {
                        stop_ids_in_stop_times_and_groups.insert((*stop_id).to_string());
                    }
                }
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            if location_type == LocationType::StopOrPlatform {
                if !stop_ids_in_stop_times_and_groups.contains(stop_id) {
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_WITHOUT_STOP_TIME,
                        NoticeSeverity::Warning,
                        "stop has no stop_times entries",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("stopId", stop_id);
                    notice
                        .insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "stopId".to_string(),
                        "stopName".to_string(),
                    ];
                    notices.push(notice);
                }
            } else if stop_ids_in_stop_times.contains(stop_id) {
                let mut notice = ValidationNotice::new(
                    CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME,
                    NoticeSeverity::Error,
                    "non-stop location has stop_times entries",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopId", stop_id);
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                if let Some(stop_time_row) = stop_time_row_by_stop_id.get(stop_id) {
                    notice.insert_context_field("stopTimeCsvRowNumber", *stop_time_row);
                }
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "stopId".to_string(),
                    "stopName".to_string(),
                    "stopTimeCsvRowNumber".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::{LocationType, Stop, StopTime};

    #[test]
    fn detects_stop_without_stop_time() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // stop_times is empty

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_STOP_WITHOUT_STOP_TIME
        );
    }

    #[test]
    fn detects_location_with_unexpected_stop_time() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "location_type".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                location_type: Some(LocationType::Station),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![StopTime {
                stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME
        );
    }

    #[test]
    fn passes_valid_cases() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "location_type".to_string()],
            rows: vec![
                Stop {
                    stop_id: "S1".to_string(),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
                Stop {
                    stop_id: "P1".to_string(),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![StopTime {
                stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
