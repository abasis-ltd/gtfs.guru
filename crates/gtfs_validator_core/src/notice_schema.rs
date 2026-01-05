use std::collections::BTreeMap;

use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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

pub fn build_notice_schema_map() -> BTreeMap<String, NoticeSchema> {
    let mut map = BTreeMap::new();
    for entry in NOTICE_SCHEMA_ENTRIES.iter() {
        let mut schema = NoticeSchema::new(entry.code, entry.severity);
        for field in entry.fields {
            schema.properties.insert(
                field.name.to_string(),
                FieldSchema {
                    field_type: FieldTypeSchema::from_type_name(field.field_type),
                    field_name: field.name.to_string(),
                    description: None,
                },
            );
        }
        map.insert(entry.code.to_string(), schema);
    }
    map
}

include!(concat!(env!("OUT_DIR"), "/notice_schema_data.rs"));

#[cfg(test)]
mod tests {
    use super::FieldTypeSchema;

    #[test]
    fn parses_array_type() {
        let schema = FieldTypeSchema::from_type_name("array<number,2>");

        assert_eq!(schema.field_type, "array");
        assert_eq!(schema.min_items, Some(2));
        assert_eq!(schema.max_items, Some(2));

        let contains = schema.contains.as_ref().expect("contains schema");
        assert_eq!(contains.field_type, "number");
    }
}
