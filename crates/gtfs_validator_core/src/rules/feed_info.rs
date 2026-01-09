use crate::feed::FEED_INFO_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_START_AND_END_RANGE_OUT_OF_ORDER: &str = "start_and_end_range_out_of_order";
const CODE_MISSING_FEED_CONTACT_EMAIL_AND_URL: &str = "missing_feed_contact_email_and_url";
const CODE_MORE_THAN_ONE_ENTITY: &str = "more_than_one_entity";

#[derive(Debug, Default)]
pub struct MissingFeedInfoValidator;

impl Validator for MissingFeedInfoValidator {
    fn name(&self) -> &'static str {
        "missing_feed_info"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.feed_info.is_some() {
            return;
        }

        if feed.translations.is_some() {
            notices.push_missing_file(FEED_INFO_FILE);
        } else {
            notices.push_missing_recommended_file(FEED_INFO_FILE);
        }
    }
}

#[derive(Debug, Default)]
pub struct FeedContactValidator;

impl Validator for FeedContactValidator {
    fn name(&self) -> &'static str {
        "feed_contact"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(feed_info) = &feed.feed_info else {
            return;
        };

        for (index, info) in feed_info.rows.iter().enumerate() {
            let row_number = feed_info.row_number(index);
            if is_blank(info.feed_contact_email.as_deref())
                && info.feed_contact_url.map(|id| id.0 == 0).unwrap_or(true)
            {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_FEED_CONTACT_EMAIL_AND_URL,
                    NoticeSeverity::Warning,
                    "missing feed_contact_email and feed_contact_url",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into()];
                notices.push(notice);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct FeedInfoValidator;

impl Validator for FeedInfoValidator {
    fn name(&self) -> &'static str {
        "feed_info_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(feed_info) = &feed.feed_info {
            if feed_info.rows.len() > 1 {
                notices.push(more_than_one_entity_notice(feed_info.rows.len()));
            }
            for (index, info) in feed_info.rows.iter().enumerate() {
                let row_number = feed_info.row_number(index);
                if let (Some(start), Some(end)) = (info.feed_start_date, info.feed_end_date) {
                    if start > end {
                        let mut notice = ValidationNotice::new(
                            CODE_START_AND_END_RANGE_OUT_OF_ORDER,
                            NoticeSeverity::Error,
                            "feed_start_date must be <= feed_end_date",
                        );
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("endFieldName", "feed_end_date");
                        notice.insert_context_field("endValue", end.to_string());
                        notice.insert_context_field("entityId", info.feed_publisher_name.trim());
                        notice.insert_context_field("filename", FEED_INFO_FILE);
                        notice.insert_context_field("startFieldName", "feed_start_date");
                        notice.insert_context_field("startValue", start.to_string());
                        notice.field_order = vec![
                            "csvRowNumber".into(),
                            "endFieldName".into(),
                            "endValue".into(),
                            "entityId".into(),
                            "filename".into(),
                            "startFieldName".into(),
                            "startValue".into(),
                        ];
                        notices.push(notice);
                    }
                }
            }
        }
    }
}

fn more_than_one_entity_notice(count: usize) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MORE_THAN_ONE_ENTITY,
        NoticeSeverity::Warning,
        "more than one entity in file",
    );
    notice.insert_context_field("entityCount", count);
    notice.insert_context_field("filename", FEED_INFO_FILE);
    notice.field_order = vec!["entityCount".into(), "filename".into()];
    notice
}

fn is_blank(value: Option<&str>) -> bool {
    value.map(|val| val.trim().is_empty()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{FeedInfo, GtfsDate};

    #[test]
    fn detects_start_date_after_end_date() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: Some(GtfsDate::parse("20250601").unwrap()),
                feed_end_date: Some(GtfsDate::parse("20250101").unwrap()),
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedInfoValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_START_AND_END_RANGE_OUT_OF_ORDER
        );
    }

    #[test]
    fn detects_missing_contact() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedContactValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_FEED_CONTACT_EMAIL_AND_URL
        );
    }

    #[test]
    fn detects_more_than_one_feed_info() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into()],
            rows: vec![
                FeedInfo {
                    feed_publisher_name: "Test1".into(),
                    feed_publisher_url: feed.pool.intern("http://example1.com"),
                    feed_lang: feed.pool.intern("en"),
                    feed_start_date: None,
                    feed_end_date: None,
                    feed_version: None,
                    feed_contact_email: Some("test@test.com".into()),
                    feed_contact_url: None,
                    default_lang: None,
                },
                FeedInfo {
                    feed_publisher_name: "Test2".into(),
                    feed_publisher_url: feed.pool.intern("http://example2.com"),
                    feed_lang: feed.pool.intern("en"),
                    feed_start_date: None,
                    feed_end_date: None,
                    feed_version: None,
                    feed_contact_email: Some("test@test.com".into()),
                    feed_contact_url: None,
                    default_lang: None,
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        FeedInfoValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MORE_THAN_ONE_ENTITY
        );
    }

    #[test]
    fn passes_with_valid_feed_info() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: Some(GtfsDate::parse("20250101").unwrap()),
                feed_end_date: Some(GtfsDate::parse("20251231").unwrap()),
                feed_version: None,
                feed_contact_email: Some("test@test.com".into()),
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedInfoValidator.validate(&feed, &mut notices);
        FeedContactValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
