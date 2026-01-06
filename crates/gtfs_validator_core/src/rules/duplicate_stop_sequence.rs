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
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            let ctx = crate::ValidationContextState::capture();
            let new_notices: Vec<ValidationNotice> = feed
                .stop_times_by_trip
                .par_iter()
                .flat_map(|(trip_id, indices)| {
                    let _guards = ctx.apply();
                    Self::check_trip(feed, *trip_id, indices)
                })
                .collect();

            for notice in new_notices {
                notices.push(notice);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            for (trip_id, indices) in &feed.stop_times_by_trip {
                let trip_notices = Self::check_trip(feed, *trip_id, indices);
                for notice in trip_notices {
                    notices.push(notice);
                }
            }
        }
    }
}

impl DuplicateStopSequenceValidator {
    fn check_trip(
        feed: &GtfsFeed,
        trip_id: gtfs_guru_model::StringId,
        indices: &[usize],
    ) -> Vec<ValidationNotice> {
        let mut notices = Vec::new();
        let mut seen_sequences: HashMap<u32, u64> = HashMap::new();
        for &idx in indices {
            let stop_time = &feed.stop_times.rows[idx];
            let row_number = feed.stop_times.row_number(idx);
            let seq = stop_time.stop_sequence;
            if let Some(previous_row) = seen_sequences.get(&seq) {
                let trip_id_value = feed.pool.resolve(trip_id);
                let mut notice = ValidationNotice::new(
                    CODE_DUPLICATE_KEY,
                    NoticeSeverity::Error,
                    "duplicate key",
                );
                notice.insert_context_field("fieldName1", "trip_id");
                notice.insert_context_field("fieldName2", "stop_sequence");
                notice.insert_context_field("fieldValue1", trip_id_value.as_str());
                notice.insert_context_field("fieldValue2", seq);
                notice.insert_context_field("filename", STOP_TIMES_FILE);
                notice.insert_context_field("newCsvRowNumber", row_number);
                notice.insert_context_field("oldCsvRowNumber", *previous_row);
                notice.field_order = vec![
                    "fieldName1".into(),
                    "fieldName2".into(),
                    "fieldValue1".into(),
                    "fieldValue2".into(),
                    "filename".into(),
                    "newCsvRowNumber".into(),
                    "oldCsvRowNumber".into(),
                ];
                notices.push(notice);
            } else {
                seen_sequences.insert(seq, row_number);
            }
        }
        notices
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
            headers: vec!["trip_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

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
            headers: vec!["trip_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        DuplicateStopSequenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn allows_same_sequence_different_trips() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".into(), "stop_sequence".into()],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T2"),
                    stop_sequence: 1,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.rebuild_stop_times_index();

        let mut notices = NoticeContainer::new();
        DuplicateStopSequenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
