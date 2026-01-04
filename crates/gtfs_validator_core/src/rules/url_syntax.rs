use crate::{
    Fix, FixOperation, FixSafety, GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice,
    Validator,
};
use url::Url;

const CODE_URI_SYNTAX_ERROR: &str = "u_r_i_syntax_error";

#[derive(Debug, Default)]
pub struct UrlSyntaxValidator;

impl Validator for UrlSyntaxValidator {
    fn name(&self) -> &'static str {
        "url_syntax"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, agency) in feed.agency.rows.iter().enumerate() {
            validate_url(
                &agency.agency_url,
                "agency.txt",
                "agency_url",
                feed.agency.row_number(index),
                notices,
            );
            if let Some(url) = agency.agency_fare_url.as_deref() {
                validate_url(
                    url,
                    "agency.txt",
                    "agency_fare_url",
                    feed.agency.row_number(index),
                    notices,
                );
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            if let Some(url) = stop.stop_url.as_deref() {
                validate_url(
                    url,
                    "stops.txt",
                    "stop_url",
                    feed.stops.row_number(index),
                    notices,
                );
            }
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            if let Some(url) = route.route_url.as_deref() {
                validate_url(
                    url,
                    "routes.txt",
                    "route_url",
                    feed.routes.row_number(index),
                    notices,
                );
            }
        }

        if let Some(feed_info) = &feed.feed_info {
            for (index, info) in feed_info.rows.iter().enumerate() {
                validate_url(
                    &info.feed_publisher_url,
                    "feed_info.txt",
                    "feed_publisher_url",
                    feed_info.row_number(index),
                    notices,
                );
                if let Some(url) = info.feed_contact_url.as_deref() {
                    validate_url(
                        url,
                        "feed_info.txt",
                        "feed_contact_url",
                        feed_info.row_number(index),
                        notices,
                    );
                }
            }
        }
    }
}

fn validate_url(
    url_str: &str,
    filename: &str,
    field_name: &str,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let trimmed = url_str.trim();
    if trimmed.is_empty() {
        return;
    }

    if let Err(err) = Url::parse(trimmed) {
        let mut notice = ValidationNotice::new(
            CODE_URI_SYNTAX_ERROR,
            NoticeSeverity::Error,
            format!("invalid URI: {}", err),
        );
        notice.insert_context_field("filename", filename);
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("fieldName", field_name);
        notice.insert_context_field("fieldValue", trimmed);
        notice.field_order = vec![
            "filename".to_string(),
            "csvRowNumber".to_string(),
            "fieldName".to_string(),
            "fieldValue".to_string(),
        ];

        // Try to suggest a fix if the URL is just missing a scheme
        if !trimmed.contains("://") && (trimmed.contains('.') || trimmed.starts_with("www.")) {
            let suggested = format!("https://{}", trimmed);
            if Url::parse(&suggested).is_ok() {
                notice.fix = Some(Fix {
                    description: "Add https:// scheme".to_string(),
                    safety: FixSafety::Safe,
                    operation: FixOperation::ReplaceField {
                        file: filename.to_string(),
                        row: row_number,
                        field: field_name.to_string(),
                        original: trimmed.to_string(),
                        replacement: suggested,
                    },
                });
            }
        }

        notices.push(notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::Agency;

    #[test]
    fn detects_invalid_agency_url() {
        let feed = GtfsFeed {
            agency: CsvTable {
                headers: vec![
                    "agency_name".to_string(),
                    "agency_url".to_string(),
                    "agency_timezone".to_string(),
                ],
                rows: vec![Agency {
                    agency_id: None,
                    agency_name: "Test".to_string(),
                    agency_url: "ht tp://invalid".to_string(),
                    agency_timezone: "UTC".to_string(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: vec![2],
            },
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        UrlSyntaxValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_URI_SYNTAX_ERROR);
        assert_eq!(
            notice.context.get("fieldName").unwrap().as_str().unwrap(),
            "agency_url"
        );
    }

    #[test]
    fn suggests_fix_for_url_missing_scheme() {
        let feed = GtfsFeed {
            agency: CsvTable {
                headers: vec![
                    "agency_name".to_string(),
                    "agency_url".to_string(),
                    "agency_timezone".to_string(),
                ],
                rows: vec![Agency {
                    agency_id: None,
                    agency_name: "Test".to_string(),
                    agency_url: "www.example.com".to_string(),
                    agency_timezone: "UTC".to_string(),
                    agency_lang: None,
                    agency_phone: None,
                    agency_fare_url: None,
                    agency_email: None,
                }],
                row_numbers: vec![2],
            },
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        UrlSyntaxValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_URI_SYNTAX_ERROR);

        // Check that a fix is suggested
        let fix = notice.fix.as_ref().expect("should suggest a fix");
        assert_eq!(fix.safety, FixSafety::Safe);

        let FixOperation::ReplaceField {
            original,
            replacement,
            ..
        } = &fix.operation;

        assert_eq!(original, "www.example.com");
        assert_eq!(replacement, "https://www.example.com");
    }
}
