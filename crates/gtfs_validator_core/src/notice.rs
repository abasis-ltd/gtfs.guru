use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::csv_reader::CsvParseError;

pub const NOTICE_CODE_CSV_PARSE_ERROR: &str = "csv_parsing_failed";
pub const NOTICE_CODE_MISSING_FILE: &str = "missing_required_file";
pub const NOTICE_CODE_MISSING_RECOMMENDED_FILE: &str = "missing_recommended_file";
pub const NOTICE_CODE_EMPTY_TABLE: &str = "empty_file";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoticeSeverity {
    Error,
    Warning,
    Info,
}

/// Safety level for auto-fixes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixSafety {
    /// Safe to apply automatically (formatting, whitespace)
    Safe,
    /// Requires user confirmation (deduplication)
    RequiresConfirmation,
    /// May change semantics (referential fixes)
    Unsafe,
}

impl FixSafety {
    /// Ordering rank: `Safe` < `RequiresConfirmation` < `Unsafe`.
    fn rank(self) -> u8 {
        match self {
            FixSafety::Safe => 0,
            FixSafety::RequiresConfirmation => 1,
            FixSafety::Unsafe => 2,
        }
    }

    /// Whether a fix at this level may run when the caller allows up to `max`.
    pub fn allowed_by(self, max: FixSafety) -> bool {
        self.rank() <= max.rank()
    }

    /// Short label used in CLI output and reports.
    pub fn label(self) -> &'static str {
        match self {
            FixSafety::Safe => "SAFE",
            FixSafety::RequiresConfirmation => "CONFIRM",
            FixSafety::Unsafe => "UNSAFE",
        }
    }
}

/// The actual fix operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FixOperation {
    /// Replace a field value
    ReplaceField {
        file: String,
        row: u64,
        field: String,
        original: String,
        replacement: String,
    },
}

/// A suggested fix for a validation issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    /// Description of what the fix does
    pub description: String,
    /// Safety level
    pub safety: FixSafety,
    /// The fix operation
    pub operation: FixOperation,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
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
            fix: None,
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
            "charIndex".into(),
            "columnIndex".into(),
            "filename".into(),
            "lineIndex".into(),
            "message".into(),
            "parsedContent".into(),
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
        self.field_order = vec!["filename".into(), "csvRowNumber".into(), "fieldName".into()];
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

    /// Attach a suggested fix to this notice
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

#[derive(Debug)]
pub struct NoticeContainer {
    notices: Vec<ValidationNotice>,
    sample_limit: Option<usize>,
    stats: Option<Box<NoticeStats>>,
}

#[derive(Debug, Default)]
struct NoticeStats {
    total: usize,
    by_severity: [usize; 3],
    by_code: HashMap<String, [usize; 3]>,
    retained_by_code: HashMap<String, [usize; 3]>,
}

impl Default for NoticeContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl NoticeContainer {
    pub fn new() -> Self {
        Self {
            notices: Vec::new(),
            sample_limit: default_sample_limit(),
            stats: None,
        }
    }

    #[cfg(test)]
    fn with_sample_limit(sample_limit: usize) -> Self {
        Self {
            notices: Vec::new(),
            sample_limit: Some(sample_limit),
            stats: None,
        }
    }

    pub fn push(&mut self, notice: ValidationNotice) {
        let Some(sample_limit) = self.sample_limit else {
            self.notices.push(notice);
            return;
        };

        let severity = severity_index(notice.severity);
        let stats = self
            .stats
            .get_or_insert_with(|| Box::new(NoticeStats::default()));
        stats.total += 1;
        stats.by_severity[severity] += 1;
        stats.by_code.entry(notice.code.clone()).or_default()[severity] += 1;
        let retained = &mut stats
            .retained_by_code
            .entry(notice.code.clone())
            .or_default()[severity];
        if *retained >= sample_limit {
            return;
        }
        *retained += 1;
        self.notices.push(notice);
    }

    pub fn push_csv_error(&mut self, error: &CsvParseError) {
        self.push(ValidationNotice::from_csv_error(error));
    }

    pub fn push_missing_file(&mut self, file: impl Into<String>) {
        self.push(ValidationNotice::missing_file(file));
    }

