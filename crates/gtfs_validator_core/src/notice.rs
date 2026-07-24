use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::csv_reader::CsvParseError;

pub const NOTICE_CODE_CSV_PARSE_ERROR: &str = "csv_parsing_failed";
pub const NOTICE_CODE_MISSING_FILE: &str = "missing_required_file";
pub const NOTICE_CODE_MISSING_RECOMMENDED_FILE: &str = "missing_recommended_file";
pub const NOTICE_CODE_EMPTY_TABLE: &str = "empty_file";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Exact per-(code, severity) bookkeeping: `total` counts every pushed
/// notice, `stored` only the ones kept in memory (`stored <= total`).
#[derive(Debug, Default, Clone, Copy)]
struct GroupTally {
    total: usize,
    stored: usize,
}

#[derive(Debug, Default)]
pub struct NoticeContainer {
    notices: Vec<ValidationNotice>,
    /// Max notices stored per (code, severity) group; `None` = unlimited.
    /// Totals stay exact either way — only the stored samples are capped.
    group_limit: Option<usize>,
    /// Exact totals per severity (Error/Warning/Info), including dropped.
    severity_totals: [usize; 3],
    /// Exact totals per code, indexed by severity ordinal.
    group_tallies: HashMap<String, [GroupTally; 3]>,
}

fn severity_index(severity: NoticeSeverity) -> usize {
    match severity {
        NoticeSeverity::Error => 0,
        NoticeSeverity::Warning => 1,
        NoticeSeverity::Info => 2,
    }
}

impl NoticeContainer {
    /// Creates a container honoring the thread-local notice group limit
    /// (see `set_notice_group_limit`).
    pub fn new() -> Self {
        Self::with_group_limit(crate::validation_context::notice_group_limit())
    }

    pub fn with_group_limit(group_limit: Option<usize>) -> Self {
        Self {
            notices: Vec::new(),
            group_limit,
            severity_totals: [0; 3],
            group_tallies: HashMap::new(),
        }
    }

