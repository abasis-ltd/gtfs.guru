use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_PATHWAY_TO_WRONG_LOCATION_TYPE: &str = "pathway_to_wrong_location_type";
const CODE_PATHWAY_TO_PLATFORM_WITH_BOARDING_AREAS: &str =
    "pathway_to_platform_with_boarding_areas";

#[derive(Debug, Default)]
pub struct PathwayEndpointTypeValidator;

impl Validator for PathwayEndpointTypeValidator {
    fn name(&self) -> &'static str {
        "pathway_endpoint_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        let mut children_by_parent: HashMap<
            gtfs_guru_model::StringId,
            Vec<&gtfs_guru_model::Stop>,
        > = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            if let Some(parent_id) = stop.parent_station {
                if parent_id.0 != 0 {
                    children_by_parent.entry(parent_id).or_default().push(stop);
                }
            }
        }

        for (index, pathway) in pathways.rows.iter().enumerate() {
            let row_number = pathways.row_number(index);
            check_endpoint(
                "from_stop_id",
                pathway.pathway_id,
                pathway.from_stop_id,
                &stops_by_id,
                &children_by_parent,
                row_number,
                feed,
                notices,
            );
            check_endpoint(
                "to_stop_id",
                pathway.pathway_id,
                pathway.to_stop_id,
                &stops_by_id,
                &children_by_parent,
                row_number,
                feed,
                notices,
            );
        }
    }
}

fn check_endpoint(
    field_name: &str,
    pathway_id: gtfs_guru_model::StringId,
    stop_id: gtfs_guru_model::StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
    children_by_parent: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Stop>>,
    row_number: u64,
    feed: &GtfsFeed,
    notices: &mut NoticeContainer,
) {
    if stop_id.0 == 0 {
        return;
    }

    let Some(stop) = stops_by_id.get(&stop_id) else {
        return;
    };

    match stop.location_type.unwrap_or(LocationType::StopOrPlatform) {
        LocationType::StopOrPlatform => {
            if children_by_parent.get(&stop_id).is_some() {
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_TO_PLATFORM_WITH_BOARDING_AREAS,
                    NoticeSeverity::Error,
                    "pathway endpoints should reference boarding areas when a platform has them",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldName", field_name);
                notice.insert_context_field("pathwayId", feed.pool.resolve(pathway_id).as_str());
                notice.insert_context_field("stopId", feed.pool.resolve(stop_id).as_str());
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "fieldName".into(),
                    "pathwayId".into(),
                    "stopId".into(),
                ];
                notices.push(notice);
            }
        }
        LocationType::Station => {
            let mut notice = ValidationNotice::new(
                CODE_PATHWAY_TO_WRONG_LOCATION_TYPE,
                NoticeSeverity::Error,
                "pathway endpoints must not reference stations",
            );
            notice.insert_context_field("csvRowNumber", row_number);
            notice.insert_context_field("fieldName", field_name);
            notice.insert_context_field("pathwayId", feed.pool.resolve(pathway_id).as_str());
            notice.insert_context_field("stopId", feed.pool.resolve(stop_id).as_str());
            notice.field_order = vec![
                "csvRowNumber".into(),
                "fieldName".into(),
                "pathwayId".into(),
                "stopId".into(),
            ];
            notices.push(notice);
        }
        LocationType::EntranceOrExit
        | LocationType::GenericNode
        | LocationType::BoardingArea
        | LocationType::Other => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationType, Pathway, Stop};

    #[test]
    fn detects_pathway_to_station() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                location_type: Some(LocationType::Station),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![Pathway {
                pathway_id: feed.pool.intern("P1"),
                from_stop_id: feed.pool.intern("S1"),
                to_stop_id: feed.pool.intern("N1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayEndpointTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_PATHWAY_TO_WRONG_LOCATION_TYPE
        );
    }

    #[test]
    fn detects_pathway_to_platform_with_boarding_areas() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "location_type".into(),
                "parent_station".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("P1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("BA1"),
                    location_type: Some(LocationType::BoardingArea),
                    parent_station: Some(feed.pool.intern("P1")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![Pathway {
                pathway_id: feed.pool.intern("PW1"),
                from_stop_id: feed.pool.intern("P1"),
                to_stop_id: feed.pool.intern("N1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayEndpointTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_PATHWAY_TO_PLATFORM_WITH_BOARDING_AREAS
        );
    }

    #[test]
    fn passes_valid_endpoints() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("E1"),
                    location_type: Some(LocationType::EntranceOrExit),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("N1"),
                    location_type: Some(LocationType::GenericNode),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("P1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
            ],
            rows: vec![
                Pathway {
                    pathway_id: feed.pool.intern("PW1"),
                    from_stop_id: feed.pool.intern("E1"),
                    to_stop_id: feed.pool.intern("N1"),
                    ..Default::default()
                },
                Pathway {
                    pathway_id: feed.pool.intern("PW2"),
                    from_stop_id: feed.pool.intern("N1"),
                    to_stop_id: feed.pool.intern("P1"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        PathwayEndpointTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
