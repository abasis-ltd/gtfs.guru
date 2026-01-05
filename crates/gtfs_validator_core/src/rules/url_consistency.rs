use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_SAME_ROUTE_AND_AGENCY_URL: &str = "same_route_and_agency_url";
const CODE_SAME_STOP_AND_AGENCY_URL: &str = "same_stop_and_agency_url";
const CODE_SAME_STOP_AND_ROUTE_URL: &str = "same_stop_and_route_url";

#[derive(Debug, Default)]
pub struct UrlConsistencyValidator;

impl Validator for UrlConsistencyValidator {
    fn name(&self) -> &'static str {
        "url_consistency"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let agency_by_url = agencies_by_url(&feed.agency);
        let route_by_url = routes_by_url(&feed.routes);

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            let Some(route_url) = route.route_url.as_deref() else {
                continue;
            };
            let route_key = normalize_url(route_url);
            if route_key.is_empty() {
                continue;
            }
            if let Some(agencies) = agency_by_url.get(&route_key) {
                for agency in agencies {
                    let mut notice = ValidationNotice::new(
                        CODE_SAME_ROUTE_AND_AGENCY_URL,
                        NoticeSeverity::Warning,
                        "route_url matches agency_url",
                    );
                    notice.insert_context_field("routeCsvRowNumber", row_number);
                    notice.insert_context_field("routeId", route.route_id.as_str());
                    notice.insert_context_field("agencyName", agency.name.as_str());
                    notice.insert_context_field("routeUrl", route_url);
                    notice.insert_context_field("agencyCsvRowNumber", agency.row_number);
                    notice.field_order = vec![
                        "agencyCsvRowNumber".to_string(),
                        "agencyName".to_string(),
                        "routeCsvRowNumber".to_string(),
                        "routeId".to_string(),
                        "routeUrl".to_string(),
                    ];
                    notices.push(notice);
                }
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let Some(stop_url) = stop.stop_url.as_deref() else {
                continue;
            };
            let stop_key = normalize_url(stop_url);
            if stop_key.is_empty() {
                continue;
            }
            if let Some(agencies) = agency_by_url.get(&stop_key) {
                for agency in agencies {
                    let mut notice = ValidationNotice::new(
                        CODE_SAME_STOP_AND_AGENCY_URL,
                        NoticeSeverity::Warning,
                        "stop_url matches agency_url",
                    );
                    notice.insert_context_field("stopCsvRowNumber", row_number);
                    notice.insert_context_field("stopId", stop.stop_id.as_str());
                    notice.insert_context_field("agencyName", agency.name.as_str());
                    notice.insert_context_field("stopUrl", stop_url);
                    notice.insert_context_field("agencyCsvRowNumber", agency.row_number);
                    notice.field_order = vec![
                        "agencyCsvRowNumber".to_string(),
                        "agencyName".to_string(),
                        "stopCsvRowNumber".to_string(),
                        "stopId".to_string(),
                        "stopUrl".to_string(),
                    ];
                    notices.push(notice);
                }
            }
            if let Some(routes) = route_by_url.get(&stop_key) {
                for route_entry in routes {
                    let mut notice = ValidationNotice::new(
                        CODE_SAME_STOP_AND_ROUTE_URL,
                        NoticeSeverity::Warning,
                        "stop_url matches route_url",
                    );
                    notice.insert_context_field("stopCsvRowNumber", row_number);
                    notice.insert_context_field("stopId", stop.stop_id.as_str());
                    notice.insert_context_field("stopUrl", stop_url);
                    notice.insert_context_field("routeId", route_entry.route_id.as_str());
                    notice.insert_context_field("routeCsvRowNumber", route_entry.row_number);
                    notice.field_order = vec![
                        "routeCsvRowNumber".to_string(),
                        "routeId".to_string(),
                        "stopCsvRowNumber".to_string(),
                        "stopId".to_string(),
                        "stopUrl".to_string(),
                    ];
                    notices.push(notice);
                }
            }
        }
    }
}

