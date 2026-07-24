use std::collections::{HashMap, HashSet};

use crate::feed::TRANSLATIONS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{GtfsDate, GtfsTime, StringId};

const CODE_MISSING_REQUIRED_FIELD: &str = "missing_required_field";
const CODE_TRANSLATION_UNEXPECTED_VALUE: &str = "translation_unexpected_value";
const CODE_TRANSLATION_UNKNOWN_TABLE_NAME: &str = "translation_unknown_table_name";
const CODE_TRANSLATION_FOREIGN_KEY_VIOLATION: &str = "translation_foreign_key_violation";

#[derive(Debug, Default)]
pub struct TranslationFieldAndReferenceValidator;

impl Validator for TranslationFieldAndReferenceValidator {
    fn name(&self) -> &'static str {
        "translation_field_and_reference"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(translations) = &feed.translations else {
            return;
        };

        if !translations
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("table_name"))
        {
            return;
        }

        if !validate_standard_required_fields(translations, notices) {
            return;
        }

        // Lazy hash indexes over the referenced tables. Without them every
        // translation row linearly scanned its target table (O(rows × table)
        // with a String allocation per comparison) — a bilingual feed like
        // STM Montréal (213k translations × 203k trips) effectively hung.
        let mut existence = ExistenceIndex::new(feed);

        for (index, translation) in translations.rows.iter().enumerate() {
            let row_number = translations.row_number(index);
            validate_translation(translation, feed, &mut existence, row_number, notices);
        }
    }
}

fn validate_standard_required_fields(
    translations: &crate::CsvTable<gtfs_guru_model::Translation>,
    notices: &mut NoticeContainer,
) -> bool {
    let mut is_valid = true;
    for (index, translation) in translations.rows.iter().enumerate() {
        let row_number = translations.row_number(index);
        if translation.table_name.map(|id| id.0 == 0).unwrap_or(true) {
            notices.push(missing_required_field_notice("table_name", row_number));
            is_valid = false;
        }
        if translation.field_name.map(|id| id.0 == 0).unwrap_or(true) {
            notices.push(missing_required_field_notice("field_name", row_number));
            is_valid = false;
        }
        if is_blank_id(translation.language) {
            notices.push(missing_required_field_notice("language", row_number));
            is_valid = false;
        }
    }
    is_valid
}

fn validate_translation(
    translation: &gtfs_guru_model::Translation,
    feed: &GtfsFeed,
    existence: &mut ExistenceIndex,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let table_name_value = translation
        .table_name
        .map(|id| feed.pool.resolve(id))
        .unwrap_or_default();
    let table_name = table_name_value.as_str();
    let record_id = normalized_optional_id(translation.record_id).map(|id| feed.pool.resolve(id));
    let record_sub_id =
        normalized_optional_id(translation.record_sub_id).map(|id| feed.pool.resolve(id));
    let field_value = normalized_optional_str(translation.field_value.as_deref());
    let record_id_value = record_id.as_deref();
    let record_sub_id_value = record_sub_id.as_deref();

    if field_value.is_some() {
        if let Some(value) = record_id_value {
            notices.push(translation_unexpected_value_notice(
                "record_id",
                value,
                row_number,
            ));
        }
        if let Some(value) = record_sub_id_value {
            notices.push(translation_unexpected_value_notice(
                "record_sub_id",
                value,
                row_number,
            ));
        }
    }

    let Some(table_spec) = table_spec(table_name, feed) else {
        notices.push(translation_unknown_table_notice(table_name, row_number));
        return;
    };

    if field_value.is_some() {
        return;
    }

    match table_spec {
        TableSpec::None => {
            if let Some(value) = record_id_value {
                notices.push(translation_unexpected_value_notice(
                    "record_id",
                    value,
                    row_number,
                ));
            }
            if let Some(value) = record_sub_id_value {
                notices.push(translation_unexpected_value_notice(
                    "record_sub_id",
                    value,
                    row_number,
                ));
            }
        }
        TableSpec::One { exists } => {
            let Some(record_id) = record_id_value else {
                notices.push(missing_required_field_notice("record_id", row_number));
                return;
            };
            if let Some(value) = record_sub_id_value {
                notices.push(translation_unexpected_value_notice(
                    "record_sub_id",
                    value,
                    row_number,
                ));
                return;
            }
            if !exists(existence, record_id) {
                notices.push(translation_foreign_key_violation_notice(
                    table_name, record_id, None, row_number,
                ));
            }
        }
        TableSpec::Two { exists } => {
            let Some(record_id) = record_id_value else {
                notices.push(missing_required_field_notice("record_id", row_number));
                return;
            };
            let Some(record_sub_id) = record_sub_id_value else {
                notices.push(missing_required_field_notice("record_sub_id", row_number));
                return;
            };
            if !exists(existence, record_id, record_sub_id) {
                notices.push(translation_foreign_key_violation_notice(
                    table_name,
                    record_id,
                    Some(record_sub_id),
                    row_number,
                ));
            }
        }
    }
}

