use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_FEED_INFO_DATE: &str = "missing_feed_info_date";

#[derive(Debug, Default)]
pub struct FeedServiceDateValidator;

impl Validator for FeedServiceDateValidator {
    fn name(&self) -> &'static str {
        "feed_service_date"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(feed_info) = &feed.feed_info else {
            return;
        };

        for (index, info) in feed_info.rows.iter().enumerate() {
            let row_number = feed_info.row_number(index);
            match (info.feed_start_date.is_some(), info.feed_end_date.is_some()) {
                (true, false) => {
                    notices.push(missing_feed_info_date_notice("feed_end_date", row_number))
                }
                (false, true) => {
                    notices.push(missing_feed_info_date_notice("feed_start_date", row_number))
                }
                _ => {}
            }
        }
    }
}

fn missing_feed_info_date_notice(field: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_FEED_INFO_DATE,
        NoticeSeverity::Warning,
        format!("missing {}", field),
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{FeedInfo, GtfsDate};

    #[test]
    fn detects_missing_end_date() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into(), "feed_start_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: Some(GtfsDate::parse("20240101").unwrap()),
                feed_end_date: None,
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedServiceDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_FEED_INFO_DATE
        );
        assert!(notices
            .iter()
            .next()
            .unwrap()
            .message
            .contains("feed_end_date"));
    }

    #[test]
    fn detects_missing_start_date() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into(), "feed_end_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: Some(GtfsDate::parse("20240101").unwrap()),
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedServiceDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert!(notices
            .iter()
            .next()
            .unwrap()
            .message
            .contains("feed_start_date"));
    }

    #[test]
    fn passes_both_dates_present() {
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec![
                "feed_publisher_name".into(),
                "feed_start_date".into(),
                "feed_end_date".into(),
            ],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: Some(GtfsDate::parse("20240101").unwrap()),
                feed_end_date: Some(GtfsDate::parse("20241231").unwrap()),
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedServiceDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
