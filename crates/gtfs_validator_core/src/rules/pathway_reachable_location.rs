use std::collections::{HashMap, HashSet, VecDeque};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::StringId;
use gtfs_guru_model::{Bidirectional, LocationType};

const CODE_PATHWAY_UNREACHABLE_LOCATION: &str = "pathway_unreachable_location";

#[derive(Debug, Default)]
pub struct PathwayReachableLocationValidator;

impl Validator for PathwayReachableLocationValidator {
    fn name(&self) -> &'static str {
        "pathway_reachable_location"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(pathways) = &feed.pathways else {
            return;
        };

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        let mut stop_rows: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();
        let mut children_by_parent: HashMap<
            gtfs_guru_model::StringId,
            Vec<&gtfs_guru_model::Stop>,
        > = HashMap::new();
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
            stop_rows.insert(stop_id, row_number);
            if let Some(parent_id) = stop.parent_station {
                if parent_id.0 != 0 {
                    children_by_parent.entry(parent_id).or_default().push(stop);
                }
            }
        }

        let mut by_from: HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Pathway>> =
            HashMap::new();
        let mut by_to: HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Pathway>> =
            HashMap::new();
        for pathway in &pathways.rows {
            let from_id = pathway.from_stop_id;
            if from_id.0 != 0 {
                by_from.entry(from_id).or_default().push(pathway);
            }
            let to_id = pathway.to_stop_id;
            if to_id.0 != 0 {
                by_to.entry(to_id).or_default().push(pathway);
            }
        }

        let mut pathway_endpoints: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        pathway_endpoints.extend(by_from.keys().copied());
        pathway_endpoints.extend(by_to.keys().copied());

        let stations_with_pathways = find_stations_with_pathways(&pathway_endpoints, &stops_by_id);
        if stations_with_pathways.is_empty() {
            return;
        }

        let locations_having_entrances = traverse_pathways(
            SearchDirection::FromEntrances,
            &feed.stops.rows,
            &by_from,
            &by_to,
        );
        let locations_having_exits =
            traverse_pathways(SearchDirection::ToExits, &feed.stops.rows, &by_from, &by_to);

        for location in &feed.stops.rows {
            let stop_id = location.stop_id;
            if stop_id.0 == 0 {
                continue;
            }

            let Some(station) = including_station(stop_id, &stops_by_id) else {
                continue;
            };
            let station_id = station.stop_id;
            if station_id.0 == 0 || !stations_with_pathways.contains(&station_id) {
                continue;
            }

            let location_type = location
                .location_type
                .unwrap_or(LocationType::StopOrPlatform);
            let is_platform_without_boarding_areas = location_type == LocationType::StopOrPlatform
                && children_by_parent.get(&stop_id).is_none();
            let should_check = location_type == LocationType::GenericNode
                || location_type == LocationType::BoardingArea
                || is_platform_without_boarding_areas;
            if !should_check {
                continue;
            }

            let has_entrance = locations_having_entrances.contains(&stop_id);
            let has_exit = locations_having_exits.contains(&stop_id);
            if !(has_entrance && has_exit) {
                let stop_id_value = feed.pool.resolve(stop_id);
                let parent_station_value = feed
                    .pool
                    .resolve(location.parent_station.unwrap_or(StringId(0)));
                let mut notice = ValidationNotice::new(
                    CODE_PATHWAY_UNREACHABLE_LOCATION,
                    NoticeSeverity::Error,
                    "location is not reachable from entrances or to exits",
                );
                let row_number = stop_rows.get(&stop_id).copied().unwrap_or(2);
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("hasEntrance", has_entrance);
                notice.insert_context_field("hasExit", has_exit);
                notice.insert_context_field("locationType", location_type_value(location_type));
                notice.insert_context_field("parentStation", parent_station_value.as_str());
                notice.insert_context_field("stopId", stop_id_value.as_str());
                notice
                    .insert_context_field("stopName", location.stop_name.as_deref().unwrap_or(""));
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "hasEntrance".into(),
                    "hasExit".into(),
                    "locationType".into(),
                    "parentStation".into(),
                    "stopId".into(),
                    "stopName".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchDirection {
    FromEntrances,
    ToExits,
}

fn including_station<'a>(
    stop_id: gtfs_guru_model::StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &'a gtfs_guru_model::Stop>,
) -> Option<&'a gtfs_guru_model::Stop> {
    let mut current = stop_id;
    for _ in 0..3 {
        let stop = *stops_by_id.get(&current)?;
        if stop.location_type == Some(LocationType::Station) {
            return Some(stop);
        }
        current = match stop.parent_station.filter(|id| id.0 != 0) {
            Some(parent_id) => parent_id,
            None => break,
        };
    }
    None
}

