use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::FareMediaType;

const CODE_DUPLICATE_FARE_MEDIA: &str = "duplicate_fare_media";

#[derive(Debug, Default)]
pub struct DuplicateFareMediaValidator;

impl Validator for DuplicateFareMediaValidator {
    fn name(&self) -> &'static str {
        "duplicate_fare_media"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(fare_media) = &feed.fare_media else {
            return;
        };

        let mut seen: HashMap<MediaKey, (u64, String)> = HashMap::new();
        for (index, media) in fare_media.rows.iter().enumerate() {
            let row_number = fare_media.row_number(index);
            let key = MediaKey::new(media);
            let fare_media_id = media.fare_media_id.trim();
            if let Some((prev_row, prev_id)) = seen.get(&key) {
                let mut notice = ValidationNotice::new(
                    CODE_DUPLICATE_FARE_MEDIA,
                    NoticeSeverity::Warning,
                    "duplicate fare_media_name and fare_media_type",
                );
                notice.insert_context_field("csvRowNumber1", *prev_row);
                notice.insert_context_field("csvRowNumber2", row_number);
                notice.insert_context_field("fareMediaId1", prev_id);
                notice.insert_context_field("fareMediaId2", fare_media_id);
                notice.field_order = vec![
                    "csvRowNumber1".to_string(),
                    "csvRowNumber2".to_string(),
                    "fareMediaId1".to_string(),
                    "fareMediaId2".to_string(),
                ];
                notices.push(notice);
            } else {
                seen.insert(key, (row_number, fare_media_id.to_string()));
            }
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct MediaKey {
    name: String,
    media_type: FareMediaType,
}

impl MediaKey {
    fn new(media: &gtfs_model::FareMedia) -> Self {
        Self {
            name: media
                .fare_media_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string(),
            media_type: media.fare_media_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_model::FareMedia;

    #[test]
    fn detects_duplicate_fare_media() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec![
                "fare_media_id".to_string(),
                "fare_media_name".to_string(),
                "fare_media_type".to_string(),
            ],
            rows: vec![
                FareMedia {
                    fare_media_id: "M1".to_string(),
                    fare_media_name: Some("Pass".to_string()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: "M2".to_string(),
                    fare_media_name: Some("Pass".to_string()),
                    fare_media_type: FareMediaType::NoneType,
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        DuplicateFareMediaValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_DUPLICATE_FARE_MEDIA
        );
    }

    #[test]
    fn passes_different_fare_media() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec![
                "fare_media_id".to_string(),
                "fare_media_name".to_string(),
                "fare_media_type".to_string(),
            ],
            rows: vec![
                FareMedia {
                    fare_media_id: "M1".to_string(),
                    fare_media_name: Some("Pass".to_string()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: "M2".to_string(),
                    fare_media_name: Some("Card".to_string()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: "M3".to_string(),
                    fare_media_name: Some("Pass".to_string()),
                    fare_media_type: FareMediaType::TransitCard,
                },
            ],
            row_numbers: vec![2, 3, 4],
        });

        let mut notices = NoticeContainer::new();
        DuplicateFareMediaValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
