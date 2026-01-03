use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_ROUTE_BOTH_NAMES_MISSING: &str = "route_both_short_and_long_name_missing";
const CODE_ROUTE_SHORT_NAME_TOO_LONG: &str = "route_short_name_too_long";
const CODE_ROUTE_LONG_NAME_CONTAINS_SHORT: &str = "route_long_name_contains_short_name";
const CODE_ROUTE_DESC_SAME_AS_NAME: &str = "same_name_and_description_for_route";

#[derive(Debug, Default)]
pub struct RoutesValidator;

impl Validator for RoutesValidator {
    fn name(&self) -> &'static str {
        "routes_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            let short_name = route
                .route_short_name
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let long_name = route
                .route_long_name
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            if short_name.is_none() && long_name.is_none() {
                let mut notice = ValidationNotice::new(
                    CODE_ROUTE_BOTH_NAMES_MISSING,
                    NoticeSeverity::Error,
                    "route_short_name and route_long_name are both missing",
                );
                notice.insert_context_field("routeId", route.route_id.as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".to_string(), "routeId".to_string()];
                notices.push(notice);
                continue;
            }

            if let Some(short) = short_name {
                if short.chars().count() > 12 {
                    let mut notice = ValidationNotice::new(
                        CODE_ROUTE_SHORT_NAME_TOO_LONG,
                        NoticeSeverity::Warning,
                        "route_short_name is too long",
                    );
                    notice.insert_context_field("routeId", route.route_id.as_str());
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("routeShortName", short);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "routeId".to_string(),
                        "routeShortName".to_string(),
                    ];
                    notices.push(notice);
                }
            }

            if let (Some(short), Some(long)) = (short_name, long_name) {
                if long
                    .to_ascii_lowercase()
                    .starts_with(&short.to_ascii_lowercase())
                {
                    let remainder = &long[short.len()..];
                    let remainder_starts_with = remainder.chars().next();
                    if remainder.is_empty()
                        || remainder_starts_with
                            .map(|ch| ch.is_whitespace() || ch == '-' || ch == '(')
                            .unwrap_or(false)
                    {
                        let mut notice = ValidationNotice::new(
                            CODE_ROUTE_LONG_NAME_CONTAINS_SHORT,
                            NoticeSeverity::Warning,
                            "route_long_name contains route_short_name",
                        );
                        notice.insert_context_field("routeId", route.route_id.as_str());
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("routeShortName", short);
                        notice.insert_context_field("routeLongName", long);
                        notice.field_order = vec![
                            "csvRowNumber".to_string(),
                            "routeId".to_string(),
                            "routeLongName".to_string(),
                            "routeShortName".to_string(),
                        ];
                        notices.push(notice);
                    }
                }
            }

            if let Some(route_desc) = route.route_desc.as_ref().map(|s| s.trim()) {
                if let Some(short) = short_name {
                    if route_desc.eq_ignore_ascii_case(short) {
                        let mut notice = ValidationNotice::new(
                            CODE_ROUTE_DESC_SAME_AS_NAME,
                            NoticeSeverity::Warning,
                            "route_desc matches route_short_name",
                        );
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("routeId", route.route_id.as_str());
                        notice.insert_context_field("routeDesc", route_desc);
                        notice.insert_context_field("specifiedField", "route_short_name");
                        notice.field_order = vec![
                            "csvRowNumber".to_string(),
                            "routeDesc".to_string(),
                            "routeId".to_string(),
                            "specifiedField".to_string(),
                        ];
                        notices.push(notice);
                        continue;
                    }
                }
                if let Some(long) = long_name {
                    if route_desc.eq_ignore_ascii_case(long) {
                        let mut notice = ValidationNotice::new(
                            CODE_ROUTE_DESC_SAME_AS_NAME,
                            NoticeSeverity::Warning,
                            "route_desc matches route_long_name",
                        );
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("routeId", route.route_id.as_str());
                        notice.insert_context_field("routeDesc", route_desc);
                        notice.insert_context_field("specifiedField", "route_long_name");
                        notice.field_order = vec![
                            "csvRowNumber".to_string(),
                            "routeDesc".to_string(),
                            "routeId".to_string(),
                            "specifiedField".to_string(),
                        ];
                        notices.push(notice);
                    }
                }
            }
        }
    }
}

