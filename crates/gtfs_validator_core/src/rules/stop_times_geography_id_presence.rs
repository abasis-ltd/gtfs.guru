use crate::feed::STOP_TIMES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";
const CODE_FORBIDDEN_GEOGRAPHY_ID: &str = "forbidden_geography_id";

#[derive(Debug, Default)]
pub struct StopTimesGeographyIdPresenceValidator;

impl Validator for StopTimesGeographyIdPresenceValidator {
    fn name(&self) -> &'static str {
        "stop_times_geography_id_presence"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let headers = &feed.stop_times.headers;
        let has_relevant_header =
            ["stop_id", "location_group_id", "location_id"]
                .iter()
                .any(|column| {
                    headers
                        .iter()
                        .any(|header| header.eq_ignore_ascii_case(column))
                });
        if !has_relevant_header {
            return;
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let has_stop_id = !stop_time.stop_id.trim().is_empty();
            let has_location_group_id = stop_time
                .location_group_id
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            let has_location_id = stop_time
                .location_id
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);

            let presence_count = [has_stop_id, has_location_group_id, has_location_id]
                .iter()
                .filter(|value| **value)
                .count();

            if presence_count == 0 {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_REQUIRED_FIELD,
                    NoticeSeverity::Error,
                    "stop_times requires one of stop_id, location_group_id, or location_id",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldName", "stop_id");
                notice.insert_context_field("filename", STOP_TIMES_FILE);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "fieldName".to_string(),
                    "filename".to_string(),
                ];
                notices.push(notice);
            } else if presence_count > 1 {
                let mut notice = ValidationNotice::new(
                    CODE_FORBIDDEN_GEOGRAPHY_ID,
                    NoticeSeverity::Error,
                    "stop_times must define only one of stop_id, location_group_id, or location_id",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field(
                    "locationGroupId",
                    stop_time.location_group_id.as_deref().unwrap_or(""),
                );
                notice.insert_context_field(
                    "locationId",
                    stop_time.location_id.as_deref().unwrap_or(""),
                );
                notice.insert_context_field("stopId", stop_time.stop_id.as_str());
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "locationGroupId".to_string(),
                    "locationId".to_string(),
                    "stopId".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::StopTime;

    #[test]
    fn detects_missing_geography_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_sequence".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_sequence: 1,
                stop_id: "".to_string(), // Empty
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // Add one of relevant headers to trigger validation
        feed.stop_times.headers.push("stop_id".to_string());

        let mut notices = NoticeContainer::new();
        StopTimesGeographyIdPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_REQUIRED_FIELD
        );
    }

    #[test]
    fn detects_forbidden_geography_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".to_string(),
                "stop_id".to_string(),
                "location_id".to_string(),
            ],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                location_id: Some("L1".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesGeographyIdPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FORBIDDEN_GEOGRAPHY_ID
        );
    }

    #[test]
    fn passes_valid_geography_id() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec!["trip_id".to_string(), "stop_id".to_string()],
            rows: vec![StopTime {
                trip_id: "T1".to_string(),
                stop_id: "S1".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        StopTimesGeographyIdPresenceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
