use std::collections::HashMap;

use crate::feed::STOP_TIMES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_DUPLICATE_KEY: &str = "duplicate_key";

#[derive(Debug, Default)]
pub struct DuplicateStopSequenceValidator;

impl Validator for DuplicateStopSequenceValidator {
    fn name(&self) -> &'static str {
        "duplicate_stop_sequence"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut seen: HashMap<(String, u32), u64> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            let key = (trip_id.to_string(), stop_time.stop_sequence);
            if let Some(previous_row) = seen.get(&key) {
                let mut notice = ValidationNotice::new(
                    CODE_DUPLICATE_KEY,
                    NoticeSeverity::Error,
                    "duplicate key",
                );
                notice.insert_context_field("fieldName1", "trip_id");
                notice.insert_context_field("fieldName2", "stop_sequence");
                notice.insert_context_field("fieldValue1", trip_id);
                notice.insert_context_field("fieldValue2", stop_time.stop_sequence);
                notice.insert_context_field("filename", STOP_TIMES_FILE);
                notice.insert_context_field("newCsvRowNumber", row_number);
                notice.insert_context_field("oldCsvRowNumber", *previous_row);
                notice.field_order = vec![
                    "fieldName1".to_string(),
                    "fieldName2".to_string(),
                    "fieldValue1".to_string(),
                    "fieldValue2".to_string(),
                    "filename".to_string(),
                    "newCsvRowNumber".to_string(),
                    "oldCsvRowNumber".to_string(),
                ];
                notices.push(notice);
            } else {
                seen.insert(key, row_number);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::StopTime;

    #[test]
    fn detects_duplicate_stop_sequence() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_sequence".to_string()],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateStopSequenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_DUPLICATE_KEY);
    }

    #[test]
    fn passes_with_unique_sequences() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_sequence".to_string()],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateStopSequenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn allows_same_sequence_different_trips() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_sequence".to_string()],
            rows: vec![
                StopTime {
                    trip_id: "T1".to_string(),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: "T2".to_string(),
                    stop_sequence: 1,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        DuplicateStopSequenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