fn normalized_optional_str(value: Option<&str>) -> Option<&str> {
    value.map(|val| val.trim()).filter(|val| !val.is_empty())
}

fn normalized_optional_id(value: Option<StringId>) -> Option<StringId> {
    value.filter(|id| id.0 != 0)
}

fn is_blank_id(value: StringId) -> bool {
    value.0 == 0
}

fn missing_required_field_notice(field: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MISSING_REQUIRED_FIELD,
        NoticeSeverity::Error,
        "missing required field",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("filename", TRANSLATIONS_FILE);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

fn translation_unexpected_value_notice(
    field: &str,
    value: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_TRANSLATION_UNEXPECTED_VALUE,
        NoticeSeverity::Error,
        format!("field {} must be empty (value={})", field, value),
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field);
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
    ];
    notice
}

fn translation_unknown_table_notice(table_name: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_TRANSLATION_UNKNOWN_TABLE_NAME,
        NoticeSeverity::Warning,
        "translation references unknown table",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("tableName", table_name);
    notice.field_order = vec!["csvRowNumber".into(), "tableName".into()];
    notice
}

fn translation_foreign_key_violation_notice(
    table_name: &str,
    record_id: &str,
    record_sub_id: Option<&str>,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_TRANSLATION_FOREIGN_KEY_VIOLATION,
        NoticeSeverity::Error,
        "translation references missing record",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("recordId", record_id);
    notice.insert_context_field("recordSubId", record_sub_id.unwrap_or(""));
    notice.insert_context_field("tableName", table_name);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "recordId".into(),
        "recordSubId".into(),
        "tableName".into(),
    ];
    notice
}

enum TableSpec {
    None,
    One {
        exists: fn(&mut ExistenceIndex, &str) -> bool,
    },
    Two {
        exists: fn(&mut ExistenceIndex, &str, &str) -> bool,
    },
}

/// Lazy hash indexes for translation foreign-key lookups.
///
/// Each referenced table is indexed at most once (first lookup builds the
/// set, later lookups are O(1)); tables never referenced by translations.txt
/// are never indexed, so feeds without translations pay nothing.
struct ExistenceIndex<'a> {
    feed: &'a GtfsFeed,
    /// table name -> set of trimmed record ids
    one_key: HashMap<&'static str, HashSet<String>>,
    /// table name -> record id -> set of numeric sub-ids (stop_times, shapes)
    two_key_u32: HashMap<&'static str, HashMap<String, HashSet<u32>>>,
    calendar_dates: Option<HashMap<String, HashSet<GtfsDate>>>,
    frequencies: Option<HashMap<String, HashSet<GtfsTime>>>,
    transfers: Option<HashSet<(String, String)>>,
}

impl<'a> ExistenceIndex<'a> {
    fn new(feed: &'a GtfsFeed) -> Self {
        Self {
            feed,
            one_key: HashMap::new(),
            two_key_u32: HashMap::new(),
            calendar_dates: None,
            frequencies: None,
            transfers: None,
        }
    }

    fn resolved(&self, id: StringId) -> String {
        self.feed.pool.resolve(id).trim().to_string()
    }

    fn one_key(&mut self, table: &'static str) -> &HashSet<String> {
        if !self.one_key.contains_key(table) {
            let set = self.build_one_key(table);
            self.one_key.insert(table, set);
        }
        &self.one_key[table]
    }

