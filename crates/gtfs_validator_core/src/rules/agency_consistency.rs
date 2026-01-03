use crate::feed::AGENCY_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";
const CODE_MISSING_RECOMMENDED_FIELD: &str = "missing_recommended_field";
const CODE_INCONSISTENT_AGENCY_TIMEZONE: &str = "inconsistent_agency_timezone";
const CODE_INCONSISTENT_AGENCY_LANG: &str = "inconsistent_agency_lang";

#[derive(Debug, Default)]
pub struct AgencyConsistencyValidator;

impl Validator for AgencyConsistencyValidator {
    fn name(&self) -> &'static str {
        "agency_consistency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let agency_count = feed.agency.rows.len();
        if agency_count == 0 {
            return;
        }

        if agency_count == 1 {
            let agency = &feed.agency.rows[0];
            if !has_value(agency.agency_id.as_deref()) {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_RECOMMENDED_FIELD,
                    NoticeSeverity::Warning,
                    "agency_id is recommended when only one agency exists",
                );
                notice.insert_context_field("csvRowNumber", 2_u64);
                notice.insert_context_field("fieldName", "agency_id");
                notice.insert_context_field("filename", AGENCY_FILE);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "fieldName".to_string(),
                    "filename".to_string(),
                ];
                notices.push(notice);
            }
            return;
        }

        for (index, agency) in feed.agency.rows.iter().enumerate() {
            if !has_value(agency.agency_id.as_deref()) {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_REQUIRED_FIELD,
                    NoticeSeverity::Error,
                    "agency_id is required when multiple agencies exist",
                );
                notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                notice.insert_context_field("fieldName", "agency_id");
                notice.insert_context_field("filename", AGENCY_FILE);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "fieldName".to_string(),
                    "filename".to_string(),
                ];
                notices.push(notice);
            }
        }

        let common_timezone = feed.agency.rows[0].agency_timezone.trim();
        for (index, agency) in feed.agency.rows.iter().enumerate().skip(1) {
            let timezone = agency.agency_timezone.trim();
            if common_timezone != timezone {
                let mut notice = ValidationNotice::new(
                    CODE_INCONSISTENT_AGENCY_TIMEZONE,
                    NoticeSeverity::Error,
                    "agencies have inconsistent timezones",
                );
                notice.insert_context_field("actual", timezone);
                notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                notice.insert_context_field("expected", common_timezone);
                notice.field_order = vec![
                    "actual".to_string(),
                    "csvRowNumber".to_string(),
                    "expected".to_string(),
                ];
                notices.push(notice);
            }
        }

        let mut common_lang: Option<String> = None;
        for (index, agency) in feed.agency.rows.iter().enumerate() {
            let Some(lang) = agency.agency_lang.as_deref() else {
                continue;
            };
            let lang = lang.trim();
            if lang.is_empty() {
                continue;
            }
            let normalized = lang.to_ascii_lowercase();
            match common_lang.as_ref() {
                None => common_lang = Some(normalized),
                Some(existing) if existing != &normalized => {
                    let mut notice = ValidationNotice::new(
                        CODE_INCONSISTENT_AGENCY_LANG,
                        NoticeSeverity::Warning,
                        "agencies have inconsistent languages",
                    );
                    notice.insert_context_field("actual", normalized.as_str());
                    notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                    notice.insert_context_field("expected", existing.as_str());
                    notice.field_order = vec![
                        "actual".to_string(),
                        "csvRowNumber".to_string(),
                        "expected".to_string(),
                    ];
                    notices.push(notice);
                }
                _ => {}
            }
        }
    }
}

fn has_value(value: Option<&str>) -> bool {
    value.map(|val| !val.trim().is_empty()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn warns_when_single_agency_missing_id() {
        let mut feed = base_feed();
        feed.agency.rows[0].agency_id = None;

        let mut notices = NoticeContainer::new();
        AgencyConsistencyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_MISSING_RECOMMENDED_FIELD);
        assert_eq!(notice.severity, NoticeSeverity::Warning);
    }

    #[test]
    fn errors_when_multiple_agencies_missing_id() {
        let mut feed = base_feed();
        feed.agency.rows.push(gtfs_model::Agency {
            agency_id: None,
            agency_name: "Agency2".to_string(),
            agency_url: "https://example.com".to_string(),
            agency_timezone: "UTC".to_string(),
            agency_lang: None,
            agency_phone: None,
            agency_fare_url: None,
            agency_email: None,
        });
        feed.agency.rows[0].agency_id = None;

        let mut notices = NoticeContainer::new();
        AgencyConsistencyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 2);
        assert!(notices
            .iter()
            .all(|notice| notice.code == CODE_MISSING_REQUIRED_FIELD));
    }

    #[test]
    fn errors_when_timezones_inconsistent() {
        let mut feed = base_feed();
        feed.agency.rows.push(gtfs_model::Agency {
            agency_id: Some("A2".to_string()),
            agency_name: "Agency2".to_string(),
            agency_url: "https://example.com".to_string(),
            agency_timezone: "Europe/Paris".to_string(),
            agency_lang: None,
            agency_phone: None,
            agency_fare_url: None,
            agency_email: None,
        });

        let mut notices = NoticeContainer::new();
        AgencyConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_INCONSISTENT_AGENCY_TIMEZONE));
    }

    #[test]
    fn warns_when_languages_inconsistent() {
        let mut feed = base_feed();
        feed.agency.rows[0].agency_lang = Some("en".to_string());
        feed.agency.rows.push(gtfs_model::Agency {
            agency_id: Some("A2".to_string()),
            agency_name: "Agency2".to_string(),
            agency_url: "https://example.com".to_string(),
            agency_timezone: "UTC".to_string(),
            agency_lang: Some("fr".to_string()),
            agency_phone: None,
            agency_fare_url: None,
            agency_email: None,
        });

        let mut notices = NoticeContainer::new();
        AgencyConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == CODE_INCONSISTENT_AGENCY_LANG));
    }

    fn base_feed() -> GtfsFeed {
        GtfsFeed {
            agency: CsvTable {
                headers: Vec::new(),
                rows: vec![gtfs_model::Agency {
                    agency_id: Some("A1".to_string()),
                    agency_name: "Agency".to_string(),
                    agency_url: "https://example.com".to_string(),
                    agency_timezone: "UTC".to_string(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: Vec::new(),
            },
            stops: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            routes: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            trips: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            stop_times: CsvTable {
                headers: Vec::new(),
                rows: Vec::new(),
                row_numbers: Vec::new(),
            },
            calendar: None,
            calendar_dates: None,
            fare_attributes: None,
            fare_rules: None,
            fare_media: None,
            fare_products: None,
            fare_leg_rules: None,
            fare_transfer_rules: None,
            fare_leg_join_rules: None,
            areas: None,
            stop_areas: None,
            timeframes: None,
            rider_categories: None,
            shapes: None,
            frequencies: None,
            transfers: None,
            location_groups: None,
            location_group_stops: None,
            locations: None,
            booking_rules: None,
            feed_info: None,
            attributions: None,
            levels: None,
            pathways: None,
            translations: None,
            networks: None,
            route_networks: None,
        }
    }
}
