use std::collections::HashSet;

use crate::feed::{CALENDAR_DATES_FILE, CALENDAR_FILE, TRIPS_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

#[derive(Debug, Default)]
pub struct TripServiceIdForeignKeyValidator;

impl Validator for TripServiceIdForeignKeyValidator {
    fn name(&self) -> &'static str {
        "trip_service_id_foreign_key"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut service_ids: HashSet<&str> = HashSet::new();
        if let Some(calendar) = &feed.calendar {
            for row in &calendar.rows {
                let service_id = row.service_id.trim();
                if !service_id.is_empty() {
                    service_ids.insert(service_id);
                }
            }
        }
        if let Some(calendar_dates) = &feed.calendar_dates {
            for row in &calendar_dates.rows {
                let service_id = row.service_id.trim();
                if !service_id.is_empty() {
                    service_ids.insert(service_id);
                }
            }
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let service_id = trip.service_id.trim();
            if service_id.is_empty() || !service_ids.contains(service_id) {
                let mut notice = ValidationNotice::new(
                    CODE_FOREIGN_KEY_VIOLATION,
                    NoticeSeverity::Error,
                    "missing referenced service_id",
                );
                notice.insert_context_field("childFieldName", "service_id");
                notice.insert_context_field("childFilename", TRIPS_FILE);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldValue", service_id);
                notice.insert_context_field("parentFieldName", "service_id");
                notice.insert_context_field("parentFilename", "calendar.txt or calendar_dates.txt");
                notice.field_order = vec![
                    "childFieldName".to_string(),
                    "childFilename".to_string(),
                    "csvRowNumber".to_string(),
                    "fieldValue".to_string(),
                    "parentFieldName".to_string(),
                    "parentFilename".to_string(),
                ];
                notice.message = format!(
                    "missing referenced service_id in {} or {}",
                    CALENDAR_FILE, CALENDAR_DATES_FILE
                );
                notices.push(notice);
            }
        }
    }
}

