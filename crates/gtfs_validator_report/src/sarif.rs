//! SARIF (Static Analysis Results Interchange Format) output for CI/CD integration.
//!
//! SARIF is a standard format for static analysis results supported by:
//! - GitHub Actions (automatic PR annotations)
//! - GitLab CI (Code Quality reports)
//! - VS Code (SARIF Viewer extension)
//! - SonarQube (result import)

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use gtfs_guru_core::{NoticeContainer, NoticeSeverity, ValidationNotice};

/// SARIF schema URL
const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";

/// SARIF version
const SARIF_VERSION: &str = "2.1.0";

/// Top-level SARIF report structure
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifReport {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

/// A single run of the tool
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

/// Tool information
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifTool {
    pub driver: SarifDriver,
}

/// Tool driver (the actual analyzer)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub information_uri: Option<String>,
    pub rules: Vec<SarifRule>,
}

/// Rule definition
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    pub id: String,
    pub name: String,
    pub short_description: SarifMessage,
    pub default_configuration: SarifRuleConfiguration,
}

/// Rule configuration
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRuleConfiguration {
    pub level: SarifLevel,
}

/// SARIF severity levels
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SarifLevel {
    Error,
    Warning,
    Note,
}

impl From<NoticeSeverity> for SarifLevel {
    fn from(severity: NoticeSeverity) -> Self {
        match severity {
            NoticeSeverity::Error => SarifLevel::Error,
            NoticeSeverity::Warning => SarifLevel::Warning,
            NoticeSeverity::Info => SarifLevel::Note,
        }
    }
}

/// A single result (validation notice)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    pub rule_id: String,
    pub level: SarifLevel,
    pub message: SarifMessage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<SarifLocation>,
}

/// Message with text content
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifMessage {
    pub text: String,
}

/// Physical location in a file
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    pub physical_location: SarifPhysicalLocation,
}

/// Physical location details
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    pub artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

/// Artifact (file) location
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifArtifactLocation {
    pub uri: String,
}

/// Region within a file
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u64>,
}

impl SarifReport {
    /// Create a SARIF report from validation notices
    pub fn from_notices(notices: &NoticeContainer) -> Self {
        let mut rules_map: HashMap<String, (NoticeSeverity, String)> = HashMap::new();
        let mut results = Vec::new();

        for notice in notices.iter() {
            // Collect unique rules
            rules_map
                .entry(notice.code.clone())
                .or_insert((notice.severity, notice.message.clone()));

            // Build result
            results.push(notice_to_result(notice));
        }

        // Build rules list
        let rules: Vec<SarifRule> = rules_map
            .into_iter()
            .map(|(code, (severity, message))| SarifRule {
                id: code.clone(),
                name: code_to_name(&code),
                short_description: SarifMessage { text: message },
                default_configuration: SarifRuleConfiguration {
                    level: severity.into(),
                },
            })
            .collect();

        SarifReport {
            schema: SARIF_SCHEMA.to_string(),
            version: SARIF_VERSION.to_string(),
            runs: vec![SarifRun {
                tool: SarifTool {
                    driver: SarifDriver {
                        name: "gtfs-validator-rs".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        information_uri: Some(
                            "https://github.com/MobilityData/gtfs-validator".to_string(),
                        ),
                        rules,
                    },
                },
                results,
            }],
        }
    }

    /// Write SARIF report to a file
    pub fn write<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let path = path.as_ref();
        let json = serde_json::to_string_pretty(self).context("serialize SARIF report")?;
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, format!("{}\n", json))
            .with_context(|| format!("write SARIF report to {}", temp_path.display()))?;
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "replace SARIF report at {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        Ok(())
    }
}

/// Convert a notice to a SARIF result
fn notice_to_result(notice: &ValidationNotice) -> SarifResult {
    let locations = if let Some(file) = &notice.file {
        vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation { uri: file.clone() },
                region: notice.row.map(|line| SarifRegion {
                    start_line: Some(line),
                    start_column: None,
                }),
            },
        }]
    } else {
        Vec::new()
    };

    // Build a more descriptive message including context
    let message_text = build_result_message(notice);

    SarifResult {
        rule_id: notice.code.clone(),
        level: notice.severity.into(),
        message: SarifMessage { text: message_text },
        locations,
    }
}

/// Build a descriptive message from notice and its context
fn build_result_message(notice: &ValidationNotice) -> String {
    if notice.context.is_empty() {
        return notice.message.clone();
    }

    let mut parts = vec![notice.message.clone()];

    // Add relevant context fields to the message
    for (key, value) in &notice.context {
        // Skip fields that are already represented in location
        if key == "filename" || key == "csvRowNumber" || key == "fieldName" {
            continue;
        }

        let value_str = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "null".to_string(),
            other => other.to_string(),
        };

        if !value_str.is_empty() && value_str != "null" {
            parts.push(format!("{}: {}", key, value_str));
        }
    }

    parts.join(" | ")
}

/// Convert snake_case code to PascalCase name
fn code_to_name(code: &str) -> String {
    code.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtfs_guru_core::NoticeContainer;

    #[test]
    fn test_empty_report() {
        let container = NoticeContainer::new();
        let report = SarifReport::from_notices(&container);

        assert_eq!(report.version, "2.1.0");
        assert_eq!(report.runs.len(), 1);
        assert!(report.runs[0].results.is_empty());
    }

    #[test]
    fn test_notice_with_location() {
        let mut container = NoticeContainer::new();
        let mut notice = ValidationNotice::new(
            "stop_lat_out_of_range",
            NoticeSeverity::Error,
            "stop_lat must be between -90 and 90",
        );
        notice.file = Some("stops.txt".to_string());
        notice.row = Some(3);
        notice.field = Some("stop_lat".to_string());
        notice.insert_context_field("actual_value", 91.0);
        container.push(notice);

        let report = SarifReport::from_notices(&container);

        assert_eq!(report.runs[0].results.len(), 1);
        let result = &report.runs[0].results[0];
        assert_eq!(result.rule_id, "stop_lat_out_of_range");
        assert!(matches!(result.level, SarifLevel::Error));
        assert_eq!(result.locations.len(), 1);
        assert_eq!(
            result.locations[0].physical_location.artifact_location.uri,
            "stops.txt"
        );
        assert_eq!(
            result.locations[0]
                .physical_location
                .region
                .as_ref()
                .unwrap()
                .start_line,
            Some(3)
        );
    }

    #[test]
    fn test_code_to_name() {
        assert_eq!(code_to_name("stop_lat_out_of_range"), "StopLatOutOfRange");
        assert_eq!(code_to_name("missing_required_file"), "MissingRequiredFile");
        assert_eq!(code_to_name("i_o_error"), "IOError");
    }

    #[test]
    fn test_severity_conversion() {
        assert!(matches!(
            SarifLevel::from(NoticeSeverity::Error),
            SarifLevel::Error
        ));
        assert!(matches!(
            SarifLevel::from(NoticeSeverity::Warning),
            SarifLevel::Warning
        ));
        assert!(matches!(
            SarifLevel::from(NoticeSeverity::Info),
            SarifLevel::Note
        ));
    }
}
