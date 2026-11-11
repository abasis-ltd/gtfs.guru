use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoticeSchemaSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy)]
pub struct NoticeSchemaEntry {
    pub code: &'static str,
    pub severity: NoticeSchemaSeverity,
    pub fields: &'static [NoticeSchemaFieldEntry],
}

#[derive(Debug, Clone, Copy)]
pub struct NoticeSchemaFieldEntry {
    pub name: &'static str,
    pub field_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldTypeSchema {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<Box<FieldTypeSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
}

impl FieldTypeSchema {
    pub fn from_type_name(name: &str) -> Self {
        let trimmed = name.trim();
        if let Some(inner) = trimmed
            .strip_prefix("array<")
            .and_then(|value| value.strip_suffix('>'))
        {
            let parts: Vec<_> = inner
                .split(',')
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect();
            let mut schema = Self {
                field_type: "array".into(),
                contains: None,
                min_items: None,
                max_items: None,
            };
            if let Some(item_type) = parts.first() {
                schema.contains = Some(Box::new(Self::from_type_name(item_type)));
            }
            if parts.len() == 2 {
                if let Ok(count) = parts[1].parse::<u32>() {
                    schema.min_items = Some(count);
                    schema.max_items = Some(count);
                }
            } else if parts.len() >= 3 {
                if let Ok(min_items) = parts[1].parse::<u32>() {
                    schema.min_items = Some(min_items);
                }
                if let Ok(max_items) = parts[2].parse::<u32>() {
                    schema.max_items = Some(max_items);
                }
            }
            return schema;
        }
        Self {
            field_type: trimmed.to_string(),
            contains: None,
            min_items: None,
            max_items: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldSchema {
    #[serde(flatten)]
    pub field_type: FieldTypeSchema,
    #[serde(rename = "fieldName")]
    pub field_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferencesSchema {
    #[serde(rename = "fileReferences")]
    pub file_references: Vec<String>,
    #[serde(rename = "bestPracticesFileReferences")]
    pub best_practices_file_references: Vec<String>,
    #[serde(rename = "sectionReferences")]
    pub section_references: Vec<String>,
    #[serde(rename = "urlReferences")]
    pub url_references: Vec<UrlReference>,
}

impl ReferencesSchema {
    pub fn is_empty(&self) -> bool {
        self.file_references.is_empty()
            && self.best_practices_file_references.is_empty()
            && self.section_references.is_empty()
            && self.url_references.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UrlReference {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoticeSchema {
    pub code: String,
    #[serde(rename = "severityLevel")]
    pub severity_level: NoticeSchemaSeverity,
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(rename = "shortSummary", skip_serializing_if = "Option::is_none")]
    pub short_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<ReferencesSchema>,
    pub properties: BTreeMap<String, FieldSchema>,
    pub deprecated: bool,
    #[serde(rename = "deprecationReason", skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,
    #[serde(rename = "deprecationVersion", skip_serializing_if = "Option::is_none")]
    pub deprecation_version: Option<String>,
    #[serde(
        rename = "replacementNoticeCodes",
        skip_serializing_if = "Option::is_none"
    )]
    pub replacement_notice_codes: Option<Vec<String>>,
}

impl NoticeSchema {
    pub fn new(code: &str, severity_level: NoticeSchemaSeverity) -> Self {
        Self {
            code: code.to_string(),
            severity_level,
            schema_type: "object".into(),
            short_summary: None,
            description: None,
            references: None,
            properties: BTreeMap::new(),
            deprecated: false,
            deprecation_reason: None,
            deprecation_version: None,
            replacement_notice_codes: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct NoticeMetadata {
    #[serde(rename = "shortSummary")]
    short_summary: Option<String>,
    description: Option<String>,
    references: Option<ReferencesSchema>,
    #[serde(default)]
    properties: BTreeMap<String, FieldMetadata>,
    #[serde(default)]
    deprecated: bool,
    #[serde(rename = "deprecationReason")]
    deprecation_reason: Option<String>,
    #[serde(rename = "deprecationVersion")]
    deprecation_version: Option<String>,
    #[serde(rename = "replacementNoticeCodes")]
    replacement_notice_codes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldMetadata {
    description: Option<String>,
    #[serde(rename = "type")]
    field_type: Option<String>,
    contains: Option<FieldItemMetadata>,
    #[serde(rename = "minItems")]
    min_items: Option<u32>,
    #[serde(rename = "maxItems")]
    max_items: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldItemMetadata {
    #[serde(rename = "type")]
    field_type: String,
}

impl FieldMetadata {
    fn type_schema(&self) -> FieldTypeSchema {
        FieldTypeSchema {
            field_type: self.field_type.clone().unwrap_or_else(|| "string".into()),
            contains: self
                .contains
                .as_ref()
                .map(|item| Box::new(FieldTypeSchema::from_type_name(&item.field_type))),
            min_items: self.min_items,
            max_items: self.max_items,
        }
    }
}

fn notice_metadata() -> &'static BTreeMap<String, NoticeMetadata> {
    static METADATA: OnceLock<BTreeMap<String, NoticeMetadata>> = OnceLock::new();
    METADATA.get_or_init(|| {
        serde_json::from_str(include_str!("../notice_metadata.json"))
            .expect("bundled notice metadata must be valid JSON")
    })
}

pub fn build_notice_schema_map() -> BTreeMap<String, NoticeSchema> {
    let mut map = BTreeMap::new();
    let metadata = notice_metadata();
    for entry in NOTICE_SCHEMA_ENTRIES.iter() {
        let mut schema = NoticeSchema::new(entry.code, entry.severity);
        let notice_metadata = metadata.get(entry.code);
        for field in entry.fields {
            let description = notice_metadata
                .and_then(|notice| notice.properties.get(field.name))
                .and_then(|field| field.description.clone());
            schema.properties.insert(
                field.name.to_string(),
                FieldSchema {
                    field_type: FieldTypeSchema::from_type_name(field.field_type),
                    field_name: field.name.to_string(),
                    description,
                },
            );
        }
        // build.rs reads context fields off `insert_context_field` calls made
        // on the variable that holds the notice. Rules that fill the notice in
        // a helper (`populate_stop_too_far_notice`, the shared CSV field
        // parser) leave it no fields to find, and the published schema then
        // claims the notice carries none. Where the scan found nothing, the
        // canonical metadata is the record of what the notice reports: every
        // code the scan does resolve matches it field for field.
        if entry.fields.is_empty() {
            if let Some(notice) = notice_metadata {
                for (name, field) in &notice.properties {
                    schema.properties.insert(
                        name.clone(),
                        FieldSchema {
                            field_type: field.type_schema(),
                            field_name: name.clone(),
                            description: field.description.clone(),
                        },
                    );
                }
            }
        }
        if let Some(metadata) = metadata.get(entry.code) {
            schema.short_summary = metadata.short_summary.clone();
            schema.description = metadata.description.clone();
            schema.references = metadata
                .references
                .clone()
                .filter(|value| !value.is_empty());
            schema.deprecated = metadata.deprecated;
            schema.deprecation_reason = metadata.deprecation_reason.clone();
            schema.deprecation_version = metadata.deprecation_version.clone();
            schema.replacement_notice_codes = metadata.replacement_notice_codes.clone();
        }
        map.insert(entry.code.to_string(), schema);
    }
    map
}

include!(concat!(env!("OUT_DIR"), "/notice_schema_data.rs"));

#[cfg(test)]
mod tests {
    use super::{build_notice_schema_map, FieldTypeSchema};

    #[test]
    fn parses_array_type() {
        let schema = FieldTypeSchema::from_type_name("array<number,2>");

        assert_eq!(schema.field_type, "array");
        assert_eq!(schema.min_items, Some(2));
        assert_eq!(schema.max_items, Some(2));

        let contains = schema.contains.as_ref().expect("contains schema");
        assert_eq!(contains.field_type, "number");
    }

    #[test]
    fn applies_bundled_notice_metadata() {
        let schemas = build_notice_schema_map();
        let schema = schemas
            .get("missing_required_field")
            .expect("missing_required_field schema");

        assert_eq!(
            schema.short_summary.as_deref(),
            Some("A required field is missing.")
        );
        assert!(schema
            .description
            .as_deref()
            .is_some_and(|value| value.contains("required")));
        assert!(schema
            .properties
            .get("filename")
            .and_then(|field| field.description.as_deref())
            .is_some_and(|value| value.contains("file")));
    }

    #[test]
    fn notices_filled_by_helpers_still_list_their_context_fields() {
        let schemas = build_notice_schema_map();
        let schema = schemas
            .get("stop_too_far_from_shape")
            .expect("stop_too_far_from_shape schema");
        for field in ["stopId", "tripId", "shapeId", "geoDistanceToShape"] {
            assert!(
                schema.properties.contains_key(field),
                "stop_too_far_from_shape lost {field}"
            );
        }
        let location = &schema.properties["match"].field_type;
        assert_eq!(location.field_type, "array");
        assert_eq!(location.min_items, Some(2));
        assert_eq!(
            location
                .contains
                .as_ref()
                .map(|item| item.field_type.as_str()),
            Some("number")
        );
        assert!(schemas["invalid_url"].properties.contains_key("fieldValue"));
    }

    #[test]
    fn no_notice_drops_fields_the_canonical_metadata_lists() {
        let metadata = super::notice_metadata();
        for (code, schema) in build_notice_schema_map() {
            let Some(notice) = metadata.get(&code) else {
                continue;
            };
            if !notice.properties.is_empty() {
                assert!(
                    !schema.properties.is_empty(),
                    "{code} lists no context fields, but the canonical metadata names {}",
                    notice.properties.len()
                );
            }
        }
    }

    #[test]
    fn every_published_notice_has_a_summary_and_description() {
        for (code, schema) in build_notice_schema_map() {
            assert!(
                schema
                    .short_summary
                    .as_deref()
                    .is_some_and(|v| !v.is_empty()),
                "{code} has no short summary"
            );
            assert!(
                schema.description.as_deref().is_some_and(|v| !v.is_empty()),
                "{code} has no description"
            );
        }
    }
}
