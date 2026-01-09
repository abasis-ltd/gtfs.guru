use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::GtfsTime;

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
                .map(|id| feed.pool.resolve(id))
                .unwrap_or_default()
                .trim()
                .to_string();
            let service_id = feed.pool.resolve(timeframe.service_id).trim().to_string();
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
                        "currCsvRowNumber".into(),
                        "currStartTime".into(),
                        "prevCsvRowNumber".into(),
                        "prevEndTime".into(),
                        "serviceId".into(),
                        "timeframeGroupId".into(),
                    ];
                    notices.push(notice);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::Timeframe;

    #[test]
    fn detects_timeframe_overlap() {
        let mut feed = GtfsFeed::default();
        feed.timeframes = Some(CsvTable {
            headers: vec![
                "timeframe_group_id".into(),
                "start_time".into(),
                "end_time".into(),
                "service_id".into(),
            ],
            rows: vec![
                Timeframe {
                    timeframe_group_id: Some(feed.pool.intern("G1")),
                    start_time: Some(GtfsTime::from_seconds(3600)),
                    end_time: Some(GtfsTime::from_seconds(7200)),
                    service_id: feed.pool.intern("S1"),
                },
                Timeframe {
                    timeframe_group_id: Some(feed.pool.intern("G1")),
                    start_time: Some(GtfsTime::from_seconds(7000)), // Overlaps
                    end_time: Some(GtfsTime::from_seconds(10000)),
                    service_id: feed.pool.intern("S1"),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        TimeframeOverlapValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, CODE_TIMEFRAME_OVERLAP);
    }

    #[test]
    fn passes_no_overlap() {
        let mut feed = GtfsFeed::default();
        feed.timeframes = Some(CsvTable {
            headers: vec![
                "timeframe_group_id".into(),
                "start_time".into(),
                "end_time".into(),
                "service_id".into(),
            ],
            rows: vec![
                Timeframe {
                    timeframe_group_id: Some(feed.pool.intern("G1")),
                    start_time: Some(GtfsTime::from_seconds(3600)),
                    end_time: Some(GtfsTime::from_seconds(7200)),
                    service_id: feed.pool.intern("S1"),
                },
                Timeframe {
                    timeframe_group_id: Some(feed.pool.intern("G1")),
                    start_time: Some(GtfsTime::from_seconds(7200)), // Starts at end of previous
                    end_time: Some(GtfsTime::from_seconds(10000)),
                    service_id: feed.pool.intern("S1"),
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        TimeframeOverlapValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
