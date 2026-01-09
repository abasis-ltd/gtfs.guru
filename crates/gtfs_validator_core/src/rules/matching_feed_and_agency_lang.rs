use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;

const CODE_FEED_INFO_LANG_AND_AGENCY_LANG_MISMATCH: &str =
    "feed_info_lang_and_agency_lang_mismatch";

#[derive(Debug, Default)]
pub struct MatchingFeedAndAgencyLangValidator;

impl Validator for MatchingFeedAndAgencyLangValidator {
    fn name(&self) -> &'static str {
        "matching_feed_and_agency_lang"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(feed_info) = &feed.feed_info else {
            return;
        };
        let Some(info) = feed_info.rows.first() else {
            return;
        };
        let row_number = 2;
        let feed_lang_value = feed.pool.resolve(info.feed_lang);
        let feed_lang = feed_lang_value.trim();
        if feed_lang.is_empty() {
            return;
        }

        let feed_lang_normalized = feed_lang.to_ascii_lowercase();
        if feed_lang_normalized == "mul" {
            return;
        }

        for agency in &feed.agency.rows {
            let Some(agency_lang) = agency.agency_lang else {
                continue;
            };
            let agency_lang_value = feed.pool.resolve(agency_lang);
            let agency_lang = agency_lang_value.trim();
            if agency_lang.is_empty() {
                continue;
            }
            if agency_lang.to_ascii_lowercase() != feed_lang_normalized {
                let agency_id_value = feed.pool.resolve(agency.agency_id.unwrap_or(StringId(0)));
                let mut notice = ValidationNotice::new(
                    CODE_FEED_INFO_LANG_AND_AGENCY_LANG_MISMATCH,
                    NoticeSeverity::Warning,
                    "agency_lang does not match feed_lang",
                );
                notice.insert_context_field("agencyId", agency_id_value.as_str());
                notice.insert_context_field("agencyLang", agency_lang);
                notice.insert_context_field("agencyName", agency.agency_name.as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("feedLang", feed_lang);
                notice.field_order = vec![
                    "agencyId".into(),
                    "agencyLang".into(),
                    "agencyName".into(),
                    "csvRowNumber".into(),
                    "feedLang".into(),
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

    #[test]
    fn emits_notice_for_mismatched_languages() {
        let mut feed = base_feed();
        feed.feed_info = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::FeedInfo {
                feed_publisher_name: "Publisher".into(),
                feed_publisher_url: feed.pool.intern("https://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some(feed.pool.intern("fr"));

        let mut notices = NoticeContainer::new();
        MatchingFeedAndAgencyLangValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_FEED_INFO_LANG_AND_AGENCY_LANG_MISMATCH);
        assert_eq!(context_str(notice, "agencyId"), "A1");
        assert_eq!(context_str(notice, "agencyLang"), "fr");
        assert_eq!(context_str(notice, "agencyName"), "Agency");
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "feedLang"), "en");
    }

    #[test]
    fn skips_when_feed_lang_is_mul() {
        let mut feed = base_feed();
        feed.feed_info = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::FeedInfo {
                feed_publisher_name: "Publisher".into(),
                feed_publisher_url: feed.pool.intern("https://example.com"),
                feed_lang: feed.pool.intern("mul"),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some(feed.pool.intern("fr"));

        let mut notices = NoticeContainer::new();
        MatchingFeedAndAgencyLangValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_when_languages_match() {
        let mut feed = base_feed();
        feed.feed_info = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::FeedInfo {
                feed_publisher_name: "Publisher".into(),
                feed_publisher_url: feed.pool.intern("https://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some(feed.pool.intern("EN"));

        let mut notices = NoticeContainer::new();
        MatchingFeedAndAgencyLangValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
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
        feed.stops = CsvTable::default();
        feed.routes = CsvTable::default();
        feed.trips = CsvTable::default();
        feed.stop_times = CsvTable::default();
        feed
    }

    fn context_str<'a>(notice: &'a ValidationNotice, key: &str) -> &'a str {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("")
    }

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }
}
