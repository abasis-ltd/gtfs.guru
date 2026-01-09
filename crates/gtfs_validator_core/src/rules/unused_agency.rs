use std::collections::HashSet;

use crate::feed::AGENCY_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_AGENCY: &str = "unused_agency";

use crate::validation_context::thorough_mode_enabled;

#[derive(Debug, Default)]
pub struct UnusedAgencyValidator;

impl Validator for UnusedAgencyValidator {
    fn name(&self) -> &'static str {
        "unused_agency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !thorough_mode_enabled() {
            return;
        }
        if feed.agency.rows.len() <= 1 {
            // If there's only one agency, it's considered used by default if routes exist,
            // or it's simply the only agency.
            return;
        }

        let mut used_agency_ids: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        for route in &feed.routes.rows {
            if let Some(agency_id) = route.agency_id.filter(|id| id.0 != 0) {
                used_agency_ids.insert(agency_id);
            } else {
                // If agency_id is omitted in routes, it refers to the only agency (if only one exists)
                // but we are in the multi-agency case here.
            }
        }

        for (index, agency) in feed.agency.rows.iter().enumerate() {
            if let Some(agency_id) = agency.agency_id.filter(|id| id.0 != 0) {
                if !used_agency_ids.contains(&agency_id) {
                    let agency_id_value = feed.pool.resolve(agency_id);
                    let mut notice = ValidationNotice::new(
                        CODE_UNUSED_AGENCY,
                        NoticeSeverity::Warning,
                        "agency is not referenced by any route",
                    );
                    notice.file = Some(AGENCY_FILE.to_string());
                    notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                    notice.insert_context_field("agencyId", agency_id_value.as_str());
                    notice.insert_context_field("agencyName", &agency.agency_name);
                    notice.field_order = vec![
                        "csvRowNumber".into(),
                        "agencyId".into(),
                        "agencyName".into(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Agency, Route};

    #[test]
    fn detects_unused_agency() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".into(),
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![
                Agency {
                    agency_id: Some(feed.pool.intern("A1")),
                    agency_name: "Agency1".into(),
                    ..Default::default()
                },
                Agency {
                    agency_id: Some(feed.pool.intern("A2")),
                    agency_name: "Agency2".into(),
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
        UnusedAgencyValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_UNUSED_AGENCY)
                .count(),
            1
        );
        let notice = notices
            .iter()
            .find(|n| n.code == CODE_UNUSED_AGENCY)
            .unwrap();
        assert_eq!(
            notice.context.get("agencyId").unwrap().as_str().unwrap(),
            "A2"
        );
    }

    #[test]
    fn passes_when_all_agencies_used() {
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
            rows: vec![
                Route {
                    route_id: feed.pool.intern("R1"),
                    agency_id: Some(feed.pool.intern("A1")),
                    ..Default::default()
                },
                Route {
                    route_id: feed.pool.intern("R2"),
                    agency_id: Some(feed.pool.intern("A2")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        UnusedAgencyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_single_agency_feed() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_id".into()],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // Even if no routes reference it, single agency is usually implicitly linked or allowed.
        // The validator logic returns early if rows.len() <= 1.

        let mut notices = NoticeContainer::new();
        UnusedAgencyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
