use crate::feed::AGENCY_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;

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
            if !has_value(agency.agency_id) {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_RECOMMENDED_FIELD,
                    NoticeSeverity::Warning,
                    "agency_id is recommended when only one agency exists",
                );
                notice.insert_context_field("csvRowNumber", 2_u64);
                notice.insert_context_field("fieldName", "agency_id");
                notice.insert_context_field("filename", AGENCY_FILE);
                notice.field_order =
                    vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
                notices.push(notice);
            }
            return;
        }

        for (index, agency) in feed.agency.rows.iter().enumerate() {
            if !has_value(agency.agency_id) {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_REQUIRED_FIELD,
                    NoticeSeverity::Error,
                    "agency_id is required when multiple agencies exist",
                );
                notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                notice.insert_context_field("fieldName", "agency_id");
                notice.insert_context_field("filename", AGENCY_FILE);
                notice.field_order =
                    vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
                notices.push(notice);
            }
        }

        let common_timezone_id = feed.agency.rows[0].agency_timezone;
        for (index, agency) in feed.agency.rows.iter().enumerate().skip(1) {
            if common_timezone_id != agency.agency_timezone {
                let timezone = feed.pool.resolve(agency.agency_timezone);
                let timezone = timezone.trim();
                let common_timezone = feed.pool.resolve(common_timezone_id);
                let common_timezone = common_timezone.trim();
                let mut notice = ValidationNotice::new(
                    CODE_INCONSISTENT_AGENCY_TIMEZONE,
                    NoticeSeverity::Error,
                    "agencies have inconsistent timezones",
                );
                notice.insert_context_field("actual", timezone);
                notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                notice.insert_context_field("expected", common_timezone);
                notice.field_order =
                    vec!["actual".into(), "csvRowNumber".into(), "expected".into()];
                notices.push(notice);
            }
        }

        let mut common_lang: Option<String> = None;
        for (index, agency) in feed.agency.rows.iter().enumerate() {
            let Some(lang) = agency.agency_lang else {
                continue;
            };
            let lang_value = feed.pool.resolve(lang);
            let lang = lang_value.trim();
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
                    notice.field_order =
                        vec!["actual".into(), "csvRowNumber".into(), "expected".into()];
                    notices.push(notice);
                }
                _ => {}
            }
        }
    }
}

fn has_value(value: Option<StringId>) -> bool {
    matches!(value, Some(id) if id.0 != 0)
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
        feed.agency.rows.push(gtfs_guru_model::Agency {
            agency_id: None,
            agency_name: "Agency2".into(),
            agency_url: feed.pool.intern("https://example.com"),
            agency_timezone: feed.pool.intern("UTC"),
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
        feed.agency.rows.push(gtfs_guru_model::Agency {
            agency_id: Some(feed.pool.intern("A2")),
            agency_name: "Agency2".into(),
            agency_url: feed.pool.intern("https://example.com"),
            agency_timezone: feed.pool.intern("Europe/Paris"),
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
        feed.agency.rows[0].agency_lang = Some(feed.pool.intern("en"));
        feed.agency.rows.push(gtfs_guru_model::Agency {
            agency_id: Some(feed.pool.intern("A2")),
            agency_name: "Agency2".into(),
            agency_url: feed.pool.intern("https://example.com"),
            agency_timezone: feed.pool.intern("UTC"),
            agency_lang: Some(feed.pool.intern("fr")),
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
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency".into(),
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                agency_lang: None,
                agency_phone: None,
                agency_fare_url: None,
                agency_email: None,
            }],
            row_numbers: Vec::new(),
        };
        feed
    }
}
