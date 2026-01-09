use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{Bidirectional, PathwayMode};

const CODE_BIDIRECTIONAL_EXIT_GATE: &str = "bidirectional_exit_gate";

#[derive(Debug, Default)]
pub struct BidirectionalExitGateValidator;

impl Validator for BidirectionalExitGateValidator {
    fn name(&self) -> &'static str {
        "bidirectional_exit_gate"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        for (index, pathway) in pathways.rows.iter().enumerate() {
            let row_number = pathways.row_number(index);
            if pathway.pathway_mode == PathwayMode::ExitGate
                && pathway.is_bidirectional == Bidirectional::Bidirectional
            {
                let mut notice = ValidationNotice::new(
                    CODE_BIDIRECTIONAL_EXIT_GATE,
                    NoticeSeverity::Error,
                    "exit gate pathways must not be bidirectional",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field(
                    "isBidirectional",
                    bidirectional_value(pathway.is_bidirectional),
                );
                notice
                    .insert_context_field("pathwayMode", pathway_mode_value(pathway.pathway_mode));
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "isBidirectional".into(),
                    "pathwayMode".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

fn bidirectional_value(value: Bidirectional) -> i32 {
    match value {
        Bidirectional::Unidirectional => 0,
        Bidirectional::Bidirectional => 1,
        Bidirectional::Other => -1,
    }
}

fn pathway_mode_value(value: PathwayMode) -> i32 {
    match value {
        PathwayMode::Walkway => 1,
        PathwayMode::Stairs => 2,
        PathwayMode::MovingSidewalk => 3,
        PathwayMode::Escalator => 4,
        PathwayMode::Elevator => 5,
        PathwayMode::FareGate => 6,
        PathwayMode::ExitGate => 7,
        PathwayMode::Other => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn emits_notice_for_bidirectional_exit_gate() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            rows: vec![gtfs_guru_model::Pathway {
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("STOP1"),
                to_stop_id: feed.pool.intern("STOP2"),
                pathway_mode: PathwayMode::ExitGate,
                is_bidirectional: Bidirectional::Bidirectional,
                length: None,
                traversal_time: None,
                stair_count: None,
                max_slope: None,
                min_width: None,
                signposted_as: None,
                reversed_signposted_as: None,
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        BidirectionalExitGateValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_BIDIRECTIONAL_EXIT_GATE);
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
        assert_eq!(context_i64(notice, "isBidirectional"), 1);
        assert_eq!(context_i64(notice, "pathwayMode"), 7);
    }

    #[test]
    fn passes_for_non_exit_gate() {
        let mut feed = GtfsFeed::default();
        feed.pathways = Some(CsvTable {
            rows: vec![gtfs_guru_model::Pathway {
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("STOP1"),
                to_stop_id: feed.pool.intern("STOP2"),
                pathway_mode: PathwayMode::Walkway,
                is_bidirectional: Bidirectional::Bidirectional,
                length: None,
                traversal_time: None,
                stair_count: None,
                max_slope: None,
                min_width: None,
                signposted_as: None,
                reversed_signposted_as: None,
            }],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        BidirectionalExitGateValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    }

    fn context_i64(notice: &ValidationNotice, key: &str) -> i64 {
        notice
            .context
            .get(key)
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }
}
