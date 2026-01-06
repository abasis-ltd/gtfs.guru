use std::collections::HashMap;

use crate::feed::{CALENDAR_DATES_FILE, CALENDAR_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_START_AND_END_RANGE_OUT_OF_ORDER: &str = "start_and_end_range_out_of_order";
const CODE_DUPLICATE_KEY: &str = "duplicate_key";

#[derive(Debug, Default)]
pub struct CalendarValidator;

impl Validator for CalendarValidator {
    fn name(&self) -> &'static str {
        "calendar_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(calendar) = &feed.calendar {
            for (index, row) in calendar.rows.iter().enumerate() {
                let row_number = calendar.row_number(index);
                if row.start_date > row.end_date {
                    let service_id_value = feed.pool.resolve(row.service_id);
                    let mut notice = ValidationNotice::new(
                        CODE_START_AND_END_RANGE_OUT_OF_ORDER,
                        NoticeSeverity::Error,
                        "calendar start_date must be <= end_date",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("endFieldName", "end_date");
                    notice.insert_context_field("endValue", row.end_date.to_string());
                    notice.insert_context_field("entityId", service_id_value.as_str());
                    notice.insert_context_field("filename", CALENDAR_FILE);
                    notice.insert_context_field("startFieldName", "start_date");
                    notice.insert_context_field("startValue", row.start_date.to_string());
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
                }
            }
        }

        if let Some(calendar_dates) = &feed.calendar_dates {
            let mut seen: HashMap<(gtfs_guru_model::StringId, gtfs_guru_model::GtfsDate), u64> =
                HashMap::new();
            for (index, row) in calendar_dates.rows.iter().enumerate() {
                let row_number = calendar_dates.row_number(index);
                let service_id = row.service_id;
                if service_id.0 == 0 {
                    continue;
                }
                let key = (service_id, row.date);
                if let Some(prev_row) = seen.get(&key) {
                    let service_id_value = feed.pool.resolve(service_id);
                    let mut notice = ValidationNotice::new(
                        CODE_DUPLICATE_KEY,
                        NoticeSeverity::Error,
                        "duplicate service_id/date in calendar_dates",
                    );
                    notice.insert_context_field("fieldName1", "service_id");
                    notice.insert_context_field("fieldName2", "date");
                    notice.insert_context_field("fieldValue1", service_id_value.as_str());
                    notice.insert_context_field("fieldValue2", row.date.to_string());
                    notice.insert_context_field("filename", CALENDAR_DATES_FILE);
                    notice.insert_context_field("newCsvRowNumber", row_number);
                    notice.insert_context_field("oldCsvRowNumber", *prev_row);
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
                    seen.insert(key, row_number);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::GtfsDate;

    #[test]
    fn test_calendar_start_after_end() {
        let mut feed = GtfsFeed::default();
        feed.calendar = Some(CsvTable {
            rows: vec![gtfs_guru_model::Calendar {
                service_id: feed.pool.intern("S1"),
                start_date: GtfsDate::parse("20240102").unwrap(),
                end_date: GtfsDate::parse("20240101").unwrap(),
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        CalendarValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_START_AND_END_RANGE_OUT_OF_ORDER
        );
    }
}
