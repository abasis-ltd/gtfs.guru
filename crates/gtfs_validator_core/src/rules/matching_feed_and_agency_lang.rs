use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

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
        let feed_lang = info.feed_lang.trim();
        if feed_lang.is_empty() {
            return;
        }

        let feed_lang_normalized = feed_lang.to_ascii_lowercase();
        if feed_lang_normalized == "mul" {
            return;
        }

        for agency in &feed.agency.rows {
            let Some(agency_lang) = agency.agency_lang.as_deref() else {
                continue;
            };
            let agency_lang = agency_lang.trim();
            if agency_lang.is_empty() {
                continue;
            }
            if agency_lang.to_ascii_lowercase() != feed_lang_normalized {
                let mut notice = ValidationNotice::new(
                    CODE_FEED_INFO_LANG_AND_AGENCY_LANG_MISMATCH,
                    NoticeSeverity::Warning,
                    "agency_lang does not match feed_lang",
                );
                notice.insert_context_field("agencyId", agency.agency_id.as_deref().unwrap_or(""));
                notice.insert_context_field("agencyLang", agency_lang);
                notice.insert_context_field("agencyName", agency.agency_name.as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("feedLang", feed_lang);
                notice.field_order = vec![
                    "agencyId".to_string(),
                    "agencyLang".to_string(),
                    "agencyName".to_string(),
                    "csvRowNumber".to_string(),
                    "feedLang".to_string(),
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
            rows: vec![gtfs_model::FeedInfo {
                feed_publisher_name: "Publisher".to_string(),
                feed_publisher_url: "https://example.com".to_string(),
                feed_lang: "en".to_string(),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some("fr".to_string());

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
            rows: vec![gtfs_model::FeedInfo {
                feed_publisher_name: "Publisher".to_string(),
                feed_publisher_url: "https://example.com".to_string(),
                feed_lang: "mul".to_string(),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some("fr".to_string());

        let mut notices = NoticeContainer::new();
        MatchingFeedAndAgencyLangValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_when_languages_match() {
        let mut feed = base_feed();
        feed.feed_info = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_model::FeedInfo {
                feed_publisher_name: "Publisher".to_string(),
                feed_publisher_url: "https://example.com".to_string(),
                feed_lang: "en".to_string(),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
            }],
            row_numbers: Vec::new(),
        });
        feed.agency.rows[0].agency_lang = Some("EN".to_string());

        let mut notices = NoticeContainer::new();
        MatchingFeedAndAgencyLangValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
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
