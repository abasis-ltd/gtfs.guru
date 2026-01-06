use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_OVERLAPPING_FREQUENCY: &str = "overlapping_frequency";

#[derive(Debug, Default)]
pub struct OverlappingFrequencyValidator;

impl Validator for OverlappingFrequencyValidator {
    fn name(&self) -> &'static str {
        "overlapping_frequency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(frequencies) = &feed.frequencies else {
            return;
        };

        let mut by_trip: HashMap<
            gtfs_guru_model::StringId,
            Vec<(u64, &gtfs_guru_model::Frequency)>,
        > = HashMap::new();
        for (index, freq) in frequencies.rows.iter().enumerate() {
            let row_number = frequencies.row_number(index);
            let trip_id = freq.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            by_trip.entry(trip_id).or_default().push((row_number, freq));
        }

        for freqs in by_trip.values_mut() {
            freqs.sort_by(|(_, a), (_, b)| {
                let start_cmp = a
                    .start_time
                    .total_seconds()
                    .cmp(&b.start_time.total_seconds());
                if start_cmp != std::cmp::Ordering::Equal {
                    return start_cmp;
                }
                let end_cmp = a.end_time.total_seconds().cmp(&b.end_time.total_seconds());
                if end_cmp != std::cmp::Ordering::Equal {
                    return end_cmp;
                }
                a.headway_secs.cmp(&b.headway_secs)
            });

            for window in freqs.windows(2) {
                let (prev_row, prev) = window[0];
                let (curr_row, curr) = window[1];
                if curr.start_time.total_seconds() < prev.end_time.total_seconds() {
                    let mut notice = ValidationNotice::new(
                        CODE_OVERLAPPING_FREQUENCY,
                        NoticeSeverity::Error,
                        "frequencies overlap for a trip",
                    );
                    notice.insert_context_field("currCsvRowNumber", curr_row);
                    notice.insert_context_field("currStartTime", curr.start_time);
                    notice.insert_context_field("prevCsvRowNumber", prev_row);
                    notice.insert_context_field("prevEndTime", prev.end_time);
                    notice.insert_context_field("tripId", feed.pool.resolve(curr.trip_id).as_str());
                    notice.field_order = vec![
                        "currCsvRowNumber".into(),
                        "currStartTime".into(),
                        "prevCsvRowNumber".into(),
                        "prevEndTime".into(),
                        "tripId".into(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Frequency, GtfsTime};

    #[test]
    fn detects_overlapping_frequency() {
        let mut feed = GtfsFeed::default();
        feed.frequencies = Some(CsvTable {
            headers: vec![
                "trip_id".into(),
                "start_time".into(),
                "end_time".into(),
                "headway_secs".into(),
            ],
            rows: vec![
                Frequency {
                    trip_id: feed.pool.intern("T1"),
                    start_time: GtfsTime::from_seconds(3600),
                    end_time: GtfsTime::from_seconds(7200),
                    headway_secs: 300,
                    ..Default::default()
                },
                Frequency {
                    trip_id: feed.pool.intern("T1"),
                    start_time: GtfsTime::from_seconds(7000), // Overlaps
                    end_time: GtfsTime::from_seconds(10000),
                    headway_secs: 300,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        OverlappingFrequencyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_OVERLAPPING_FREQUENCY
        );
    }

    #[test]
    fn passes_when_frequencies_dont_overlap() {
        let mut feed = GtfsFeed::default();
        feed.frequencies = Some(CsvTable {
            headers: vec![
                "trip_id".into(),
                "start_time".into(),
                "end_time".into(),
                "headway_secs".into(),
            ],
            rows: vec![
                Frequency {
                    trip_id: feed.pool.intern("T1"),
                    start_time: GtfsTime::from_seconds(3600),
                    end_time: GtfsTime::from_seconds(7200),
                    headway_secs: 300,
                    ..Default::default()
                },
                Frequency {
                    trip_id: feed.pool.intern("T1"),
                    start_time: GtfsTime::from_seconds(7200), // Starts at end of previous
                    end_time: GtfsTime::from_seconds(10000),
                    headway_secs: 300,
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        OverlappingFrequencyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
