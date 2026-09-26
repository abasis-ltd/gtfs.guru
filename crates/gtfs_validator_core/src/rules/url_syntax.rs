//! `u_r_i_syntax_error`, a `--thorough` companion to `invalid_url`.
//!
//! It fires on exactly the values `invalid_url` rejects, and a row with an
//! error is not loaded (the canonical validator builds no entity for it), so
//! the check runs in the row validator, which still sees the row, rather than
//! over the loaded tables.

use crate::{FixSafety, NoticeSeverity, ValidationNotice};
use url::Url;

const CODE_URI_SYNTAX_ERROR: &str = "u_r_i_syntax_error";

/// The URL columns the check covers.
const CHECKED_URL_FIELDS: &[(&str, &str)] = &[
    ("agency.txt", "agency_url"),
    ("agency.txt", "agency_fare_url"),
    ("stops.txt", "stop_url"),
    ("routes.txt", "route_url"),
    ("feed_info.txt", "feed_publisher_url"),
    ("feed_info.txt", "feed_contact_url"),
];

/// Whether `field` of `filename` is checked.
pub(crate) fn checks_field(filename: &str, field: &str) -> bool {
    CHECKED_URL_FIELDS
        .iter()
        .any(|(file, name)| file.eq_ignore_ascii_case(filename) && *name == field)
}

/// The notice for `url_str`, when it does not parse as a URI.
pub(crate) fn uri_syntax_error_notice(
    url_str: &str,
    filename: &str,
    field_name: &str,
    row_number: u64,
) -> Option<ValidationNotice> {
    let trimmed = url_str.trim();
    if trimmed.is_empty() {
        return None;
    }
    let err = Url::parse(trimmed).err()?;
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
        "filename".into(),
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
    ];

    // The location lives in context fields here, so it has to be passed in.
    if let Some(replacement) = crate::fix_suggest::url(trimmed) {
        crate::fix_suggest::attach_fix(
            &mut notice,
            "Add the https:// scheme",
            FixSafety::Safe,
            filename,
            row_number,
            field_name,
            trimmed,
            replacement,
        );
    }
    Some(notice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FixOperation;

    #[test]
    fn detects_invalid_agency_url() {
        let notice = uri_syntax_error_notice("ht tp://invalid", "agency.txt", "agency_url", 2)
            .expect("invalid URI");
        assert_eq!(notice.code, CODE_URI_SYNTAX_ERROR);
        assert_eq!(
            notice.context.get("fieldName").unwrap().as_str().unwrap(),
            "agency_url"
        );
        assert!(
            uri_syntax_error_notice("https://example.com", "agency.txt", "agency_url", 2).is_none()
        );
        assert!(checks_field("agency.txt", "agency_url"));
        assert!(!checks_field("agency.txt", "agency_email"));
    }

    #[test]
    fn suggests_fix_for_url_missing_scheme() {
        let notice = uri_syntax_error_notice("www.example.com", "agency.txt", "agency_url", 2)
            .expect("invalid URI");

        // Check that a fix is suggested
        let fix = notice.fix.as_ref().expect("should suggest a fix");
        assert_eq!(fix.safety, FixSafety::Safe);

        let FixOperation::ReplaceField {
            original,
            replacement,
            ..
        } = &fix.operation
        else {
            panic!("expected field replacement");
        };

        assert_eq!(original, "www.example.com");
        assert_eq!(replacement, "https://www.example.com");
    }
}
