use crate::feed::FARE_ATTRIBUTES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;

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
            if !has_value(fare.agency_id) {
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
    use gtfs_guru_model::{Agency, FareAttribute};

    #[test]
    fn emits_warning_when_single_agency_missing_id() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_name".into()],
            rows: vec![Agency {
                agency_name: "Agency".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".into()],
            rows: vec![FareAttribute {
                fare_id: feed.pool.intern("F1"),
                agency_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareAttributeAgencyIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_RECOMMENDED_FIELD
        );
    }

    #[test]
    fn emits_error_when_multiple_agencies_missing_id() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_name".into()],
            rows: vec![
                Agency {
                    agency_name: "Agency1".into(),
                    ..Default::default()
                },
                Agency {
                    agency_name: "Agency2".into(),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".into()],
            rows: vec![FareAttribute {
                fare_id: feed.pool.intern("F1"),
                agency_id: None,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareAttributeAgencyIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_REQUIRED_FIELD
        );
    }

    #[test]
    fn passes_when_agency_id_present() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec!["agency_id".into(), "agency_name".into()],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency".into(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".into(), "agency_id".into()],
            rows: vec![FareAttribute {
                fare_id: feed.pool.intern("F1"),
                agency_id: Some(feed.pool.intern("A1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareAttributeAgencyIdValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
