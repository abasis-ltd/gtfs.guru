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
        if feed.table_has_errors(TRIPS_FILE)
            || feed.table_has_errors(CALENDAR_FILE)
            || feed.table_has_errors(CALENDAR_DATES_FILE)
        {
            return;
        }
        if !feed
            .trips
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("service_id"))
        {
            return;
        }

        let mut service_ids: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        if let Some(calendar) = &feed.calendar {
            for row in &calendar.rows {
                let service_id = row.service_id;
                if service_id.0 != 0 {
                    service_ids.insert(service_id);
                }
            }
        }
        if let Some(calendar_dates) = &feed.calendar_dates {
            for row in &calendar_dates.rows {
                let service_id = row.service_id;
                if service_id.0 != 0 {
                    service_ids.insert(service_id);
                }
            }
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            let service_id = trip.service_id;
            if service_id.0 == 0 {
                continue;
            }
            if !service_ids.contains(&service_id) {
                let service_id_value = feed.pool.resolve(service_id);
                let mut notice = ValidationNotice::new(
                    CODE_FOREIGN_KEY_VIOLATION,
                    NoticeSeverity::Error,
                    "missing referenced service_id",
                );
                notice.insert_context_field("childFieldName", "service_id");
                notice.insert_context_field("childFilename", TRIPS_FILE);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldValue", service_id_value.as_str());
                notice.insert_context_field("parentFieldName", "service_id");
                notice.insert_context_field("parentFilename", "calendar.txt or calendar_dates.txt");
                notice.field_order = vec![
                    "childFieldName".into(),
                    "childFilename".into(),
                    "csvRowNumber".into(),
                    "fieldValue".into(),
                    "parentFieldName".into(),
                    "parentFilename".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Calendar, CalendarDate, Trip};

    #[test]
    fn detects_missing_service_id() {
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "service_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                service_id: feed.pool.intern("missing_service"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripServiceIdForeignKeyValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_FOREIGN_KEY_VIOLATION));
    }

    #[test]
    fn passes_service_id_in_calendar() {
        let mut feed = GtfsFeed::default();
        feed.calendar = Some(CsvTable {
            headers: vec!["service_id".into()],
            rows: vec![Calendar {
                service_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "service_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                service_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripServiceIdForeignKeyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_service_id_in_calendar_dates() {
        let mut feed = GtfsFeed::default();
        feed.calendar_dates = Some(CsvTable {
            headers: vec!["service_id".into()],
            rows: vec![CalendarDate {
                service_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });
        feed.trips = CsvTable {
            headers: vec!["trip_id".into(), "service_id".into()],
            rows: vec![Trip {
                trip_id: feed.pool.intern("T1"),
                service_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        TripServiceIdForeignKeyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