    pub fn push_empty_table(&mut self, file: impl Into<String>) {
        self.push(ValidationNotice::empty_table(file));
    }

    pub fn push_missing_recommended_file(&mut self, file: impl Into<String>) {
        self.push(ValidationNotice::missing_recommended_file(file));
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidationNotice> {
        self.notices.iter()
    }

    pub fn len(&self) -> usize {
        self.stats
            .as_ref()
            .map(|stats| stats.total)
            .unwrap_or(self.notices.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn merge(&mut self, other: NoticeContainer) {
        let Some(sample_limit) = self.sample_limit else {
            self.notices.extend(other.notices);
            return;
        };

        let other_stats = other.stats;
        let stats = self
            .stats
            .get_or_insert_with(|| Box::new(NoticeStats::default()));
        if let Some(other_stats) = other_stats {
            stats.total += other_stats.total;
            for (target, value) in stats.by_severity.iter_mut().zip(other_stats.by_severity) {
                *target += value;
            }
            for (code, counts) in other_stats.by_code {
                let target = stats.by_code.entry(code).or_default();
                for (target, value) in target.iter_mut().zip(counts) {
                    *target += value;
                }
            }
        } else {
            for notice in &other.notices {
                let severity = severity_index(notice.severity);
                stats.total += 1;
                stats.by_severity[severity] += 1;
                stats.by_code.entry(notice.code.clone()).or_default()[severity] += 1;
            }
        }

        for notice in other.notices {
            let severity = severity_index(notice.severity);
            let retained = &mut stats
                .retained_by_code
                .entry(notice.code.clone())
                .or_default()[severity];
            if *retained < sample_limit {
                *retained += 1;
                self.notices.push(notice);
            }
        }
    }

    pub fn count_by_severity(&self, severity: NoticeSeverity) -> usize {
        self.stats
            .as_ref()
            .map(|stats| stats.by_severity[severity_index(severity)])
            .unwrap_or_else(|| {
                self.notices
                    .iter()
                    .filter(|notice| notice.severity == severity)
                    .count()
            })
    }

    pub fn count_for(&self, code: &str, severity: NoticeSeverity) -> usize {
        self.stats
            .as_ref()
            .and_then(|stats| stats.by_code.get(code))
            .map(|counts| counts[severity_index(severity)])
            .unwrap_or_else(|| {
                self.notices
                    .iter()
                    .filter(|notice| notice.code == code && notice.severity == severity)
                    .count()
            })
    }

    pub fn into_vec(self) -> Vec<ValidationNotice> {
        self.notices
    }
}

const fn severity_index(severity: NoticeSeverity) -> usize {
    match severity {
        NoticeSeverity::Error => 0,
        NoticeSeverity::Warning => 1,
        NoticeSeverity::Info => 2,
    }
}

const fn default_sample_limit() -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(1_000)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(code: &str, severity: NoticeSeverity) -> ValidationNotice {
        ValidationNotice::new(code, severity, "test notice")
    }

    #[test]
    fn bounded_container_keeps_exact_counts() {
        let mut notices = NoticeContainer::with_sample_limit(2);
        for _ in 0..5 {
            notices.push(notice("repeated", NoticeSeverity::Error));
        }
        notices.push(notice("repeated", NoticeSeverity::Warning));

        assert_eq!(notices.len(), 6);
        assert_eq!(notices.iter().count(), 3);
        assert_eq!(notices.count_by_severity(NoticeSeverity::Error), 5);
        assert_eq!(notices.count_by_severity(NoticeSeverity::Warning), 1);
        assert_eq!(notices.count_for("repeated", NoticeSeverity::Error), 5);
    }

    #[test]
    fn bounded_merge_preserves_counts_and_sample_limit() {
        let mut left = NoticeContainer::with_sample_limit(2);
        let mut right = NoticeContainer::with_sample_limit(2);
        for _ in 0..2 {
            left.push(notice("repeated", NoticeSeverity::Error));
        }
        for _ in 0..3 {
            right.push(notice("repeated", NoticeSeverity::Error));
        }

        left.merge(right);

        assert_eq!(left.len(), 5);
        assert_eq!(left.iter().count(), 2);
        assert_eq!(left.count_for("repeated", NoticeSeverity::Error), 5);
    }
}