fn normalize_url(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn agencies_by_url(
    agencies: &crate::CsvTable<gtfs_guru_model::Agency>,
) -> HashMap<String, Vec<AgencyEntry>> {
    let mut map = HashMap::new();
    for (index, agency) in agencies.rows.iter().enumerate() {
        let key = normalize_url(&agency.agency_url);
        if key.is_empty() {
            continue;
        }
        map.entry(key).or_insert_with(Vec::new).push(AgencyEntry {
            row_number: agencies.row_number(index),
            name: agency.agency_name.clone(),
        });
    }
    map
}

fn routes_by_url(
    routes: &crate::CsvTable<gtfs_guru_model::Route>,
) -> HashMap<String, Vec<RouteEntry>> {
    let mut map = HashMap::new();
    for (index, route) in routes.rows.iter().enumerate() {
        let Some(route_url) = route.route_url.as_deref() else {
            continue;
        };
        let key = normalize_url(route_url);
        if key.is_empty() {
            continue;
        }
        map.entry(key).or_insert_with(Vec::new).push(RouteEntry {
            row_number: routes.row_number(index),
            route_id: route.route_id.clone(),
        });
    }
    map
}

#[derive(Debug, Clone)]
struct AgencyEntry {
    row_number: u64,
    name: String,
}

#[derive(Debug, Clone)]
struct RouteEntry {
    row_number: u64,
    route_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Agency, Route, Stop};

    #[test]
    fn detects_identical_route_and_agency_url() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".to_string(),
                "agency_name".to_string(),
                "agency_url".to_string(),
                "agency_timezone".to_string(),
            ],
            rows: vec![Agency {
                agency_id: Some("A1".to_string()),
                agency_name: "Agency A".to_string(),
                agency_url: "http://example.com/agency".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.routes = CsvTable {
            headers: vec![
                "route_id".to_string(),
                "agency_id".to_string(),
                "route_url".to_string(),
            ],
            rows: vec![Route {
                route_id: "R1".to_string(),
                agency_id: Some("A1".to_string()),
                route_url: Some("http://example.com/agency".to_string()), // Same as agency
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UrlConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_SAME_ROUTE_AND_AGENCY_URL));
    }

    #[test]
    fn detects_identical_stop_and_agency_url() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".to_string(),
                "agency_name".to_string(),
                "agency_url".to_string(),
                "agency_timezone".to_string(),
            ],
            rows: vec![Agency {
                agency_id: Some("A1".to_string()),
                agency_name: "Agency A".to_string(),
                agency_url: "http://example.com/agency".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "stop_url".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_url: Some("http://example.com/agency".to_string()), // Same as agency
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UrlConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_SAME_STOP_AND_AGENCY_URL));
    }

    #[test]
    fn detects_identical_stop_and_route_url() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec!["route_id".to_string(), "route_url".to_string()],
            rows: vec![Route {
                route_id: "R1".to_string(),
                route_url: Some("http://example.com/route".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "stop_url".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_url: Some("http://example.com/route".to_string()), // Same as route
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UrlConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_SAME_STOP_AND_ROUTE_URL));
    }

    #[test]
    fn passes_distinct_urls() {
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_id".to_string(),
                "agency_name".to_string(),
                "agency_url".to_string(),
                "agency_timezone".to_string(),
            ],
            rows: vec![Agency {
                agency_id: Some("A1".to_string()),
                agency_name: "Agency A".to_string(),
                agency_url: "http://example.com/agency".to_string(),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.routes = CsvTable {
            headers: vec![
                "route_id".to_string(),
                "agency_id".to_string(),
                "route_url".to_string(),
            ],
            rows: vec![Route {
                route_id: "R1".to_string(),
                agency_id: Some("A1".to_string()),
                route_url: Some("http://example.com/route".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".to_string(), "stop_url".to_string()],
            rows: vec![Stop {
                stop_id: "S1".to_string(),
                stop_url: Some("http://example.com/stop".to_string()),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UrlConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
