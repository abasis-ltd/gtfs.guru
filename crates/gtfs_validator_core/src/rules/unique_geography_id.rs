use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_DUPLICATE_GEOGRAPHY_ID: &str = "duplicate_geography_id";

#[derive(Debug, Default)]
pub struct UniqueGeographyIdValidator;

impl Validator for UniqueGeographyIdValidator {
    fn name(&self) -> &'static str {
        "unique_geography_id"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let mut sources_by_id: HashMap<gtfs_guru_model::StringId, HashSet<GeographySource>> =
            HashMap::new();
        let mut stop_rows: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stop_rows.entry(stop_id).or_insert(row_number);
            insert_source(&mut sources_by_id, stop_id, GeographySource::Stop);
        }

        let mut location_group_rows: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();
        if let Some(location_group_stops) = &feed.location_group_stops {
            for (index, row) in location_group_stops.rows.iter().enumerate() {
                let row_number = location_group_stops.row_number(index);
                let group_id = row.location_group_id;
                if group_id.0 == 0 {
                    continue;
                }
                location_group_rows.entry(group_id).or_insert(row_number);
                insert_source(
                    &mut sources_by_id,
                    group_id,
                    GeographySource::LocationGroupStop,
                );
            }
        }

        let mut feature_index_by_id: HashMap<gtfs_guru_model::StringId, usize> = HashMap::new();
        if let Some(locations) = &feed.locations {
            if !locations.has_fatal_errors() {
                if locations.feature_index_by_id.is_empty() {
                    for location_id in &locations.location_ids {
                        insert_source(&mut sources_by_id, *location_id, GeographySource::GeoJson);
                    }
                } else {
                    for (location_id, index) in &locations.feature_index_by_id {
                        insert_source(&mut sources_by_id, *location_id, GeographySource::GeoJson);
                        feature_index_by_id.insert(*location_id, *index);
                    }
                }
            }
        }

        for (id, sources) in sources_by_id {
            if sources.len() > 1 {
                let id_value = feed.pool.resolve(id);
                notices.push(duplicate_id_notice(
                    id,
                    id_value.as_str(),
                    &sources,
                    &stop_rows,
                    &location_group_rows,
                    &feature_index_by_id,
                ));
            }
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
enum GeographySource {
    Stop,
    LocationGroupStop,
    GeoJson,
}

fn insert_source(
    sources_by_id: &mut HashMap<gtfs_guru_model::StringId, HashSet<GeographySource>>,
    id: gtfs_guru_model::StringId,
    source: GeographySource,
) {
    if id.0 == 0 {
        return;
    }

    sources_by_id.entry(id).or_default().insert(source);
}

fn duplicate_id_notice(
    id: gtfs_guru_model::StringId,
    id_value: &str,
    sources: &HashSet<GeographySource>,
    stop_rows: &HashMap<gtfs_guru_model::StringId, u64>,
    location_group_rows: &HashMap<gtfs_guru_model::StringId, u64>,
    feature_index_by_id: &HashMap<gtfs_guru_model::StringId, usize>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_DUPLICATE_GEOGRAPHY_ID,
        NoticeSeverity::Error,
        format!(
            "geography id '{}' is duplicated across multiple files",
            id_value
        ),
    );
    if sources.contains(&GeographySource::Stop) {
        if let Some(row_number) = stop_rows.get(&id).copied() {
            notice.insert_context_field("csvRowNumberA", row_number);
        }
    }
    if sources.contains(&GeographySource::LocationGroupStop) {
        if let Some(row_number) = location_group_rows.get(&id).copied() {
            notice.insert_context_field("csvRowNumberB", row_number);
        }
    }
    if sources.contains(&GeographySource::GeoJson) {
        if let Some(index) = feature_index_by_id.get(&id) {
            notice.insert_context_field("featureIndex", *index as u64);
        }
    }
    notice.insert_context_field("geographyId", id_value);
    notice.field_order = vec![
        "csvRowNumberA".into(),
        "csvRowNumberB".into(),
        "featureIndex".into(),
        "geographyId".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationGroupStop, Stop};

    #[test]
    fn detects_duplicate_id_between_stops_and_location_groups() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("ID1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.location_group_stops = Some(CsvTable {
            headers: vec!["location_group_id".into()],
            rows: vec![LocationGroupStop {
                location_group_id: feed.pool.intern("ID1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        UniqueGeographyIdValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_DUPLICATE_GEOGRAPHY_ID));
    }

    #[test]
    fn passes_unique_ids() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.location_group_stops = Some(CsvTable {
            headers: vec!["location_group_id".into()],
            rows: vec![LocationGroupStop {
                location_group_id: feed.pool.intern("LG1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        UniqueGeographyIdValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
