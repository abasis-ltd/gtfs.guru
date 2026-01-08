use crate::feed::{LOCATIONS_GEOJSON_FILE, STOP_TIMES_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_FOREIGN_KEY_VIOLATION: &str = "foreign_key_violation";

#[derive(Debug, Default)]
pub struct LocationIdForeignKeyValidator;

impl Validator for LocationIdForeignKeyValidator {
    fn name(&self) -> &'static str {
        "location_id_foreign_key"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(locations) = &feed.locations else {
            return;
        };
        if locations.has_fatal_errors() {
            return;
        }
        if feed.table_has_errors(STOP_TIMES_FILE) {
            return;
        }
        if !feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("location_id"))
        {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let Some(location_id) = stop_time.location_id.filter(|id| id.0 != 0) else {
                continue;
            };

            if !locations.location_ids.contains(&location_id) {
                let location_id_value = feed.pool.resolve(location_id);
                notices.push(missing_ref_notice(
                    location_id_value.as_str(),
                    feed.stop_times.row_number(index),
                ));
            }
        }
    }
}

fn missing_ref_notice(location_id: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FOREIGN_KEY_VIOLATION,
        NoticeSeverity::Error,
        format!("missing referenced id {}", location_id),
    );
    notice.row = Some(row_number);
    notice.field_order = vec![
        "childFieldName".into(),
        "childFilename".into(),
        "csvRowNumber".into(),
        "fieldValue".into(),
        "parentFieldName".into(),
        "parentFilename".into(),
    ];
    notice.insert_context_field("childFieldName", "location_id");
    notice.insert_context_field("childFilename", STOP_TIMES_FILE);
    notice.insert_context_field("parentFieldName", "id");
    notice.insert_context_field("parentFilename", LOCATIONS_GEOJSON_FILE);
    notice.insert_context_field("fieldValue", location_id);
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geojson::LocationsGeoJson;
    use crate::{CsvTable, GtfsFeed, NoticeContainer};
    use gtfs_guru_model::StopTime;
    use std::collections::HashSet;

    #[test]
    fn detects_missing_location_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into(), "location_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: Some(feed.pool.intern("L1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        let mut locations = LocationsGeoJson::default();
        locations.location_ids = HashSet::from([feed.pool.intern("L2")]);
        feed.locations = Some(locations);

        let mut notices = NoticeContainer::new();
        LocationIdForeignKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FOREIGN_KEY_VIOLATION
        );
    }

    #[test]
    fn passes_valid_location_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into(), "location_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: Some(feed.pool.intern("L1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        let mut locations = LocationsGeoJson::default();
        locations.location_ids = HashSet::from([feed.pool.intern("L1")]);
        feed.locations = Some(locations);

        let mut notices = NoticeContainer::new();
        LocationIdForeignKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_missing_header() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                location_id: Some(feed.pool.intern("L1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.locations = Some(LocationsGeoJson::default());

        let mut notices = NoticeContainer::new();
        LocationIdForeignKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
