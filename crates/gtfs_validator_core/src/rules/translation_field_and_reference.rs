use crate::feed::TRANSLATIONS_FILE;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{GtfsDate, GtfsTime, StringId};
use rustc_hash::FxHashSet;

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

        let mut indexes = RefIndexes::default();
        for (index, translation) in translations.rows.iter().enumerate() {
            let row_number = translations.row_number(index);
            validate_translation(translation, feed, &mut indexes, row_number, notices);
        }
    }
}

/// Lazily-built lookup sets for foreign-key checks.
///
/// The string pool interns trimmed strings, so two ids resolve to the same
/// trimmed string if and only if the ids are equal. That lets every
/// `resolve(id).trim() == record_id` scan be replaced by a `StringId` set
/// lookup without changing which notices are produced.
#[derive(Default)]
struct RefIndexes {
    agency: Option<FxHashSet<StringId>>,
    stops: Option<FxHashSet<StringId>>,
    routes: Option<FxHashSet<StringId>>,
    trips: Option<FxHashSet<StringId>>,
    stop_times: Option<FxHashSet<(StringId, u32)>>,
    calendar: Option<FxHashSet<StringId>>,
    calendar_dates: Option<FxHashSet<(StringId, GtfsDate)>>,
    shapes: Option<FxHashSet<(StringId, u32)>>,
    frequencies: Option<FxHashSet<(StringId, GtfsTime)>>,
    transfers: Option<FxHashSet<(StringId, StringId)>>,
    fare_attributes: Option<FxHashSet<StringId>>,
    levels: Option<FxHashSet<StringId>>,
    pathways: Option<FxHashSet<StringId>>,
    attributions: Option<FxHashSet<StringId>>,
    areas: Option<FxHashSet<StringId>>,
    fare_media: Option<FxHashSet<StringId>>,
    rider_categories: Option<FxHashSet<StringId>>,
    location_groups: Option<FxHashSet<StringId>>,
    networks: Option<FxHashSet<StringId>>,
    route_networks: Option<FxHashSet<StringId>>,
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
    indexes: &mut RefIndexes,
    row_number: u64,
    notices: &mut NoticeContainer,
) {
    let table_name_value = translation
        .table_name
        .map(|id| feed.pool.resolve(id))
        .unwrap_or_default();
    let table_name = table_name_value.as_str();
    let record_id_id = normalized_optional_id(translation.record_id);
    let record_sub_id_id = normalized_optional_id(translation.record_sub_id);
    let record_id = record_id_id.map(|id| feed.pool.resolve(id));
    let record_sub_id = record_sub_id_id.map(|id| feed.pool.resolve(id));
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
            let (Some(record_id), Some(record_id_str)) = (record_id_id, record_id_value) else {
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
            if !exists(feed, indexes, record_id) {
                notices.push(translation_foreign_key_violation_notice(
                    table_name,
                    record_id_str,
                    None,
                    row_number,
                ));
            }
        }
        TableSpec::Two { exists } => {
            let (Some(record_id), Some(record_id_str)) = (record_id_id, record_id_value) else {
                notices.push(missing_required_field_notice("record_id", row_number));
                return;
            };
            let (Some(record_sub_id), Some(record_sub_id_str)) =
                (record_sub_id_id, record_sub_id_value)
            else {
                notices.push(missing_required_field_notice("record_sub_id", row_number));
                return;
            };
            if !exists(feed, indexes, record_id, record_sub_id, record_sub_id_str) {
                notices.push(translation_foreign_key_violation_notice(
                    table_name,
                    record_id_str,
                    Some(record_sub_id_str),
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
        exists: fn(&GtfsFeed, &mut RefIndexes, StringId) -> bool,
    },
    Two {
        // Args: record_id, record_sub_id, resolved record_sub_id string
        // (the string form is needed where the sub id is parsed, e.g. a
        // stop_sequence number, a date or a time).
        exists: fn(&GtfsFeed, &mut RefIndexes, StringId, StringId, &str) -> bool,
    },
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

fn agency_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .agency
        .get_or_insert_with(|| {
            feed.agency
                .rows
                .iter()
                .filter_map(|agency| agency.agency_id)
                .collect()
        })
        .contains(&record_id)
}

fn stop_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .stops
        .get_or_insert_with(|| feed.stops.rows.iter().map(|stop| stop.stop_id).collect())
        .contains(&record_id)
}

fn route_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .routes
        .get_or_insert_with(|| feed.routes.rows.iter().map(|route| route.route_id).collect())
        .contains(&record_id)
}

fn trip_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .trips
        .get_or_insert_with(|| feed.trips.rows.iter().map(|trip| trip.trip_id).collect())
        .contains(&record_id)
}

