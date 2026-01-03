use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::LocationType;

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

        let mut stops_by_id: HashMap<&str, &gtfs_model::Stop> = HashMap::new();
        let mut children_by_parent: HashMap<&str, Vec<&gtfs_model::Stop>> = HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            if let Some(parent_id) = stop.parent_station.as_deref() {
                let parent_id = parent_id.trim();
                if !parent_id.is_empty() {
                    children_by_parent.entry(parent_id).or_default().push(stop);
                }
            }
        }

        for (index, pathway) in pathways.rows.iter().enumerate() {
            let row_number = pathways.row_number(index);
            check_endpoint(
                "from_stop_id",
                pathway.pathway_id.as_str(),
                pathway.from_stop_id.as_str(),
                &stops_by_id,
                &children_by_parent,
                row_number,
                notices,
            );
            check_endpoint(
                "to_stop_id",
                pathway.pathway_id.as_str(),
                pathway.to_stop_id.as_str(),
                &stops_by_id,
                &children_by_parent,
                row_number,
                notices,
            );
        }
    }
}

fn check_endpoint(
    field_name: &str,
    pathway_id: &str,
    stop_id: &str,
    stops_by_id: &HashMap<&str, &gtfs_model::Stop>,
    children_by_parent: &HashMap<&str, Vec<&gtfs_model::Stop>>,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let stop_id = stop_id.trim();
    if stop_id.is_empty() {
        return;
    }

    let Some(stop) = stops_by_id.get(stop_id) else {
        return;
    };

    match stop.location_type.unwrap_or(LocationType::StopOrPlatform) {
        LocationType::StopOrPlatform => {
            if children_by_parent.get(stop_id).is_some() {
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_TO_PLATFORM_WITH_BOARDING_AREAS,
                    NoticeSeverity::Error,
                    "pathway endpoints should reference boarding areas when a platform has them",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("fieldName", field_name);
                notice.insert_context_field("pathwayId", pathway_id);
                notice.insert_context_field("stopId", stop_id);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "fieldName".to_string(),
                    "pathwayId".to_string(),
                    "stopId".to_string(),
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
            notice.insert_context_field("pathwayId", pathway_id);
            notice.insert_context_field("stopId", stop_id);
            notice.field_order = vec![
                "csvRowNumber".to_string(),
                "fieldName".to_string(),
                "pathwayId".to_string(),
                "stopId".to_string(),
            ];
            notices.push(notice);
        }
        LocationType::EntranceOrExit
        | LocationType::GenericNode
        | LocationType::BoardingArea
        | LocationType::Other => {}
    }
}

