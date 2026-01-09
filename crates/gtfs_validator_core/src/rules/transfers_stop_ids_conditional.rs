use crate::feed::TRANSFERS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::TransferType;

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";

#[derive(Debug, Default)]
pub struct TransferStopIdsConditionalValidator;

impl Validator for TransferStopIdsConditionalValidator {
    fn name(&self) -> &'static str {
        "transfer_stop_ids_conditional"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(transfers) = &feed.transfers else {
            return;
        };

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            if transfer.transfer_type.is_none() {
                continue;
            }
            if is_in_seat_transfer(transfer.transfer_type) {
                continue;
            }
            if is_blank(
                transfer
                    .from_stop_id
                    .map(|id| feed.pool.resolve(id))
                    .as_deref(),
            ) {
                notices.push(missing_required_field_notice("from_stop_id", row_number));
            }
            if is_blank(
                transfer
                    .to_stop_id
                    .map(|id| feed.pool.resolve(id))
                    .as_deref(),
            ) {
                notices.push(missing_required_field_notice("to_stop_id", row_number));
            }
        }
    }
}

fn is_in_seat_transfer(transfer_type: Option<TransferType>) -> bool {
    matches!(
        transfer_type,
        Some(TransferType::InSeat) | Some(TransferType::InSeatNotAllowed)
    )
}

fn missing_required_field_notice(field: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_REQUIRED_FIELD,
        NoticeSeverity::Error,
        "required field is missing",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("filename", TRANSFERS_FILE);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

fn is_blank(value: Option<&str>) -> bool {
    value.map(|val| val.trim().is_empty()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Transfer, TransferType};

    #[test]
    fn detects_missing_stop_ids_for_regular_transfer() {
        let feed = GtfsFeed {
            transfers: Some(CsvTable {
                headers: vec![
                    "from_stop_id".into(),
                    "to_stop_id".into(),
                    "transfer_type".into(),
                ],
                rows: vec![Transfer {
                    from_stop_id: None, // Missing
                    to_stop_id: None,   // Missing
                    transfer_type: Some(TransferType::Recommended),
                    ..Default::default()
                }],
                row_numbers: vec![2],
            }),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        TransferStopIdsConditionalValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_MISSING_REQUIRED_FIELD)
                .count(),
            2
        );
    }

    #[test]
    fn skips_check_for_in_seat_transfer() {
        let feed = GtfsFeed {
            transfers: Some(CsvTable {
                headers: vec![
                    "from_stop_id".into(),
                    "to_stop_id".into(),
                    "transfer_type".into(),
                ],
                rows: vec![Transfer {
                    from_stop_id: None,
                    to_stop_id: None,
                    transfer_type: Some(TransferType::InSeat),
                    ..Default::default()
                }],
                row_numbers: vec![2],
            }),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        TransferStopIdsConditionalValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn passes_valid_transfer() {
        let mut feed = GtfsFeed::default();
        let s1 = feed.pool.intern("S1");
        let s2 = feed.pool.intern("S2");

        feed.transfers = Some(CsvTable {
            headers: vec![
                "from_stop_id".into(),
                "to_stop_id".into(),
                "transfer_type".into(),
            ],
            rows: vec![Transfer {
                from_stop_id: Some(s1),
                to_stop_id: Some(s2),
                transfer_type: Some(TransferType::Recommended),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransferStopIdsConditionalValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
