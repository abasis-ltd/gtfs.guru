use std::collections::HashSet;

use crate::feed::AGENCY_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_UNUSED_AGENCY: &str = "unused_agency";

#[derive(Debug, Default)]
pub struct UnusedAgencyValidator;

impl Validator for UnusedAgencyValidator {
    fn name(&self) -> &'static str {
        "unused_agency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.agency.rows.len() <= 1 {
            // If there's only one agency, it's considered used by default if routes exist,
            // or it's simply the only agency.
            return;
        }

        let mut used_agency_ids: HashSet<&str> = HashSet::new();
        for route in &feed.routes.rows {
            if let Some(agency_id) = route.agency_id.as_deref() {
                let agency_id = agency_id.trim();
                if !agency_id.is_empty() {
                    used_agency_ids.insert(agency_id);
                }
            } else {
                // If agency_id is omitted in routes, it refers to the only agency (if only one exists)
                // but we are in the multi-agency case here.
            }
        }

        for (index, agency) in feed.agency.rows.iter().enumerate() {
            if let Some(agency_id) = agency.agency_id.as_deref() {
                let agency_id = agency_id.trim();
                if agency_id.is_empty() {
                    continue;
                }

                if !used_agency_ids.contains(agency_id) {
                    let mut notice = ValidationNotice::new(
                        CODE_UNUSED_AGENCY,
                        NoticeSeverity::Warning,
                        "agency is not referenced by any route",
                    );
                    notice.file = Some(AGENCY_FILE.to_string());
                    notice.insert_context_field("csvRowNumber", feed.agency.row_number(index));
                    notice.insert_context_field("agencyId", agency_id);
                    notice.insert_context_field("agencyName", &agency.agency_name);
                    notice.field_order = vec![
                        "csvRowNumber".to_string(),
                        "agencyId".to_string(),
                        "agencyName".to_string(),
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
    use gtfs_model::{Agency, Route};

    #[test]
    fn detects_unused_agency() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".to_string(),
                "agency_name".to_string(),
                "agency_url".to_string(),
                "agency_timezone".to_string(),
            ],
            rows: vec![
                Agency {
                    agency_id: Some("A1".to_string()),
                    agency_name: "Agency1".to_string(),
                    ..Default::default()
                },
                Agency {
                    agency_id: Some("A2".to_string()),
                    agency_name: "Agency2".to_string(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string(), "agency_id".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                agency_id: Some("A1".to_string()),
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
            headers: vec!["agency_id".to_string()],
            rows: vec![
                Agency {
                    agency_id: Some("A1".to_string()),
                    ..Default::default()
                },
                Agency {
                    agency_id: Some("A2".to_string()),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string(), "agency_id".to_string()],
            rows: vec![
                Route {
                    route_id: "R1".to_string(),
                    agency_id: Some("A1".to_string()),
                    ..Default::default()
                },
                Route {
                    route_id: "R2".to_string(),
                    agency_id: Some("A2".to_string()),
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
            headers: vec!["agency_id".to_string()],
            rows: vec![Agency {
                agency_id: Some("A1".to_string()),
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
