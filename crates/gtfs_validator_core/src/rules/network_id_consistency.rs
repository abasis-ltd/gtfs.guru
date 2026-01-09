use crate::feed::{NETWORKS_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_ROUTE_NETWORKS_SPECIFIED_IN_MORE_THAN_ONE_FILE: &str =
    "route_networks_specified_in_more_than_one_file";

#[derive(Debug, Default)]
pub struct NetworkIdConsistencyValidator;

impl Validator for NetworkIdConsistencyValidator {
    fn name(&self) -> &'static str {
        "network_id_consistency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let has_network_id_column = feed
            .routes
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("network_id"));
        if !has_network_id_column {
            return;
        }

        if feed.route_networks.is_some() {
            notices.push(route_networks_specified_notice(ROUTE_NETWORKS_FILE));
        }
        if feed.networks.is_some() {
            notices.push(route_networks_specified_notice(NETWORKS_FILE));
        }
    }
}

fn route_networks_specified_notice(file_name_b: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_ROUTE_NETWORKS_SPECIFIED_IN_MORE_THAN_ONE_FILE,
        NoticeSeverity::Error,
        "route networks specified in more than one file",
    );
    notice.insert_context_field("fieldName", "network_id");
    notice.insert_context_field("fileNameA", ROUTES_FILE);
    notice.insert_context_field("fileNameB", file_name_b);
    notice.field_order = vec!["fieldName".into(), "fileNameA".into(), "fileNameB".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn detects_network_id_in_routes_and_route_networks() {
        let feed = GtfsFeed {
            routes: CsvTable {
                headers: vec!["network_id".into()],
                rows: vec![Default::default()],
                ..Default::default()
            },
            route_networks: Some(CsvTable::default()),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        NetworkIdConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_ROUTE_NETWORKS_SPECIFIED_IN_MORE_THAN_ONE_FILE));
    }

    #[test]
    fn detects_network_id_in_routes_and_networks() {
        let feed = GtfsFeed {
            routes: CsvTable {
                headers: vec!["network_id".into()],
                rows: vec![Default::default()],
                ..Default::default()
            },
            networks: Some(CsvTable::default()),
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        NetworkIdConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_ROUTE_NETWORKS_SPECIFIED_IN_MORE_THAN_ONE_FILE));
    }

    #[test]
    fn passes_when_only_in_routes() {
        let feed = GtfsFeed {
            routes: CsvTable {
                headers: vec!["network_id".into()],
                rows: vec![Default::default()],
                ..Default::default()
            },
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        NetworkIdConsistencyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
