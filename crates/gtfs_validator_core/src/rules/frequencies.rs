use crate::feed::FREQUENCIES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_START_AND_END_RANGE_OUT_OF_ORDER: &str = "start_and_end_range_out_of_order";
const CODE_START_AND_END_RANGE_EQUAL: &str = "start_and_end_range_equal";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Frequency, GtfsTime};

    #[test]
    fn detects_start_after_end() {
        let mut feed = GtfsFeed::default();
        feed.frequencies = Some(CsvTable {
            headers: vec!["trip_id".into(), "start_time".into(), "end_time".into()],
            rows: vec![Frequency {
                trip_id: feed.pool.intern("T1"),
                start_time: GtfsTime::parse("10:00:00").unwrap(),
                end_time: GtfsTime::parse("08:00:00").unwrap(),
                headway_secs: 600,
                exact_times: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FrequenciesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_START_AND_END_RANGE_OUT_OF_ORDER
        );
    }

    #[test]
    fn detects_start_equals_end() {
        let mut feed = GtfsFeed::default();
        feed.frequencies = Some(CsvTable {
            headers: vec!["trip_id".into(), "start_time".into(), "end_time".into()],
            rows: vec![Frequency {
                trip_id: feed.pool.intern("T1"),
                start_time: GtfsTime::parse("08:00:00").unwrap(),
                end_time: GtfsTime::parse("08:00:00").unwrap(),
                headway_secs: 600,
                exact_times: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FrequenciesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_START_AND_END_RANGE_EQUAL
        );
    }

    #[test]
    fn passes_with_valid_frequency() {
        let mut feed = GtfsFeed::default();
        feed.frequencies = Some(CsvTable {
            headers: vec!["trip_id".into(), "start_time".into(), "end_time".into()],
            rows: vec![Frequency {
                trip_id: feed.pool.intern("T1"),
                start_time: GtfsTime::parse("08:00:00").unwrap(),
                end_time: GtfsTime::parse("10:00:00").unwrap(),
                headway_secs: 600,
                exact_times: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FrequenciesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}

#[derive(Debug, Default)]
pub struct FrequenciesValidator;

impl Validator for FrequenciesValidator {
    fn name(&self) -> &'static str {
        "frequencies_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(frequencies) = &feed.frequencies {
            for (index, freq) in frequencies.rows.iter().enumerate() {
                let row_number = frequencies.row_number(index);
                let trip_id = freq.trip_id;
                let trip_id_value = feed.pool.resolve(trip_id);
                let start_value = freq.start_time.to_string();
                let end_value = freq.end_time.to_string();
                let start = freq.start_time.total_seconds();
                let end = freq.end_time.total_seconds();
                if start > end {
                    let mut notice = ValidationNotice::new(
                        CODE_START_AND_END_RANGE_OUT_OF_ORDER,
                        NoticeSeverity::Error,
                        "start_time must be < end_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("endFieldName", "end_time");
                    notice.insert_context_field("endValue", end_value);
                    notice.insert_context_field("entityId", trip_id_value.as_str());
                    notice.insert_context_field("filename", FREQUENCIES_FILE);
                    notice.insert_context_field("startFieldName", "start_time");
                    notice.insert_context_field("startValue", start_value);
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "endFieldName".into(),
                        "endValue".into(),
                        "entityId".into(),
                        "filename".into(),
                        "startFieldName".into(),
                        "startValue".into(),
                    ];
                    notices.push(notice);
                } else if start == end {
                    let mut notice = ValidationNotice::new(
                        CODE_START_AND_END_RANGE_EQUAL,
                        NoticeSeverity::Error,
                        "start_time must be different from end_time",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("endFieldName", "end_time");
                    notice.insert_context_field("entityId", trip_id_value.as_str());
                    notice.insert_context_field("filename", FREQUENCIES_FILE);
                    notice.insert_context_field("startFieldName", "start_time");
                    notice.insert_context_field("value", start_value);
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "endFieldName".into(),
                        "entityId".into(),
                        "filename".into(),
                        "startFieldName".into(),
                        "value".into(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}
