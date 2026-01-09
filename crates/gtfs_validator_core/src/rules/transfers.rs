use crate::feed::TRANSFERS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";

#[derive(Debug, Default)]
pub struct TransfersValidator;

impl Validator for TransfersValidator {
    fn name(&self) -> &'static str {
        "transfers_basic"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(transfers) = &feed.transfers {
            for (index, transfer) in transfers.rows.iter().enumerate() {
                let row_number = transfers.row_number(index);
                if matches!(
                    transfer.transfer_type,
                    Some(gtfs_guru_model::TransferType::MinTime)
                ) && transfer.min_transfer_time.is_none()
                {
                    let mut notice = ValidationNotice::new(
                        CODE_MISSING_REQUIRED_FIELD,
                        NoticeSeverity::Error,
                        "transfer_type=2 requires min_transfer_time",
                    );
                    notice.set_location(TRANSFERS_FILE, "min_transfer_time", row_number);
                    notice.field_order =
                        vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
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

    #[test]
    fn emits_notice_when_min_time_missing() {
        let mut feed = GtfsFeed::default();
        feed.transfers = Some(CsvTable {
            rows: vec![gtfs_guru_model::Transfer {
                from_stop_id: Some(feed.pool.intern("STOP1")),
                to_stop_id: Some(feed.pool.intern("STOP2")),
                transfer_type: Some(gtfs_guru_model::TransferType::MinTime),
                min_transfer_time: None,
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        TransfersValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_MISSING_REQUIRED_FIELD
        );
    }

    #[test]
    fn passes_when_min_time_present() {
        let mut feed = GtfsFeed::default();
        feed.transfers = Some(CsvTable {
            rows: vec![gtfs_guru_model::Transfer {
                from_stop_id: Some(feed.pool.intern("STOP1")),
                to_stop_id: Some(feed.pool.intern("STOP2")),
                transfer_type: Some(gtfs_guru_model::TransferType::MinTime),
                min_transfer_time: Some(120),
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        TransfersValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
