use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::GtfsTime;

const CODE_TIMEFRAME_OVERLAP: &str = "timeframe_overlap";

#[derive(Debug, Default)]
pub struct TimeframeOverlapValidator;

impl Validator for TimeframeOverlapValidator {
    fn name(&self) -> &'static str {
        "timeframe_overlap"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(timeframes) = &feed.timeframes else {
            return;
        };

        let mut grouped: HashMap<(String, String), Vec<(u64, GtfsTime, GtfsTime)>> = HashMap::new();
        for (index, timeframe) in timeframes.rows.iter().enumerate() {
            let row_number = timeframes.row_number(index);
            let (Some(start_time), Some(end_time)) = (timeframe.start_time, timeframe.end_time)
            else {
                continue;
            };
            let group_id = timeframe
                .timeframe_group_id
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string();
            let service_id = timeframe.service_id.trim().to_string();
            grouped
                .entry((group_id, service_id))
                .or_default()
                .push((row_number, start_time, end_time));
        }

        for ((group_id, service_id), timeframes) in grouped.iter_mut() {
            timeframes.sort_by(|(_, start_a, end_a), (_, start_b, end_b)| {
                start_a
                    .total_seconds()
                    .cmp(&start_b.total_seconds())
                    .then_with(|| end_a.total_seconds().cmp(&end_b.total_seconds()))
            });
            for window in timeframes.windows(2) {
                let (prev_row, _prev_start, prev_end) = window[0];
                let (curr_row, curr_start, _) = window[1];
                if curr_start.total_seconds() < prev_end.total_seconds() {
                    let mut notice = ValidationNotice::new(
                        CODE_TIMEFRAME_OVERLAP,
                        NoticeSeverity::Error,
                        "timeframes overlap for same timeframe_group_id and service_id",
                    );
                    notice.insert_context_field("currCsvRowNumber", curr_row);
                    notice.insert_context_field("currStartTime", curr_start.to_string());
                    notice.insert_context_field("prevCsvRowNumber", prev_row);
                    notice.insert_context_field("prevEndTime", prev_end.to_string());
                    notice.insert_context_field("serviceId", service_id);
                    notice.insert_context_field("timeframeGroupId", group_id);
                    notice.field_order = vec![
                        "currCsvRowNumber".to_string(),
                        "currStartTime".to_string(),
                        "prevCsvRowNumber".to_string(),
                        "prevEndTime".to_string(),
                        "serviceId".to_string(),
                        "timeframeGroupId".to_string(),
                    ];
                    notices.push(notice);
                    break;
                }
            }
        }
    }
}

