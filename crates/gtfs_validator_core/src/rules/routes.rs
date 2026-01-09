use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_ROUTE_BOTH_NAMES_MISSING: &str = "route_both_short_and_long_name_missing";
const CODE_ROUTE_SHORT_NAME_TOO_LONG: &str = "route_short_name_too_long";
const CODE_ROUTE_LONG_NAME_CONTAINS_SHORT: &str = "route_long_name_contains_short_name";
const CODE_ROUTE_DESC_SAME_AS_NAME: &str = "same_name_and_description_for_route";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Route, RouteType};

    #[test]
    fn detects_both_names_missing() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_type".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                route_short_name: None,
                route_long_name: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RoutesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_ROUTE_BOTH_NAMES_MISSING
        );
    }

    #[test]
    fn detects_short_name_too_long() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "route_short_name".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                route_short_name: Some("VeryLongRouteName".into()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RoutesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_ROUTE_SHORT_NAME_TOO_LONG
        );
    }

    #[test]
    fn detects_long_name_contains_short() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_long_name".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                route_short_name: Some("42".into()),
                route_long_name: Some("42 Downtown Express".into()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RoutesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_ROUTE_LONG_NAME_CONTAINS_SHORT
        );
    }

    #[test]
    fn detects_desc_same_as_short_name() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_desc".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                route_short_name: Some("42".into()),
                route_desc: Some("42".into()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RoutesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_ROUTE_DESC_SAME_AS_NAME
        );
    }

    #[test]
    fn passes_with_valid_route() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_short_name".into(),
                "route_long_name".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_type: RouteType::Bus,
                route_short_name: Some("42".into()),
                route_long_name: Some("Downtown Express".into()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RoutesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}

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
                notice.insert_context_field("routeId", feed.pool.resolve(route.route_id).as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.field_order = vec!["csvRowNumber".into(), "routeId".into()];
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
                    notice.insert_context_field(
                        "routeId",
                        feed.pool.resolve(route.route_id).as_str(),
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("routeShortName", short);
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "routeId".into(),
                        "routeShortName".into(),
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
                        notice.insert_context_field(
                            "routeId",
                            feed.pool.resolve(route.route_id).as_str(),
                        );
                        notice.insert_context_field("csvRowNumber", row_number);
                        notice.insert_context_field("routeShortName", short);
                        notice.insert_context_field("routeLongName", long);
                        notice.field_order = vec![
                            "csvRowNumber".into(),
                            "routeId".into(),
                            "routeLongName".into(),
                            "routeShortName".into(),
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
                        notice.insert_context_field(
                            "routeId",
                            feed.pool.resolve(route.route_id).as_str(),
                        );
                        notice.insert_context_field("routeDesc", route_desc);
                        notice.insert_context_field("specifiedField", "route_short_name");
                        notice.field_order = vec![
                            "csvRowNumber".into(),
                            "routeDesc".into(),
                            "routeId".into(),
                            "specifiedField".into(),
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
                        notice.insert_context_field(
                            "routeId",
                            feed.pool.resolve(route.route_id).as_str(),
                        );
                        notice.insert_context_field("routeDesc", route_desc);
                        notice.insert_context_field("specifiedField", "route_long_name");
                        notice.field_order = vec![
                            "csvRowNumber".into(),
                            "routeDesc".into(),
                            "routeId".into(),
                            "specifiedField".into(),
                        ];
                        notices.push(notice);
                    }
                }
            }
        }
    }
}
