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

        for (index, translation) in translations.rows.iter().enumerate() {
            let row_number = translations.row_number(index);
            validate_translation(translation, feed, row_number, notices);
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
            if !exists(feed, record_id) {
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
            if !exists(feed, record_id, record_sub_id) {
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
        exists: fn(&GtfsFeed, &str) -> bool,
    },
    Two {
        exists: fn(&GtfsFeed, &str, &str) -> bool,
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

fn agency_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.agency
        .rows
        .iter()
        .filter_map(|agency| agency.agency_id)
        .any(|value| feed.pool.resolve(value).trim() == record_id)
}

fn stop_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.stops
        .rows
        .iter()
        .any(|stop| feed.pool.resolve(stop.stop_id).trim() == record_id)
}

fn route_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.routes
        .rows
        .iter()
        .any(|route| feed.pool.resolve(route.route_id).trim() == record_id)
}

fn trip_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.trips
        .rows
        .iter()
        .any(|trip| feed.pool.resolve(trip.trip_id).trim() == record_id)
}

fn stop_time_exists(feed: &GtfsFeed, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(sequence) = record_sub_id.parse::<u32>() else {
        return false;
    };
    feed.stop_times.rows.iter().any(|stop_time| {
        feed.pool.resolve(stop_time.trip_id).trim() == record_id
            && stop_time.stop_sequence == sequence
    })
}

fn calendar_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.calendar
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|calendar| feed.pool.resolve(calendar.service_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn calendar_date_exists(feed: &GtfsFeed, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(date) = GtfsDate::parse(record_sub_id) else {
        return false;
    };
    feed.calendar_dates
        .as_ref()
        .map(|table| {
            table.rows.iter().any(|calendar_date| {
                feed.pool.resolve(calendar_date.service_id).trim() == record_id
                    && calendar_date.date == date
            })
        })
        .unwrap_or(false)
}

fn shape_exists(feed: &GtfsFeed, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(sequence) = record_sub_id.parse::<u32>() else {
        return false;
    };
    feed.shapes
        .as_ref()
        .map(|table| {
            table.rows.iter().any(|shape| {
                feed.pool.resolve(shape.shape_id).trim() == record_id
                    && shape.shape_pt_sequence == sequence
            })
        })
        .unwrap_or(false)
}

fn frequency_exists(feed: &GtfsFeed, record_id: &str, record_sub_id: &str) -> bool {
    let Ok(start_time) = GtfsTime::parse(record_sub_id) else {
        return false;
    };
    feed.frequencies
        .as_ref()
        .map(|table| {
            table.rows.iter().any(|frequency| {
                feed.pool.resolve(frequency.trip_id).trim() == record_id
                    && frequency.start_time == start_time
            })
        })
        .unwrap_or(false)
}

fn transfer_exists(feed: &GtfsFeed, record_id: &str, record_sub_id: &str) -> bool {
    feed.transfers
        .as_ref()
        .map(|table| {
            table.rows.iter().any(|transfer| {
                let from_matches = transfer
                    .from_stop_id
                    .filter(|id| id.0 != 0)
                    .map(|id| feed.pool.resolve(id))
                    .map(|value| value.trim() == record_id)
                    .unwrap_or(false);
                let to_matches = transfer
                    .to_stop_id
                    .filter(|id| id.0 != 0)
                    .map(|id| feed.pool.resolve(id))
                    .map(|value| value.trim() == record_sub_id)
                    .unwrap_or(false);
                from_matches && to_matches
            })
        })
        .unwrap_or(false)
}

fn fare_attribute_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.fare_attributes
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|fare_attribute| feed.pool.resolve(fare_attribute.fare_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn level_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.levels
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|level| feed.pool.resolve(level.level_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn pathway_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.pathways
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|pathway| feed.pool.resolve(pathway.pathway_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn attribution_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.attributions
        .as_ref()
        .map(|table| {
            table.rows.iter().any(|attribution| {
                attribution
                    .attribution_id
                    .filter(|id| id.0 != 0)
                    .map(|id| feed.pool.resolve(id))
                    .map(|value| value.trim() == record_id)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn area_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.areas
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|area| feed.pool.resolve(area.area_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn fare_media_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.fare_media
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|media| feed.pool.resolve(media.fare_media_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn rider_category_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.rider_categories
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|category| feed.pool.resolve(category.rider_category_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn location_group_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.location_groups
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|group| feed.pool.resolve(group.location_group_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn network_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.networks
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|network| feed.pool.resolve(network.network_id).trim() == record_id)
        })
        .unwrap_or(false)
}

fn route_network_exists(feed: &GtfsFeed, record_id: &str) -> bool {
    feed.route_networks
        .as_ref()
        .map(|table| {
            table
                .rows
                .iter()
                .any(|route_network| feed.pool.resolve(route_network.route_id).trim() == record_id)
        })
        .unwrap_or(false)
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
