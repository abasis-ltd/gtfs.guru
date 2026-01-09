use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_ONLY_START_OR_END: &str = "timeframe_only_start_or_end_time_specified";
const CODE_TIME_GREATER_THAN_24: &str =
    "timeframe_start_or_end_time_greater_than_twenty_four_hours";
const TWENTY_FOUR_HOURS_SECONDS: i32 = 24 * 3600;

#[derive(Debug, Default)]
pub struct TimeframeStartAndEndTimeValidator;

impl Validator for TimeframeStartAndEndTimeValidator {
    fn name(&self) -> &'static str {
        "timeframe_start_and_end_time"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(timeframes) = &feed.timeframes else {
            return;
        };

        for (index, timeframe) in timeframes.rows.iter().enumerate() {
            let row_number = timeframes.row_number(index);
            let has_start = timeframe.start_time.is_some();
            let has_end = timeframe.end_time.is_some();
            if has_start != has_end {
                let mut notice = ValidationNotice::new(
                    CODE_ONLY_START_OR_END,
                    NoticeSeverity::Error,
                    "start_time and end_time must both be set or both empty",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into()];
                notices.push(notice);
            }

            if let Some(start_time) = timeframe.start_time {
                if start_time.total_seconds() > TWENTY_FOUR_HOURS_SECONDS {
                    notices.push(time_greater_than_24_notice(
                        "start_time",
                        start_time,
                        row_number,
                    ));
                }
            }
            if let Some(end_time) = timeframe.end_time {
                if end_time.total_seconds() > TWENTY_FOUR_HOURS_SECONDS {
                    notices.push(time_greater_than_24_notice(
                        "end_time", end_time, row_number,
                    ));
                }
            }
        }
    }
}

fn time_greater_than_24_notice(
    field: &str,
    time: gtfs_guru_model::GtfsTime,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_TIME_GREATER_THAN_24,
        NoticeSeverity::Error,
        "timeframe time is greater than 24:00:00",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("time", time.to_string());
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "time".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, Timeframe};

    #[test]
    fn detects_only_one_time_specified() {
        let feed = GtfsFeed {
            timeframes: Some(CsvTable {
                headers: vec!["start_time".into()],
                rows: vec![Timeframe {
                    start_time: Some(GtfsTime::from_seconds(3600)),
                    end_time: None,
                    ..Default::default()
                }],
                row_numbers: vec![2],
            }),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        TimeframeStartAndEndTimeValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_ONLY_START_OR_END));
    }

    #[test]
    fn detects_time_greater_than_24() {
        let feed = GtfsFeed {
            timeframes: Some(CsvTable {
                headers: vec!["start_time".into(), "end_time".into()],
                rows: vec![Timeframe {
                    start_time: Some(GtfsTime::from_seconds(25 * 3600)),
                    end_time: Some(GtfsTime::from_seconds(26 * 3600)),
                    ..Default::default()
                }],
                row_numbers: vec![2],
            }),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        TimeframeStartAndEndTimeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 2);
        assert!(notices.iter().all(|n| n.code == CODE_TIME_GREATER_THAN_24));
    }

    #[test]
    fn passes_valid_times() {
        let feed = GtfsFeed {
            timeframes: Some(CsvTable {
                headers: vec!["start_time".into(), "end_time".into()],
                rows: vec![Timeframe {
                    start_time: Some(GtfsTime::from_seconds(3600)),
                    end_time: Some(GtfsTime::from_seconds(7200)),
                    ..Default::default()
                }],
                row_numbers: vec![2],
            }),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        TimeframeStartAndEndTimeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