    fn build_one_key(&self, table: &'static str) -> HashSet<String> {
        let feed = self.feed;
        match table {
            "agency" => feed
                .agency
                .rows
                .iter()
                .filter_map(|agency| agency.agency_id)
                .map(|id| self.resolved(id))
                .collect(),
            "stops" => feed
                .stops
                .rows
                .iter()
                .map(|stop| self.resolved(stop.stop_id))
                .collect(),
            "routes" => feed
                .routes
                .rows
                .iter()
                .map(|route| self.resolved(route.route_id))
                .collect(),
            "trips" => feed
                .trips
                .rows
                .iter()
                .map(|trip| self.resolved(trip.trip_id))
                .collect(),
            "calendar" => feed
                .calendar
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|calendar| self.resolved(calendar.service_id))
                        .collect()
                })
                .unwrap_or_default(),
            "fare_attributes" => feed
                .fare_attributes
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|fare| self.resolved(fare.fare_id))
                        .collect()
                })
                .unwrap_or_default(),
            "levels" => feed
                .levels
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|level| self.resolved(level.level_id))
                        .collect()
                })
                .unwrap_or_default(),
            "pathways" => feed
                .pathways
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|pathway| self.resolved(pathway.pathway_id))
                        .collect()
                })
                .unwrap_or_default(),
            "attributions" => feed
                .attributions
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .filter_map(|attribution| attribution.attribution_id)
                        .filter(|id| id.0 != 0)
                        .map(|id| self.resolved(id))
                        .collect()
                })
                .unwrap_or_default(),
            "areas" => feed
                .areas
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|area| self.resolved(area.area_id))
                        .collect()
                })
                .unwrap_or_default(),
            "fare_media" => feed
                .fare_media
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|media| self.resolved(media.fare_media_id))
                        .collect()
                })
                .unwrap_or_default(),
            "rider_categories" => feed
                .rider_categories
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|category| self.resolved(category.rider_category_id))
                        .collect()
                })
                .unwrap_or_default(),
            "location_groups" => feed
                .location_groups
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|group| self.resolved(group.location_group_id))
                        .collect()
                })
                .unwrap_or_default(),
            "networks" => feed
                .networks
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|network| self.resolved(network.network_id))
                        .collect()
                })
                .unwrap_or_default(),
            "route_networks" => feed
                .route_networks
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|route_network| self.resolved(route_network.route_id))
                        .collect()
                })
                .unwrap_or_default(),
            _ => HashSet::new(),
        }
    }

    fn two_key_u32(&mut self, table: &'static str) -> &HashMap<String, HashSet<u32>> {
        if !self.two_key_u32.contains_key(table) {
            let feed = self.feed;
            let mut map: HashMap<String, HashSet<u32>> = HashMap::new();
            match table {
                "stop_times" => {
                    for stop_time in &feed.stop_times.rows {
                        map.entry(self.resolved(stop_time.trip_id))
                            .or_default()
                            .insert(stop_time.stop_sequence);
                    }
                }
                "shapes" => {
                    if let Some(shapes) = feed.shapes.as_ref() {
                        for shape in &shapes.rows {
                            map.entry(self.resolved(shape.shape_id))
                                .or_default()
                                .insert(shape.shape_pt_sequence);
                        }
                    }
                }
                _ => {}
            }
            self.two_key_u32.insert(table, map);
        }
        &self.two_key_u32[table]
    }

    fn calendar_dates(&mut self) -> &HashMap<String, HashSet<GtfsDate>> {
        if self.calendar_dates.is_none() {
            let mut map: HashMap<String, HashSet<GtfsDate>> = HashMap::new();
            if let Some(table) = self.feed.calendar_dates.as_ref() {
                for calendar_date in &table.rows {
                    map.entry(self.resolved(calendar_date.service_id))
                        .or_default()
                        .insert(calendar_date.date);
                }
            }
            self.calendar_dates = Some(map);
        }
        self.calendar_dates.as_ref().expect("built above")
    }

    fn frequencies(&mut self) -> &HashMap<String, HashSet<GtfsTime>> {
        if self.frequencies.is_none() {
            let mut map: HashMap<String, HashSet<GtfsTime>> = HashMap::new();
            if let Some(table) = self.feed.frequencies.as_ref() {
                for frequency in &table.rows {
                    map.entry(self.resolved(frequency.trip_id))
                        .or_default()
                        .insert(frequency.start_time);
                }
            }
            self.frequencies = Some(map);
        }
        self.frequencies.as_ref().expect("built above")
    }

    fn transfers(&mut self) -> &HashSet<(String, String)> {
        if self.transfers.is_none() {
            let mut set = HashSet::new();
            if let Some(table) = self.feed.transfers.as_ref() {
                for transfer in &table.rows {
                    let from = transfer.from_stop_id.filter(|id| id.0 != 0);
                    let to = transfer.to_stop_id.filter(|id| id.0 != 0);
                    if let (Some(from), Some(to)) = (from, to) {
                        set.insert((self.resolved(from), self.resolved(to)));
                    }
                }
            }
            self.transfers = Some(set);
        }
        self.transfers.as_ref().expect("built above")
    }
}