fn find_stations_with_pathways(
    pathway_endpoints: &HashSet<gtfs_guru_model::StringId>,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
) -> HashSet<gtfs_guru_model::StringId> {
    let mut stations_with_pathways = HashSet::new();
    for stop_id in pathway_endpoints {
        if let Some(station) = including_station(*stop_id, stops_by_id) {
            let station_id = station.stop_id;
            if station_id.0 != 0 {
                stations_with_pathways.insert(station_id);
            }
        }
    }
    stations_with_pathways
}

fn traverse_pathways(
    direction: SearchDirection,
    stops: &[gtfs_guru_model::Stop],
    by_from: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Pathway>>,
    by_to: &HashMap<gtfs_guru_model::StringId, Vec<&gtfs_guru_model::Pathway>>,
) -> HashSet<gtfs_guru_model::StringId> {
    let mut visited: HashSet<gtfs_guru_model::StringId> = HashSet::new();
    let mut queue: VecDeque<gtfs_guru_model::StringId> = VecDeque::new();

    for stop in stops {
        if stop.location_type == Some(LocationType::EntranceOrExit) {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            if visited.insert(stop_id) {
                queue.push_back(stop_id);
            }
        }
    }

    while let Some(curr) = queue.pop_front() {
        if let Some(pathways) = by_from.get(&curr) {
            for pathway in pathways {
                if direction == SearchDirection::FromEntrances
                    || pathway.is_bidirectional == Bidirectional::Bidirectional
                {
                    maybe_visit(pathway.to_stop_id, &mut visited, &mut queue);
                }
            }
        }
        if let Some(pathways) = by_to.get(&curr) {
            for pathway in pathways {
                if direction == SearchDirection::ToExits
                    || pathway.is_bidirectional == Bidirectional::Bidirectional
                {
                    maybe_visit(pathway.from_stop_id, &mut visited, &mut queue);
                }
            }
        }
    }

    visited
}

fn location_type_value(location_type: LocationType) -> i32 {
    match location_type {
        LocationType::StopOrPlatform => 0,
        LocationType::Station => 1,
        LocationType::EntranceOrExit => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Other => -1,
    }
}

fn maybe_visit(
    stop_id: gtfs_guru_model::StringId,
    visited: &mut HashSet<gtfs_guru_model::StringId>,
    queue: &mut VecDeque<gtfs_guru_model::StringId>,
) {
    if stop_id.0 == 0 {
        return;
    }
    if visited.insert(stop_id) {
        queue.push_back(stop_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Bidirectional, LocationType, Pathway, Stop};

    #[test]
    fn detects_unreachable_location() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "location_type".into(),
                "parent_station".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("ST1"),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("E1"),
                    location_type: Some(LocationType::EntranceOrExit),
                    parent_station: Some(feed.pool.intern("ST1")),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("P1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    parent_station: Some(feed.pool.intern("ST1")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        // Pathway from E1 to P1, but no way back or to an exit
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
                "is_bidirectional".into(),
            ],
            rows: vec![Pathway {
                pathway_id: feed.pool.intern("PW1"),
                from_stop_id: feed.pool.intern("E1"),
                to_stop_id: feed.pool.intern("P1"),
                is_bidirectional: Bidirectional::Unidirectional,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayReachableLocationValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_PATHWAY_UNREACHABLE_LOCATION);
        assert_eq!(
            notice.context.get("hasEntrance").unwrap().as_bool(),
            Some(true)
        );
        assert_eq!(
            notice.context.get("hasExit").unwrap().as_bool(),
            Some(false)
        );
    }

    #[test]
    fn passes_reachable_location() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![
                "stop_id".into(),
                "location_type".into(),
                "parent_station".into(),
            ],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("ST1"),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("E1"),
                    location_type: Some(LocationType::EntranceOrExit),
                    parent_station: Some(feed.pool.intern("ST1")),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("P1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    parent_station: Some(feed.pool.intern("ST1")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3, 4],
        };
        // Bidirectional pathway between E1 and P1
        feed.pathways = Some(CsvTable {
            headers: vec![
                "pathway_id".into(),
                "from_stop_id".into(),
                "to_stop_id".into(),
                "is_bidirectional".into(),
            ],
            rows: vec![Pathway {
                pathway_id: feed.pool.intern("PW1"),
                from_stop_id: feed.pool.intern("E1"),
                to_stop_id: feed.pool.intern("P1"),
                is_bidirectional: Bidirectional::Bidirectional,
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        PathwayReachableLocationValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
