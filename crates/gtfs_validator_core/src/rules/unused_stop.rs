use std::collections::HashSet;

use crate::feed::STOPS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_STOP: &str = "unused_stop";

use crate::validation_context::thorough_mode_enabled;

#[derive(Debug, Default)]
pub struct UnusedStopValidator;

impl Validator for UnusedStopValidator {
    fn name(&self) -> &'static str {
        "unused_stop"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !thorough_mode_enabled() {
            return;
        }

        let mut used_stop_ids: HashSet<&str> = HashSet::new();
        for stop_time in &feed.stop_times.rows {
            let stop_id = stop_time.stop_id.trim();
            if !stop_id.is_empty() {
                used_stop_ids.insert(stop_id);
            }
        }

        // Also check parent stations of used stops
        let mut all_used_ids = used_stop_ids.clone();
        let stops_by_id: std::collections::HashMap<&str, &gtfs_guru_model::Stop> = feed
            .stops
            .rows
            .iter()
            .map(|s| (s.stop_id.as_str(), s))
            .collect();

        let mut queue: Vec<&str> = used_stop_ids.into_iter().collect();
        while let Some(id) = queue.pop() {
            if let Some(stop) = stops_by_id.get(id) {
                if let Some(parent_id) = stop.parent_station.as_deref() {
                    let parent_id = parent_id.trim();
                    if !parent_id.is_empty() && !all_used_ids.contains(parent_id) {
                        all_used_ids.insert(parent_id);
                        queue.push(parent_id);
                    }
                }
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }

            if !all_used_ids.contains(stop_id) {
                let mut notice = ValidationNotice::new(
                    CODE_UNUSED_STOP,
                    NoticeSeverity::Warning,
                    "stop is not used by any trip",
                );
                notice.file = Some(STOPS_FILE.to_string());
                notice.insert_context_field("csvRowNumber", feed.stops.row_number(index));
                notice.insert_context_field("stopId", stop_id);
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                notice.field_order =
                    vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationType, Stop, StopTime};

    #[test]
    fn detects_unused_stop() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_name".into()],
            rows: vec![
                Stop {
                    stop_id: "S1".into(),
                    stop_name: Some("Stop 1".into()),
                    ..Default::default()
                },
                Stop {
                    stop_id: "S2".into(),
                    stop_name: Some("Stop 2".into()),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: "T1".into(),
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UnusedStopValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_UNUSED_STOP)
                .count(),
            1
        );
        let notice = notices.iter().find(|n| n.code == CODE_UNUSED_STOP).unwrap();
        assert_eq!(
            notice.context.get("stopId").unwrap().as_str().unwrap(),
            "S2"
        );
    }

    #[test]
    fn passes_used_stop_and_parent_station() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "stop_name".into(),
                "parent_station".into(),
                "location_type".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: "S1".into(),
                    stop_name: Some("Stop 1".into()),
                    parent_station: Some("ST1".into()),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
                Stop {
                    stop_id: "ST1".into(),
                    stop_name: Some("Station 1".into()),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_id".into()],
            rows: vec![StopTime {
                trip_id: "T1".into(),
                stop_id: "S1".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UnusedStopValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn detects_unused_station_without_used_children() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_name".into(), "location_type".into()],
            rows: vec![Stop {
                stop_id: "ST1".into(),
                stop_name: Some("Station 1".into()),
                location_type: Some(LocationType::Station),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable::default();

        let mut notices = NoticeContainer::new();
        UnusedStopValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_UNUSED_STOP));
    }
}
