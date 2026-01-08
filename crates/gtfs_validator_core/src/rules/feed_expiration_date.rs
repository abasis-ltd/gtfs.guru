use chrono::{Duration, NaiveDate};

use crate::feed::FEED_INFO_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::GtfsDate;

const CODE_FEED_EXPIRATION_DATE_7_DAYS: &str = "feed_expiration_date7_days";
const CODE_FEED_EXPIRATION_DATE_30_DAYS: &str = "feed_expiration_date30_days";

#[derive(Debug, Default)]
pub struct FeedExpirationDateValidator;

impl Validator for FeedExpirationDateValidator {
    fn name(&self) -> &'static str {
        "feed_expiration_date"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(feed_info) = &feed.feed_info else {
            return;
        };

        let validation_date = crate::validation_date();
        let threshold_7_days = validation_date + Duration::days(7);
        let threshold_30_days = validation_date + Duration::days(30);

        for (index, info) in feed_info.rows.iter().enumerate() {
            let row_number = feed_info.row_number(index);
            let Some(feed_end_date_raw) = info.feed_end_date else {
                continue;
            };
            let Some(feed_end_date) = gtfs_date_to_naive(feed_end_date_raw) else {
                continue;
            };

            if feed_end_date <= threshold_7_days {
                notices.push(expiration_notice_7_days(row_number));
            } else if feed_end_date <= threshold_30_days {
                notices.push(expiration_notice_30_days(row_number));
            }
        }
    }
}

fn gtfs_date_to_naive(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

fn expiration_notice_7_days(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FEED_EXPIRATION_DATE_7_DAYS,
        NoticeSeverity::Warning,
        "feed_end_date is within 7 days of current date",
    );
    populate_expiration_notice(&mut notice, row_number);
    notice
}

fn expiration_notice_30_days(row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_FEED_EXPIRATION_DATE_30_DAYS,
        NoticeSeverity::Warning,
        "feed_end_date is within 30 days of current date",
    );
    populate_expiration_notice(&mut notice, row_number);
    notice
}

fn populate_expiration_notice(notice: &mut ValidationNotice, row_number: u64) {
    notice.set_location(FEED_INFO_FILE, "feed_end_date", row_number);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::FeedInfo;

    #[test]
    fn detects_expiration_within_7_days() {
        let _guard =
            crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2024, 5, 18).unwrap()));
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into(), "feed_end_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: Some(GtfsDate::parse("20240520").unwrap()), // Within 7 days of 2024-05-18
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedExpirationDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FEED_EXPIRATION_DATE_7_DAYS
        );
    }

    #[test]
    fn detects_expiration_within_30_days() {
        let _guard =
            crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2024, 5, 18).unwrap()));
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into(), "feed_end_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: Some(GtfsDate::parse("20240610").unwrap()), // Within 30 days of 2024-05-18
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedExpirationDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_FEED_EXPIRATION_DATE_30_DAYS
        );
    }

    #[test]
    fn passes_far_expiration() {
        let _guard =
            crate::set_validation_date(Some(NaiveDate::from_ymd_opt(2024, 5, 18).unwrap()));
        let mut feed = GtfsFeed::default();
        feed.feed_info = Some(CsvTable {
            headers: vec!["feed_publisher_name".into(), "feed_end_date".into()],
            rows: vec![FeedInfo {
                feed_publisher_name: "Test".into(),
                feed_publisher_url: feed.pool.intern("http://example.com"),
                feed_lang: feed.pool.intern("en"),
                feed_start_date: None,
                feed_end_date: Some(GtfsDate::parse("20250101").unwrap()),
                feed_version: None,
                feed_contact_email: None,
                feed_contact_url: None,
                default_lang: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FeedExpirationDateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
