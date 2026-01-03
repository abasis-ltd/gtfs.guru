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
        let mut sources_by_id: HashMap<String, HashSet<GeographySource>> = HashMap::new();
        let mut stop_rows: HashMap<&str, u64> = HashMap::new();

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id.trim();
            if stop_id.is_empty() {
                continue;
            }
            stop_rows.entry(stop_id).or_insert(row_number);
            insert_source(&mut sources_by_id, stop_id, GeographySource::Stop);
        }

        let mut location_group_rows: HashMap<&str, u64> = HashMap::new();
        if let Some(location_group_stops) = &feed.location_group_stops {
            for (index, row) in location_group_stops.rows.iter().enumerate() {
                let row_number = location_group_stops.row_number(index);
                let group_id = row.location_group_id.trim();
                if group_id.is_empty() {
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

        let mut feature_index_by_id: HashMap<&str, usize> = HashMap::new();
        if let Some(locations) = &feed.locations {
            if !locations.has_fatal_errors() {
                if locations.feature_index_by_id.is_empty() {
                    for location_id in &locations.location_ids {
                        insert_source(
                            &mut sources_by_id,
                            location_id.as_str(),
                            GeographySource::GeoJson,
                        );
                    }
                } else {
                    for (location_id, index) in &locations.feature_index_by_id {
                        insert_source(
                            &mut sources_by_id,
                            location_id.as_str(),
                            GeographySource::GeoJson,
                        );
                        feature_index_by_id.insert(location_id.as_str(), *index);
                    }
                }
            }
        }

        for (id, sources) in sources_by_id {
            if sources.len() > 1 {
                notices.push(duplicate_id_notice(
                    &id,
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
    sources_by_id: &mut HashMap<String, HashSet<GeographySource>>,
    id: &str,
    source: GeographySource,
) {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return;
    }

    sources_by_id
        .entry(trimmed.to_string())
        .or_default()
        .insert(source);
}

fn duplicate_id_notice(
    id: &str,
    sources: &HashSet<GeographySource>,
    stop_rows: &HashMap<&str, u64>,
    location_group_rows: &HashMap<&str, u64>,
    feature_index_by_id: &HashMap<&str, usize>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_DUPLICATE_GEOGRAPHY_ID,
        NoticeSeverity::Error,
        format!("geography id '{}' is duplicated across multiple files", id),
    );
    if sources.contains(&GeographySource::Stop) {
        if let Some(row_number) = stop_rows.get(id).copied() {
            notice.insert_context_field("csvRowNumberA", row_number);
        }
    }
    if sources.contains(&GeographySource::LocationGroupStop) {
        if let Some(row_number) = location_group_rows.get(id).copied() {
            notice.insert_context_field("csvRowNumberB", row_number);
        }
    }
    if sources.contains(&GeographySource::GeoJson) {
        if let Some(index) = feature_index_by_id.get(id) {
            notice.insert_context_field("featureIndex", *index as u64);
        }
    }
    notice.insert_context_field("geographyId", id);
    notice.field_order = vec![
        "csvRowNumberA".to_string(),
        "csvRowNumberB".to_string(),
        "featureIndex".to_string(),
        "geographyId".to_string(),
    ];
    notice
}