fn stop_time_exists(
    feed: &GtfsFeed,
    indexes: &mut RefIndexes,
    record_id: StringId,
    _record_sub_id: StringId,
    record_sub_id_str: &str,
) -> bool {
    let Ok(sequence) = record_sub_id_str.parse::<u32>() else {
        return false;
    };
    indexes
        .stop_times
        .get_or_insert_with(|| {
            feed.stop_times
                .rows
                .iter()
                .map(|stop_time| (stop_time.trip_id, stop_time.stop_sequence))
                .collect()
        })
        .contains(&(record_id, sequence))
}

fn calendar_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .calendar
        .get_or_insert_with(|| {
            feed.calendar
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|calendar| calendar.service_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn calendar_date_exists(
    feed: &GtfsFeed,
    indexes: &mut RefIndexes,
    record_id: StringId,
    _record_sub_id: StringId,
    record_sub_id_str: &str,
) -> bool {
    let Ok(date) = GtfsDate::parse(record_sub_id_str) else {
        return false;
    };
    indexes
        .calendar_dates
        .get_or_insert_with(|| {
            feed.calendar_dates
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|calendar_date| (calendar_date.service_id, calendar_date.date))
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&(record_id, date))
}

fn shape_exists(
    feed: &GtfsFeed,
    indexes: &mut RefIndexes,
    record_id: StringId,
    _record_sub_id: StringId,
    record_sub_id_str: &str,
) -> bool {
    let Ok(sequence) = record_sub_id_str.parse::<u32>() else {
        return false;
    };
    indexes
        .shapes
        .get_or_insert_with(|| {
            feed.shapes
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|shape| (shape.shape_id, shape.shape_pt_sequence))
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&(record_id, sequence))
}

fn frequency_exists(
    feed: &GtfsFeed,
    indexes: &mut RefIndexes,
    record_id: StringId,
    _record_sub_id: StringId,
    record_sub_id_str: &str,
) -> bool {
    let Ok(start_time) = GtfsTime::parse(record_sub_id_str) else {
        return false;
    };
    indexes
        .frequencies
        .get_or_insert_with(|| {
            feed.frequencies
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|frequency| (frequency.trip_id, frequency.start_time))
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&(record_id, start_time))
}

fn transfer_exists(
    feed: &GtfsFeed,
    indexes: &mut RefIndexes,
    record_id: StringId,
    record_sub_id: StringId,
    _record_sub_id_str: &str,
) -> bool {
    indexes
        .transfers
        .get_or_insert_with(|| {
            feed.transfers
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .filter_map(|transfer| {
                            let from = transfer.from_stop_id.filter(|id| id.0 != 0)?;
                            let to = transfer.to_stop_id.filter(|id| id.0 != 0)?;
                            Some((from, to))
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&(record_id, record_sub_id))
}

fn fare_attribute_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .fare_attributes
        .get_or_insert_with(|| {
            feed.fare_attributes
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|fare_attribute| fare_attribute.fare_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn level_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .levels
        .get_or_insert_with(|| {
            feed.levels
                .as_ref()
                .map(|table| table.rows.iter().map(|level| level.level_id).collect())
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn pathway_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .pathways
        .get_or_insert_with(|| {
            feed.pathways
                .as_ref()
                .map(|table| table.rows.iter().map(|pathway| pathway.pathway_id).collect())
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn attribution_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .attributions
        .get_or_insert_with(|| {
            feed.attributions
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .filter_map(|attribution| {
                            attribution.attribution_id.filter(|id| id.0 != 0)
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn area_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .areas
        .get_or_insert_with(|| {
            feed.areas
                .as_ref()
                .map(|table| table.rows.iter().map(|area| area.area_id).collect())
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn fare_media_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .fare_media
        .get_or_insert_with(|| {
            feed.fare_media
                .as_ref()
                .map(|table| table.rows.iter().map(|media| media.fare_media_id).collect())
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn rider_category_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .rider_categories
        .get_or_insert_with(|| {
            feed.rider_categories
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|category| category.rider_category_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn location_group_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .location_groups
        .get_or_insert_with(|| {
            feed.location_groups
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|group| group.location_group_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn network_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .networks
        .get_or_insert_with(|| {
            feed.networks
                .as_ref()
                .map(|table| table.rows.iter().map(|network| network.network_id).collect())
                .unwrap_or_default()
        })
        .contains(&record_id)
}

fn route_network_exists(feed: &GtfsFeed, indexes: &mut RefIndexes, record_id: StringId) -> bool {
    indexes
        .route_networks
        .get_or_insert_with(|| {
            feed.route_networks
                .as_ref()
                .map(|table| {
                    table
                        .rows
                        .iter()
                        .map(|route_network| route_network.route_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .contains(&record_id)
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
