use crate::feed::FARE_ATTRIBUTES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";
const CODE_MISSING_RECOMMENDED_FIELD: &str = "missing_recommended_field";

#[derive(Debug, Default)]
pub struct FareAttributeAgencyIdValidator;

impl Validator for FareAttributeAgencyIdValidator {
    fn name(&self) -> &'static str {
        "fare_attribute_agency_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_attributes) = &feed.fare_attributes else {
            return;
        };

        let total_agencies = feed.agency.rows.len();
        if total_agencies == 0 {
            return;
        }

        for (index, fare) in fare_attributes.rows.iter().enumerate() {
            let row_number = fare_attributes.row_number(index);
            if !has_value(fare.agency_id.as_deref()) {
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
                notice.insert_context_field("filename", FARE_ATTRIBUTES_FILE);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "fieldName".to_string(),
                    "filename".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn has_value(value: Option<&str>) -> bool {
    value.map(|val| !val.trim().is_empty()).unwrap_or(false)
}