fn table_spec(table_name: &str, feed: &GtfsFeed) -> Option<TableSpec> {
    match table_name {
        "agency" => Some(TableSpec::One {
            exists: agency_exists,
        }),
        "stops" => Some(TableSpec::One {
            exists: stop_exists,
        }),
        "routes" => Some(TableSpec::One {
            exists: route_exists,
        }),
        "trips" => Some(TableSpec::One {
            exists: trip_exists,
        }),
        "stop_times" => Some(TableSpec::Two {
            exists: stop_time_exists,
        }),
        "calendar" => feed.calendar.as_ref().map(|_| TableSpec::One {
            exists: calendar_exists,
        }),
        "calendar_dates" => feed.calendar_dates.as_ref().map(|_| TableSpec::Two {
            exists: calendar_date_exists,
        }),
        "shapes" => feed.shapes.as_ref().map(|_| TableSpec::Two {
            exists: shape_exists,
        }),
        "frequencies" => feed.frequencies.as_ref().map(|_| TableSpec::Two {
            exists: frequency_exists,
        }),
        "transfers" => feed.transfers.as_ref().map(|_| TableSpec::Two {
            exists: transfer_exists,
        }),
        "fare_attributes" => feed.fare_attributes.as_ref().map(|_| TableSpec::One {
            exists: fare_attribute_exists,
        }),
        "levels" => feed.levels.as_ref().map(|_| TableSpec::One {
            exists: level_exists,
        }),
        "pathways" => feed.pathways.as_ref().map(|_| TableSpec::One {
            exists: pathway_exists,
        }),
        "attributions" => feed.attributions.as_ref().map(|_| TableSpec::One {
            exists: attribution_exists,
        }),
        "areas" => feed.areas.as_ref().map(|_| TableSpec::One {
            exists: area_exists,
        }),
        "fare_media" => feed.fare_media.as_ref().map(|_| TableSpec::One {
            exists: fare_media_exists,
        }),
        "rider_categories" => feed.rider_categories.as_ref().map(|_| TableSpec::One {
            exists: rider_category_exists,
        }),
        "location_groups" => feed.location_groups.as_ref().map(|_| TableSpec::One {
            exists: location_group_exists,
        }),
        "networks" => feed.networks.as_ref().map(|_| TableSpec::One {
            exists: network_exists,
        }),
        "route_networks" => feed.route_networks.as_ref().map(|_| TableSpec::One {
            exists: route_network_exists,
        }),
        "feed_info" => feed.feed_info.as_ref().map(|_| TableSpec::None),
        _ => None,
    }
}

fn agency_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("agency").contains(record_id)
}

fn stop_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("stops").contains(record_id)
}

fn route_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("routes").contains(record_id)
}

fn trip_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("trips").contains(record_id)
}

fn stop_time_exists(index: &mut ExistenceIndex, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(sequence) = record_sub_id.parse::<u32>() else {
        return false;
    };
    index
        .two_key_u32("stop_times")
        .get(record_id)
        .is_some_and(|sequences| sequences.contains(&sequence))
}

fn calendar_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("calendar").contains(record_id)
}

