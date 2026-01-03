use std::collections::HashSet;

use crate::feed::STOPS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_STOP: &str = "unused_stop";

#[derive(Debug, Default)]
pub struct UnusedStopValidator;

impl Validator for UnusedStopValidator {
    fn name(&self) -> &'static str {
        "unused_stop"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut used_stop_ids: HashSet<&str> = HashSet::new();
        for stop_time in &feed.stop_times.rows {
            let stop_id = stop_time.stop_id.trim();
            if !stop_id.is_empty() {
                used_stop_ids.insert(stop_id);
            }
        }

        // Also check parent stations of used stops
        let mut all_used_ids = used_stop_ids.clone();
        let stops_by_id: std::collections::HashMap<&str, &gtfs_model::Stop> = feed
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
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "stopId".to_string(),
                    "stopName".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

