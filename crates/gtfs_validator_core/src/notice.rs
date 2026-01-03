use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::csv_reader::CsvParseError;

pub const NOTICE_CODE_CSV_PARSE_ERROR: &str = "csv_parsing_failed";
pub const NOTICE_CODE_MISSING_FILE: &str = "missing_required_file";
pub const NOTICE_CODE_MISSING_RECOMMENDED_FILE: &str = "missing_recommended_file";
pub const NOTICE_CODE_EMPTY_TABLE: &str = "empty_file";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationNotice {
    pub code: String,
    pub severity: NoticeSeverity,
    pub message: String,
    pub file: Option<String>,
    pub row: Option<u64>,
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_order: Vec<String>,
}

impl ValidationNotice {
    pub fn new(
        code: impl Into<String>,
        severity: NoticeSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            file: None,
            row: None,
            field: None,
            context: BTreeMap::new(),
            field_order: Vec::new(),
        }
    }

    pub fn from_csv_error(error: &CsvParseError) -> Self {
        let mut notice = ValidationNotice::new(
            NOTICE_CODE_CSV_PARSE_ERROR,
            NoticeSeverity::Error,
            error.message.clone(),
        );
        notice.insert_context_field("charIndex", error.char_index.unwrap_or_default());
        notice.insert_context_field("columnIndex", error.column_index.unwrap_or_default());
        notice.insert_context_field("filename", error.file.clone());
        notice.insert_context_field("lineIndex", error.line_index.unwrap_or_default());
        notice.insert_context_field("message", error.message.clone());
        notice.insert_context_field(
            "parsedContent",
            error.parsed_content.clone().unwrap_or_default(),
        );
        notice.field_order = vec![
            "charIndex".to_string(),
            "columnIndex".to_string(),
            "filename".to_string(),
            "lineIndex".to_string(),
            "message".to_string(),
            "parsedContent".to_string(),
        ];
        return notice;
    }

    pub fn missing_file(file: impl Into<String>) -> Self {
        let file = file.into();
        let mut notice = ValidationNotice::new(
            NOTICE_CODE_MISSING_FILE,
            NoticeSeverity::Error,
            "missing required GTFS file",
        );
        notice.file = Some(file.clone());
        notice.insert_context_field("filename", file);
        return notice;
    }

    pub fn empty_table(file: impl Into<String>) -> Self {
        let file = file.into();
        let mut notice = ValidationNotice::new(
            NOTICE_CODE_EMPTY_TABLE,
            NoticeSeverity::Error,
            "GTFS table has no rows",
        );
        notice.file = Some(file.clone());
        notice.insert_context_field("filename", file);
        return notice;
    }

    pub fn missing_recommended_file(file: impl Into<String>) -> Self {
        let file = file.into();
        let mut notice = ValidationNotice::new(
            NOTICE_CODE_MISSING_RECOMMENDED_FILE,
            NoticeSeverity::Warning,
            "missing recommended GTFS file",
        );
        notice.file = Some(file.clone());
        notice.insert_context_field("filename", file);
        return notice;
    }

    pub fn insert_context_field<V: Serialize>(&mut self, name: impl Into<String>, value: V) {
        let key = name.into();
        let serialized = serde_json::to_value(value).unwrap_or_else(|_| Value::Null);
        if !self.field_order.iter().any(|item| item == &key) {
            self.field_order.push(key.clone());
        }
        self.context.insert(key, serialized);
    }

    pub fn with_context_field<V: Serialize>(mut self, name: impl Into<String>, value: V) -> Self {
        self.insert_context_field(name, value);
        self
    }

    pub fn set_location(&mut self, file: impl Into<String>, field: impl Into<String>, row: u64) {
        self.file = Some(file.into());
        self.field = Some(field.into());
        self.row = Some(row);
        self.field_order = vec![
            "filename".to_string(),
            "csvRowNumber".to_string(),
            "fieldName".to_string(),
        ];
    }

    pub fn with_location(
        mut self,
        file: impl Into<String>,
        field: impl Into<String>,
        row: u64,
    ) -> Self {
        self.set_location(file, field, row);
        self
    }
}

#[derive(Debug, Default)]
pub struct NoticeContainer {
    notices: Vec<ValidationNotice>,
}

impl NoticeContainer {
    pub fn new() -> Self {
        Self {
            notices: Vec::new(),
        }
    }

    pub fn push(&mut self, notice: ValidationNotice) {
        self.notices.push(notice);
    }

    pub fn push_csv_error(&mut self, error: &CsvParseError) {
        self.notices.push(ValidationNotice::from_csv_error(error));
    }

    pub fn push_missing_file(&mut self, file: impl Into<String>) {
        self.notices.push(ValidationNotice::missing_file(file));
    }

    pub fn push_empty_table(&mut self, file: impl Into<String>) {
        self.notices.push(ValidationNotice::empty_table(file));
    }

    pub fn push_missing_recommended_file(&mut self, file: impl Into<String>) {
        self.notices
            .push(ValidationNotice::missing_recommended_file(file));
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidationNotice> {
        self.notices.iter()
    }

    pub fn len(&self) -> usize {
        self.notices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    pub fn merge(&mut self, other: NoticeContainer) {
        self.notices.extend(other.notices);
    }
}
