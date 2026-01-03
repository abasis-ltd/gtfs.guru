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
    notice.field_order = vec!["csvRowNumber".to_string(), "fieldName".to_string()];
    notice
}

