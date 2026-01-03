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
                    let mut notice = ValidationNotice::new(
                        CODE_START_AND_END_RANGE_OUT_OF_ORDER,
                        NoticeSeverity::Error,
                        "calendar start_date must be <= end_date",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("endFieldName", "end_date");
                    notice.insert_context_field("endValue", row.end_date.to_string());
                    notice.insert_context_field("entityId", row.service_id.trim());
                    notice.insert_context_field("filename", CALENDAR_FILE);
                    notice.insert_context_field("startFieldName", "start_date");
                    notice.insert_context_field("startValue", row.start_date.to_string());
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "endFieldName".to_string(),
                        "endValue".to_string(),
                        "entityId".to_string(),
                        "filename".to_string(),
                        "startFieldName".to_string(),
                        "startValue".to_string(),
                    ];
                    notices.push(notice);
                }
            }
        }

        if let Some(calendar_dates) = &feed.calendar_dates {
            let mut seen: HashMap<(String, String), u64> = HashMap::new();
            for (index, row) in calendar_dates.rows.iter().enumerate() {
                let row_number = calendar_dates.row_number(index);
                let service_id = row.service_id.trim();
                if service_id.is_empty() {
                    continue;
                }
                let key = (service_id.to_string(), row.date.to_string());
                if let Some(prev_row) = seen.get(&key) {
                    let mut notice = ValidationNotice::new(
                        CODE_DUPLICATE_KEY,
                        NoticeSeverity::Error,
                        "duplicate service_id/date in calendar_dates",
                    );
                    notice.insert_context_field("fieldName1", "service_id");
                    notice.insert_context_field("fieldName2", "date");
                    notice.insert_context_field("fieldValue1", service_id);
                    notice.insert_context_field("fieldValue2", row.date.to_string());
                    notice.insert_context_field("filename", CALENDAR_DATES_FILE);
                    notice.insert_context_field("newCsvRowNumber", row_number);
                    notice.insert_context_field("oldCsvRowNumber", *prev_row);
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
}

