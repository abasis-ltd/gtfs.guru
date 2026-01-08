use compact_str::CompactString;
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
        let agency_by_url = agencies_by_url(&feed.agency, feed);
        let route_by_url = routes_by_url(&feed.routes, feed);

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            let Some(route_url) = route.route_url.map(|id| feed.pool.resolve(id)) else {
                continue;
            };
            let route_key = normalize_url(route_url.as_str());
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
                    notice.insert_context_field(
                        "routeId",
                        feed.pool.resolve(route.route_id).as_str(),
                    );
                    notice.insert_context_field("agencyName", agency.name.as_str());
                    notice.insert_context_field("routeUrl", route_url.as_str());
                    notice.insert_context_field("agencyCsvRowNumber", agency.row_number);
                    notice.field_order = vec![
                        "agencyCsvRowNumber".into(),
                        "agencyName".into(),
                        "routeCsvRowNumber".into(),
                        "routeId".into(),
                        "routeUrl".into(),
                    ];
                    notices.push(notice);
                }
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let Some(stop_url) = stop.stop_url.map(|id| feed.pool.resolve(id)) else {
                continue;
            };
            let stop_key = normalize_url(stop_url.as_str());
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
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice.insert_context_field("agencyName", agency.name.as_str());
                    notice.insert_context_field("stopUrl", stop_url.as_str());
                    notice.insert_context_field("agencyCsvRowNumber", agency.row_number);
                    notice.field_order = vec![
                        "agencyCsvRowNumber".into(),
                        "agencyName".into(),
                        "stopCsvRowNumber".into(),
                        "stopId".into(),
                        "stopUrl".into(),
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
                    notice.insert_context_field("stopId", feed.pool.resolve(stop.stop_id).as_str());
                    notice.insert_context_field("stopUrl", stop_url.as_str());
                    notice.insert_context_field(
                        "routeId",
                        feed.pool.resolve(route_entry.route_id).as_str(),
                    );
                    notice.insert_context_field("routeCsvRowNumber", route_entry.row_number);
                    notice.field_order = vec![
                        "routeCsvRowNumber".into(),
                        "routeId".into(),
                        "stopCsvRowNumber".into(),
                        "stopId".into(),
                        "stopUrl".into(),
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
    feed: &GtfsFeed,
) -> HashMap<String, Vec<AgencyEntry>> {
    let mut map = HashMap::new();
    for (index, agency) in agencies.rows.iter().enumerate() {
        let key = normalize_url(feed.pool.resolve(agency.agency_url).as_str());
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
    feed: &GtfsFeed,
) -> HashMap<String, Vec<RouteEntry>> {
    let mut map = HashMap::new();
    for (index, route) in routes.rows.iter().enumerate() {
        let Some(route_url) = route.route_url.map(|id| feed.pool.resolve(id)) else {
            continue;
        };
        let key = normalize_url(route_url.as_str());
        if key.is_empty() {
            continue;
        }
        map.entry(key).or_insert_with(Vec::new).push(RouteEntry {
            row_number: routes.row_number(index),
            route_id: route.route_id,
        });
    }
    map
}

#[derive(Debug, Clone)]
struct AgencyEntry {
    row_number: u64,
    name: CompactString,
}

#[derive(Debug, Clone)]
struct RouteEntry {
    row_number: u64,
    route_id: gtfs_guru_model::StringId,
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
                "agency_id".into(),
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency A".into(),
                agency_url: feed.pool.intern("http://example.com/agency"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "agency_id".into(), "route_url".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                agency_id: Some(feed.pool.intern("A1")),
                route_url: Some(feed.pool.intern("http://example.com/agency")), // Same as agency
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
                "agency_id".into(),
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency A".into(),
                agency_url: feed.pool.intern("http://example.com/agency"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_url".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_url: Some(feed.pool.intern("http://example.com/agency")), // Same as agency
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
            headers: vec!["route_id".into(), "route_url".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_url: Some(feed.pool.intern("http://example.com/route")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_url".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_url: Some(feed.pool.intern("http://example.com/route")), // Same as route
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
                "agency_id".into(),
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![Agency {
                agency_id: Some(feed.pool.intern("A1")),
                agency_name: "Agency A".into(),
                agency_url: feed.pool.intern("http://example.com/agency"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.routes = CsvTable {
            headers: vec!["route_id".into(), "agency_id".into(), "route_url".into()],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                agency_id: Some(feed.pool.intern("A1")),
                route_url: Some(feed.pool.intern("http://example.com/route")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_url".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                stop_url: Some(feed.pool.intern("http://example.com/stop")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        UrlConsistencyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
