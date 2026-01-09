use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::FareMediaType;

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

        let mut seen: HashMap<MediaKey, (u64, gtfs_guru_model::StringId)> = HashMap::new();
        for (index, media) in fare_media.rows.iter().enumerate() {
            let row_number = fare_media.row_number(index);
            let key = MediaKey::new(media);
            let fare_media_id = media.fare_media_id;
            if let Some((prev_row, prev_id)) = seen.get(&key) {
                let prev_id_value = feed.pool.resolve(*prev_id);
                let fare_media_value = feed.pool.resolve(fare_media_id);
                let mut notice = ValidationNotice::new(
                    CODE_DUPLICATE_FARE_MEDIA,
                    NoticeSeverity::Warning,
                    "duplicate fare_media_name and fare_media_type",
                );
                notice.insert_context_field("csvRowNumber1", *prev_row);
                notice.insert_context_field("csvRowNumber2", row_number);
                notice.insert_context_field("fareMediaId1", prev_id_value.as_str());
                notice.insert_context_field("fareMediaId2", fare_media_value.as_str());
                notice.field_order = vec![
                    "csvRowNumber1".into(),
                    "csvRowNumber2".into(),
                    "fareMediaId1".into(),
                    "fareMediaId2".into(),
                ];
                notices.push(notice);
            } else {
                seen.insert(key, (row_number, fare_media_id));
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
    fn new(media: &gtfs_guru_model::FareMedia) -> Self {
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
    use gtfs_guru_model::FareMedia;

    #[test]
    fn detects_duplicate_fare_media() {
        let mut feed = GtfsFeed::default();
        feed.fare_media = Some(CsvTable {
            headers: vec![
                "fare_media_id".into(),
                "fare_media_name".into(),
                "fare_media_type".into(),
            ],
            rows: vec![
                FareMedia {
                    fare_media_id: feed.pool.intern("M1"),
                    fare_media_name: Some("Pass".into()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: feed.pool.intern("M2"),
                    fare_media_name: Some("Pass".into()),
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
                "fare_media_id".into(),
                "fare_media_name".into(),
                "fare_media_type".into(),
            ],
            rows: vec![
                FareMedia {
                    fare_media_id: feed.pool.intern("M1"),
                    fare_media_name: Some("Pass".into()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: feed.pool.intern("M2"),
                    fare_media_name: Some("Card".into()),
                    fare_media_type: FareMediaType::NoneType,
                },
                FareMedia {
                    fare_media_id: feed.pool.intern("M3"),
                    fare_media_name: Some("Pass".into()),
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
