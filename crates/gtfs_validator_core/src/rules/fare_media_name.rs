use crate::feed::FARE_MEDIA_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::FareMediaType;

const CODE_MISSING_RECOMMENDED_FIELD: &str = "missing_recommended_field";

#[derive(Debug, Default)]
pub struct FareMediaNameValidator;

impl Validator for FareMediaNameValidator {
    fn name(&self) -> &'static str {
        "fare_media_name"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_media) = &feed.fare_media else {
            return;
        };

        for (index, media) in fare_media.rows.iter().enumerate() {
            let row_number = fare_media.row_number(index);
            if should_have_name(media.fare_media_type)
                && media
                    .fare_media_name
                    .as_deref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                let mut notice = ValidationNotice::new(
                    CODE_MISSING_RECOMMENDED_FIELD,
                    NoticeSeverity::Warning,
                    "fare_media_name is recommended for fare_media_type",
                );
                notice.set_location(FARE_MEDIA_FILE, "fare_media_name", row_number);
                notice.field_order =
                    vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
                notices.push(notice);
            }
        }
    }
}

fn should_have_name(media_type: FareMediaType) -> bool {
    matches!(
        media_type,
        FareMediaType::TransitCard | FareMediaType::MobileApp
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::FareMedia;

    #[test]
    fn emits_warning_when_name_missing_for_transit_card() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec!["fare_media_id".into(), "fare_media_type".into()],
            rows: vec![FareMedia {
                fare_media_id: feed.pool.intern("M1"),
                fare_media_type: FareMediaType::TransitCard,
                fare_media_name: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareMediaNameValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_RECOMMENDED_FIELD
        );
    }

    #[test]
    fn passes_when_name_present_for_transit_card() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec![
                "fare_media_id".into(),
                "fare_media_type".into(),
                "fare_media_name".into(),
            ],
            rows: vec![FareMedia {
                fare_media_id: feed.pool.intern("M1"),
                fare_media_type: FareMediaType::TransitCard,
                fare_media_name: Some("Pass".into()),
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareMediaNameValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }

    #[test]
    fn passes_when_name_missing_for_other_type() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec!["fare_media_id".into(), "fare_media_type".into()],
            rows: vec![FareMedia {
                fare_media_id: feed.pool.intern("M1"),
                fare_media_type: FareMediaType::NoneType,
                fare_media_name: None,
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareMediaNameValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
