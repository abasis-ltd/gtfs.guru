//! The specification surface this build supports, as machine-readable data.
//!
//! The spec watcher (`scripts/spec_watch.py`) diffs this against the published
//! GTFS reference and the canonical validator's `rules.json`, so it has to be
//! derived from the same tables validation uses rather than restated by hand.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::csv_schema::schema_for_file;
use crate::csv_validation::enum_values_for_field;
use crate::feed::GTFS_FILE_NAMES;
use crate::notice_schema::build_notice_schema_map;
use crate::spec_baseline::{canonical_baseline_id, spec_revision_id};

#[derive(Debug, Clone, Serialize)]
pub struct FileSurface {
    /// False for files with no column schema, such as `locations.geojson`,
    /// whose fields the watcher must not read as unsupported.
    #[serde(rename = "hasFieldSchema")]
    pub has_field_schema: bool,
    /// Declaration order, which follows the specification's own field order.
    pub fields: Vec<String>,
    #[serde(rename = "requiredFields")]
    pub required_fields: Vec<String>,
    #[serde(rename = "recommendedFields")]
    pub recommended_fields: Vec<String>,
    /// Accepted values per enum column, keyed by field name.
    pub enums: BTreeMap<String, Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecSurface {
    #[serde(rename = "specRevision")]
    pub spec_revision: String,
    #[serde(rename = "canonicalBaseline")]
    pub canonical_baseline: String,
    #[serde(rename = "validatorVersion")]
    pub validator_version: String,
    pub files: BTreeMap<String, FileSurface>,
    /// Notice code to severity, covering every notice this build can emit.
    pub notices: BTreeMap<String, String>,
}

pub fn spec_surface() -> SpecSurface {
    let mut files = BTreeMap::new();
    for name in GTFS_FILE_NAMES {
        let Some(schema) = schema_for_file(name) else {
            files.insert(
                (*name).to_string(),
                FileSurface {
                    has_field_schema: false,
                    fields: Vec::new(),
                    required_fields: Vec::new(),
                    recommended_fields: Vec::new(),
                    enums: BTreeMap::new(),
                },
            );
            continue;
        };
        let mut enums = BTreeMap::new();
        for field in schema.fields {
            if let Some(values) = enum_values_for_field(field) {
                enums.insert((*field).to_string(), values.to_vec());
            }
        }
        files.insert(
            (*name).to_string(),
            FileSurface {
                has_field_schema: true,
                fields: schema.fields.iter().map(|f| (*f).to_string()).collect(),
                required_fields: schema
                    .required_fields
                    .iter()
                    .map(|f| (*f).to_string())
                    .collect(),
                recommended_fields: schema
                    .recommended_fields
                    .iter()
                    .map(|f| (*f).to_string())
                    .collect(),
                enums,
            },
        );
    }

    let notices = build_notice_schema_map()
        .into_iter()
        .map(|(code, schema)| {
            let severity = serde_json::to_value(schema.severity_level)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "UNKNOWN".to_string());
            (code, severity)
        })
        .collect();

    SpecSurface {
        spec_revision: spec_revision_id().to_string(),
        canonical_baseline: canonical_baseline_id().to_string(),
        validator_version: env!("CARGO_PKG_VERSION").to_string(),
        files,
        notices,
    }
}

#[cfg(test)]
mod tests {
    use super::spec_surface;

    #[test]
    fn describes_files_fields_enums_and_notices() {
        let surface = spec_surface();

        let stops = surface.files.get("stops.txt").expect("stops.txt surface");
        assert!(stops.has_field_schema);
        assert_eq!(stops.fields.first().map(String::as_str), Some("stop_id"));
        assert!(stops.required_fields.contains(&"stop_id".to_string()));
        assert_eq!(
            stops.enums.get("location_type").map(Vec::as_slice),
            Some([0, 1, 2, 3, 4].as_slice())
        );

        let geojson = surface
            .files
            .get("locations.geojson")
            .expect("locations.geojson surface");
        assert!(!geojson.has_field_schema);
        assert!(geojson.fields.is_empty());

        assert_eq!(
            surface
                .notices
                .get("missing_required_field")
                .map(String::as_str),
            Some("ERROR")
        );
        assert!(surface.spec_revision.starts_with("google/transit@"));
    }
}
