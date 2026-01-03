use crate::feed::FARE_MEDIA_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::FareMediaType;

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

fn should_have_name(media_type: FareMediaType) -> bool {
    matches!(
        media_type,
        FareMediaType::TransitCard | FareMediaType::MobileApp
    )
}

