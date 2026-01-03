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