    pub fn push(&mut self, notice: ValidationNotice) {
        let sev = severity_index(notice.severity);
        self.severity_totals[sev] += 1;
        if !self.group_tallies.contains_key(notice.code.as_str()) {
            self.group_tallies
                .insert(notice.code.clone(), <[GroupTally; 3]>::default());
        }
        let tally = &mut self
            .group_tallies
            .get_mut(notice.code.as_str())
            .expect("tally inserted above")[sev];
        tally.total += 1;
        if let Some(limit) = self.group_limit {
            if tally.stored >= limit {
                return;
            }
        }
        tally.stored += 1;
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

    /// Number of notices actually stored (i.e. after any group cap).
    pub fn len(&self) -> usize {
        self.notices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    /// Exact number of notices pushed, including ones dropped by the cap.
    pub fn total_len(&self) -> usize {
        self.severity_totals.iter().sum()
    }

    /// Exact (errors, warnings, infos) totals, including dropped notices.
    pub fn severity_counts(&self) -> (usize, usize, usize) {
        (
            self.severity_totals[0],
            self.severity_totals[1],
            self.severity_totals[2],
        )
    }

    /// Exact total for one (code, severity) group, including dropped notices.
    pub fn group_total(&self, code: &str, severity: NoticeSeverity) -> usize {
        self.group_tallies
            .get(code)
            .map(|tallies| tallies[severity_index(severity)].total)
            .unwrap_or(0)
    }

    /// Number of notices dropped by the group cap.
    pub fn dropped_len(&self) -> usize {
        self.total_len() - self.notices.len()
    }

    pub fn is_truncated(&self) -> bool {
        self.dropped_len() > 0
    }

    pub fn merge(&mut self, other: NoticeContainer) {
        let NoticeContainer {
            notices,
            group_limit: _,
            severity_totals,
            group_tallies,
        } = other;

        if self.group_limit.is_none() {
            // No cap on the destination: keep everything `other` stored and
            // fold its exact counters in wholesale.
            for (sev, count) in severity_totals.iter().enumerate() {
                self.severity_totals[sev] += count;
            }
            for (code, tallies) in group_tallies {
                let entry = self.group_tallies.entry(code).or_default();
                for (sev, tally) in tallies.iter().enumerate() {
                    entry[sev].total += tally.total;
                    entry[sev].stored += tally.stored;
                }
            }
            self.notices.extend(notices);
            return;
        }

        // Capped destination: replay stored notices through `push` (which
        // re-applies the cap), then account for notices `other` already
        // dropped so totals stay exact.
        for notice in notices {
            self.push(notice);
        }
        for (code, tallies) in group_tallies {
            for (sev, tally) in tallies.iter().enumerate() {
                let dropped = tally.total - tally.stored;
                if dropped > 0 {
                    self.severity_totals[sev] += dropped;
                    self.group_tallies.entry(code.clone()).or_default()[sev].total += dropped;
                }
            }
        }
    }

    pub fn into_vec(self) -> Vec<ValidationNotice> {
        self.notices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(code: &str, severity: NoticeSeverity) -> ValidationNotice {
        ValidationNotice::new(code, severity, "test message")
    }

    #[test]
    fn uncapped_container_stores_everything() {
        let mut container = NoticeContainer::with_group_limit(None);
        for _ in 0..5 {
            container.push(notice("code_a", NoticeSeverity::Error));
        }
        assert_eq!(container.len(), 5);
        assert_eq!(container.total_len(), 5);
        assert!(!container.is_truncated());
        assert_eq!(container.group_total("code_a", NoticeSeverity::Error), 5);
    }

    #[test]
    fn capped_container_drops_but_keeps_exact_counts() {
        let mut container = NoticeContainer::with_group_limit(Some(3));
        for _ in 0..10 {
            container.push(notice("code_a", NoticeSeverity::Error));
        }
        for _ in 0..2 {
            container.push(notice("code_b", NoticeSeverity::Warning));
        }
        assert_eq!(container.len(), 5); // 3 stored + 2 stored
        assert_eq!(container.total_len(), 12);
        assert_eq!(container.dropped_len(), 7);
        assert!(container.is_truncated());
        assert_eq!(container.severity_counts(), (10, 2, 0));
        assert_eq!(container.group_total("code_a", NoticeSeverity::Error), 10);
        assert_eq!(container.group_total("code_b", NoticeSeverity::Warning), 2);
    }

    #[test]
    fn cap_is_per_code_and_severity() {
        let mut container = NoticeContainer::with_group_limit(Some(2));
        for _ in 0..4 {
            container.push(notice("code_a", NoticeSeverity::Error));
            container.push(notice("code_a", NoticeSeverity::Warning));
        }
        // Same code, different severities: each group capped independently.
        assert_eq!(container.len(), 4);
        assert_eq!(container.group_total("code_a", NoticeSeverity::Error), 4);
        assert_eq!(container.group_total("code_a", NoticeSeverity::Warning), 4);
    }

    #[test]
    fn merge_into_capped_container_enforces_cap_and_keeps_totals() {
        let mut a = NoticeContainer::with_group_limit(Some(3));
        let mut b = NoticeContainer::with_group_limit(Some(3));
        for _ in 0..5 {
            a.push(notice("code_a", NoticeSeverity::Error));
            b.push(notice("code_a", NoticeSeverity::Error));
        }
        a.merge(b);
        assert_eq!(a.len(), 3); // cap still enforced after merge
        assert_eq!(a.total_len(), 10); // exact total across both containers
        assert_eq!(a.group_total("code_a", NoticeSeverity::Error), 10);
        assert_eq!(a.severity_counts(), (10, 0, 0));
    }

    #[test]
    fn merge_into_uncapped_container_preserves_counts_from_capped_source() {
        let mut capped = NoticeContainer::with_group_limit(Some(2));
        for _ in 0..6 {
            capped.push(notice("code_a", NoticeSeverity::Info));
        }
        let mut dest = NoticeContainer::with_group_limit(None);
        dest.push(notice("code_b", NoticeSeverity::Error));
        dest.merge(capped);
        assert_eq!(dest.len(), 3); // 1 + the 2 the source stored
        assert_eq!(dest.total_len(), 7);
        assert_eq!(dest.group_total("code_a", NoticeSeverity::Info), 6);
        assert_eq!(dest.severity_counts(), (1, 0, 6));
    }

    #[test]
    fn new_respects_thread_local_limit() {
        let _guard = crate::validation_context::set_notice_group_limit(Some(1));
        let mut container = NoticeContainer::new();
        container.push(notice("code_a", NoticeSeverity::Error));
        container.push(notice("code_a", NoticeSeverity::Error));
        assert_eq!(container.len(), 1);
        assert_eq!(container.total_len(), 2);
    }
}