fn calendar_date_exists(index: &mut ExistenceIndex, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(date) = GtfsDate::parse(record_sub_id) else {
        return false;
    };
    index
        .calendar_dates()
        .get(record_id)
        .is_some_and(|dates| dates.contains(&date))
}

fn shape_exists(index: &mut ExistenceIndex, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(sequence) = record_sub_id.parse::<u32>() else {
        return false;
    };
    index
        .two_key_u32("shapes")
        .get(record_id)
        .is_some_and(|sequences| sequences.contains(&sequence))
}

fn frequency_exists(index: &mut ExistenceIndex, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(start_time) = GtfsTime::parse(record_sub_id) else {
        return false;
    };
    index
        .frequencies()
        .get(record_id)
        .is_some_and(|times| times.contains(&start_time))
}

fn transfer_exists(index: &mut ExistenceIndex, record_id: &str, record_sub_id: &str) -> bool {
    index
        .transfers()
        .contains(&(record_id.to_string(), record_sub_id.to_string()))
}

fn fare_attribute_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("fare_attributes").contains(record_id)
}

fn level_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("levels").contains(record_id)
}

fn pathway_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("pathways").contains(record_id)
}

fn attribution_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("attributions").contains(record_id)
}

fn area_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("areas").contains(record_id)
}

fn fare_media_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("fare_media").contains(record_id)
}

fn rider_category_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("rider_categories").contains(record_id)
}

fn location_group_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("location_groups").contains(record_id)
}

fn network_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("networks").contains(record_id)
}

fn route_network_exists(index: &mut ExistenceIndex, record_id: &str) -> bool {
    index.one_key("route_networks").contains(record_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Stop, Translation};

    #[test]
    fn detects_missing_required_fields() {
        let mut feed = GtfsFeed::default();
        feed.translations = Some(CsvTable {
            headers: vec!["table_name".into(), "field_name".into(), "language".into()],
            rows: vec![
                Translation {
                    table_name: Some(feed.pool.intern("stops")),
                    field_name: Some(feed.pool.intern("stop_name")),
                    language: StringId(0), // Missing language
                    ..Default::default()
                },
                Translation {
                    table_name: None, // Missing table_name
                    field_name: Some(feed.pool.intern("stop_name")),
                    language: feed.pool.intern("en"),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        TranslationFieldAndReferenceValidator.validate(&feed, &mut notices);

        assert_eq!(
            notices
                .iter()
                .filter(|n| n.code == CODE_MISSING_REQUIRED_FIELD)
                .count(),
            2
        );
    }

    #[test]
    fn detects_unknown_table_name() {
        let mut feed = GtfsFeed::default();
        feed.translations = Some(CsvTable {
            headers: vec![
                "table_name".into(),
                "field_name".into(),
                "language".into(),
                "record_id".into(),
            ],
            rows: vec![Translation {
                table_name: Some(feed.pool.intern("unknown_table")),
                field_name: Some(feed.pool.intern("field")),
                language: feed.pool.intern("en"),
                record_id: Some(feed.pool.intern("1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TranslationFieldAndReferenceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSLATION_UNKNOWN_TABLE_NAME));
    }

    #[test]
    fn detects_foreign_key_violation() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            ..Default::default()
        };
        feed.translations = Some(CsvTable {
            headers: vec![
                "table_name".into(),
                "field_name".into(),
                "language".into(),
                "record_id".into(),
            ],
            rows: vec![Translation {
                table_name: Some(feed.pool.intern("stops")),
                field_name: Some(feed.pool.intern("stop_name")),
                language: feed.pool.intern("en"),
                record_id: Some(feed.pool.intern("S2")), // Does not exist
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TranslationFieldAndReferenceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSLATION_FOREIGN_KEY_VIOLATION));
    }

    #[test]
    fn passes_valid_translation() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            ..Default::default()
        };
        feed.translations = Some(CsvTable {
            headers: vec![
                "table_name".into(),
                "field_name".into(),
                "language".into(),
                "record_id".into(),
            ],
            rows: vec![Translation {
                table_name: Some(feed.pool.intern("stops")),
                field_name: Some(feed.pool.intern("stop_name")),
                language: feed.pool.intern("en"),
                record_id: Some(feed.pool.intern("S1")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TranslationFieldAndReferenceValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
