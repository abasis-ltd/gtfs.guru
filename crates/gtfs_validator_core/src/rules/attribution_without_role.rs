use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;
use gtfs_guru_model::YesNo;

const CODE_ATTRIBUTION_WITHOUT_ROLE: &str = "attribution_without_role";

#[derive(Debug, Default)]
pub struct AttributionWithoutRoleValidator;

impl Validator for AttributionWithoutRoleValidator {
    fn name(&self) -> &'static str {
        "attribution_without_role"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(attributions) = &feed.attributions else {
            return;
        };
        if !has_role_headers(&attributions.headers) {
            return;
        }

        for (index, attribution) in attributions.rows.iter().enumerate() {
            let row_number = attributions.row_number(index);
            if !has_some_role(attribution) {
                let resolved = feed
                    .pool
                    .resolve(attribution.attribution_id.unwrap_or(StringId(0)));
                let attribution_id = resolved.trim();
                notices.push(attribution_without_role_notice(attribution_id, row_number));
            }
        }
    }
}

fn has_role_headers(headers: &[String]) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case("is_producer"))
        || headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("is_operator"))
        || headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("is_authority"))
}

fn has_some_role(attribution: &gtfs_guru_model::Attribution) -> bool {
    matches!(attribution.is_producer, Some(YesNo::Yes))
        || matches!(attribution.is_operator, Some(YesNo::Yes))
        || matches!(attribution.is_authority, Some(YesNo::Yes))
}

fn attribution_without_role_notice(attribution_id: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_ATTRIBUTION_WITHOUT_ROLE,
        NoticeSeverity::Warning,
        "attribution has no role set",
    );
    notice.insert_context_field("attributionId", attribution_id);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.field_order = vec!["attributionId".into(), "csvRowNumber".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::RouteType;

    #[test]
    fn emits_notice_when_no_roles_set() {
        let mut feed = base_feed();
        feed.attributions = Some(CsvTable {
            headers: vec![
                "is_producer".into(),
                "is_operator".into(),
                "is_authority".into(),
            ],
            rows: vec![gtfs_guru_model::Attribution {
                attribution_id: None,
                agency_id: None,
                route_id: None,
                trip_id: None,
                organization_name: feed.pool.intern("Org"),
                is_producer: Some(YesNo::No),
                is_operator: None,
                is_authority: Some(YesNo::No),
                attribution_url: None,
                attribution_email: None,
                attribution_phone: None,
            }],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        AttributionWithoutRoleValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_ATTRIBUTION_WITHOUT_ROLE);
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_str(notice, "attributionId"), "");
    }

    #[test]
    fn skips_when_role_header_missing() {
        let mut feed = base_feed();
        feed.attributions = Some(CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Attribution {
                attribution_id: None,
                agency_id: None,
                route_id: None,
                trip_id: None,
                organization_name: feed.pool.intern("Org"),
                is_producer: None,
                is_operator: None,
                is_authority: None,
                attribution_url: None,
                attribution_email: None,
                attribution_phone: None,
            }],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        AttributionWithoutRoleValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_when_role_set() {
        let mut feed = base_feed();
        feed.attributions = Some(CsvTable {
            headers: vec!["is_producer".into()],
            rows: vec![gtfs_guru_model::Attribution {
                attribution_id: None,
                agency_id: None,
                route_id: None,
                trip_id: None,
                organization_name: feed.pool.intern("Org"),
                is_producer: Some(YesNo::Yes),
                is_operator: None,
                is_authority: None,
                attribution_url: None,
                attribution_email: None,
                attribution_phone: None,
            }],
            row_numbers: Vec::new(),
        });

        let mut notices = NoticeContainer::new();
        AttributionWithoutRoleValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn base_feed() -> GtfsFeed {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Agency {
                agency_id: None,
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
        feed.stops = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Stop {
                stop_id: feed.pool.intern("STOP1"),
                stop_name: Some("Stop".into()),
                stop_lat: Some(10.0),
                stop_lon: Some(20.0),
                ..Default::default()
            }],
            row_numbers: Vec::new(),
        };
        feed.routes = CsvTable {
            headers: Vec::new(),
            rows: vec![gtfs_guru_model::Route {
                route_id: feed.pool.intern("R1"),
                route_short_name: Some("R1".into()),
                route_type: RouteType::Bus,
                ..Default::default()
            }],
            row_numbers: Vec::new(),
        };
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
