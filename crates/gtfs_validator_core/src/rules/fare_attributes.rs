use crate::feed::FARE_ATTRIBUTES_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_NUMBER_OUT_OF_RANGE: &str = "number_out_of_range";

#[derive(Debug, Default)]
pub struct FareAttributesValidator;

impl Validator for FareAttributesValidator {
    fn name(&self) -> &'static str {
        "fare_attributes_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(fares) = &feed.fare_attributes {
            for (index, fare) in fares.rows.iter().enumerate() {
                let row_number = fares.row_number(index);
                if fare.price < 0.0 {
                    notices.push(number_out_of_range_notice(
                        "price", row_number, "float", fare.price,
                    ));
                }
            }
        }
    }
}

fn number_out_of_range_notice(
    field: &str,
    row_number: u64,
    field_type: &str,
    field_value: f64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_NUMBER_OUT_OF_RANGE,
        NoticeSeverity::Error,
        "value out of range",
    );
    notice.set_location(FARE_ATTRIBUTES_FILE, field, row_number);
    notice.insert_context_field("fieldType", field_type);
    notice.insert_context_field("fieldValue", field_value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldType".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::FareAttribute;

    #[test]
    fn detects_negative_price() {
        let mut feed = GtfsFeed::default();
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".into(), "price".into()],
            rows: vec![FareAttribute {
                fare_id: feed.pool.intern("F1"),
                price: -5.0,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareAttributesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_NUMBER_OUT_OF_RANGE
        );
    }

    #[test]
    fn passes_valid_price() {
        let mut feed = GtfsFeed::default();
        feed.fare_attributes = Some(CsvTable {
            headers: vec!["fare_id".into(), "price".into()],
            rows: vec![FareAttribute {
                fare_id: feed.pool.intern("F1"),
                price: 5.0,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareAttributesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
