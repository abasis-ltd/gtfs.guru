use crate::feed::ROUTES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";
const CODE_MISSING_RECOMMENDED_FIELD: &str = "missing_recommended_field";

#[derive(Debug, Default)]
pub struct RouteAgencyIdValidator;

impl Validator for RouteAgencyIdValidator {
    fn name(&self) -> &'static str {
        "route_agency_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let total_agencies = feed.agency.rows.len();
        if total_agencies == 0 {
            return;
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            if !has_value(route.agency_id) {
                let (code, severity, message) = if total_agencies > 1 {
                    (
                        CODE_MISSING_REQUIRED_FIELD,
                        NoticeSeverity::Error,
                        "agency_id is required when multiple agencies exist",
                    )
                } else {
                    (
                        CODE_MISSING_RECOMMENDED_FIELD,
                        NoticeSeverity::Warning,
                        "agency_id is recommended when only one agency exists",
                    )
                };
                let mut notice = ValidationNotice::new(code, severity, message);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldName", "agency_id");
                notice.insert_context_field("filename", ROUTES_FILE);
                notice.field_order =
                    vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
                notices.push(notice);
            }
        }
    }
}

fn has_value(value: Option<StringId>) -> bool {
    matches!(value, Some(id) if id.0 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Agency, Route};

    #[test]
    fn detects_missing_required_agency_id() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_id".into()],
            rows: vec![
                Agency {
                    agency_id: Some(feed.pool.intern("A1")),
                    ..Default::default()
                },
                Agency {
                    agency_id: Some(feed.pool.intern("A2")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                agency_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RouteAgencyIdValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_REQUIRED_FIELD));
    }

    #[test]
    fn detects_missing_recommended_agency_id() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_id".into()],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                agency_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RouteAgencyIdValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_MISSING_RECOMMENDED_FIELD));
    }

    #[test]
    fn passes_when_agency_id_present() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_id".into()],
            rows: vec![
                Agency {
                    agency_id: Some(feed.pool.intern("A1")),
                    ..Default::default()
                },
                Agency {
                    agency_id: Some(feed.pool.intern("A2")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "agency_id".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                agency_id: Some(feed.pool.intern("A1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RouteAgencyIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
