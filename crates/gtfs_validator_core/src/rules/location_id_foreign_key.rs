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
        if !feed
            .stop_times
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("location_id"))
        {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let Some(location_id) = stop_time
                .location_id
                .as_deref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            if !locations.location_ids.contains(location_id) {
                notices.push(missing_ref_notice(
                    location_id,
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
        "childFieldName".to_string(),
        "childFilename".to_string(),
        "csvRowNumber".to_string(),
        "fieldValue".to_string(),
        "parentFieldName".to_string(),
        "parentFilename".to_string(),
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
            headers: vec!["stop_id".to_string(), "location_id".to_string()],
            rows: vec![StopTime {
                stop_id: "S1".to_string(),
                location_id: Some("L1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        let mut locations = LocationsGeoJson::default();
        locations.location_ids = HashSet::from(["L2".to_string()]);
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
            headers: vec!["stop_id".to_string(), "location_id".to_string()],
            rows: vec![StopTime {
                stop_id: "S1".to_string(),
                location_id: Some("L1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        let mut locations = LocationsGeoJson::default();
        locations.location_ids = HashSet::from(["L1".to_string()]);
        feed.locations = Some(locations);

        let mut notices = NoticeContainer::new();
        LocationIdForeignKeyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn skips_missing_header() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".to_string()],
            rows: vec![StopTime {
                stop_id: "S1".to_string(),
                location_id: Some("L1".to_string()),
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
