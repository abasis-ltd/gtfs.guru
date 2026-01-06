//! Performance timing infrastructure for GTFS validator
//!
//! Provides detailed timing breakdowns for loading, parsing, and validation phases.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Records timing information for a named operation
#[derive(Debug, Clone)]
pub struct TimingRecord {
    pub name: String,
    pub duration: Duration,
    pub category: TimingCategory,
}

/// Categories of timed operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimingCategory {
    Loading,
    Parsing,
    Indexing,
    Validation,
}

impl TimingCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimingCategory::Loading => "loading",
            TimingCategory::Parsing => "parsing",
            TimingCategory::Indexing => "indexing",
            TimingCategory::Validation => "validation",
        }
    }
}

/// Collector for timing records
#[derive(Debug, Default)]
pub struct TimingCollector {
    records: Mutex<Vec<TimingRecord>>,
}

impl TimingCollector {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }

    /// Record a timed operation
    pub fn record(&self, name: impl Into<String>, duration: Duration, category: TimingCategory) {
        if let Ok(mut records) = self.records.lock() {
            records.push(TimingRecord {
                name: name.into(),
                duration,
                category,
            });
        }
    }

    /// Time a closure and record the result
    pub fn time<F, R>(&self, name: impl Into<String>, category: TimingCategory, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let name = name.into();
        let start = Instant::now();
        let result = f();
        self.record(name, start.elapsed(), category);
        result
    }

    /// Get all records
    pub fn get_records(&self) -> Vec<TimingRecord> {
        self.records.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Get records by category
    pub fn get_by_category(&self, category: TimingCategory) -> Vec<TimingRecord> {
        self.get_records()
            .into_iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Get total duration for a category
    pub fn total_for_category(&self, category: TimingCategory) -> Duration {
        self.get_by_category(category)
            .iter()
            .map(|r| r.duration)
            .sum()
    }

    /// Generate a summary report
    pub fn summary(&self) -> TimingSummary {
        let records = self.get_records();
        let mut by_category: HashMap<TimingCategory, Vec<TimingRecord>> = HashMap::new();

        for record in records {
            by_category.entry(record.category).or_default().push(record);
        }

        TimingSummary { by_category }
    }
}

/// Summary of timing records
#[derive(Debug)]
pub struct TimingSummary {
    pub by_category: HashMap<TimingCategory, Vec<TimingRecord>>,
}

impl TimingSummary {
    /// Format as a human-readable report
    pub fn format_report(&self) -> String {
        let mut output = String::new();
        output.push_str("\n=== Performance Timing Report ===\n\n");

        // Category order
        let categories = [
            TimingCategory::Loading,
            TimingCategory::Parsing,
            TimingCategory::Indexing,
            TimingCategory::Validation,
        ];

        for category in categories {
            if let Some(records) = self.by_category.get(&category) {
                let total: Duration = records.iter().map(|r| r.duration).sum();
                output.push_str(&format!(
                    "## {} (total: {:.3}s)\n",
                    category.as_str().to_uppercase(),
                    total.as_secs_f64()
                ));

                // Sort by duration descending
                let mut sorted: Vec<_> = records.iter().collect();
                sorted.sort_by(|a, b| b.duration.cmp(&a.duration));

                // Show top 10 or all if less
                for record in sorted.iter().take(10) {
                    output.push_str(&format!(
                        "  {:40} {:>10.3}s\n",
                        record.name,
                        record.duration.as_secs_f64()
                    ));
                }

                if sorted.len() > 10 {
                    output.push_str(&format!("  ... and {} more\n", sorted.len() - 10));
                }
                output.push('\n');
            }
        }

        // Grand total
        let grand_total: Duration = self
            .by_category
            .values()
            .flat_map(|v| v.iter())
            .map(|r| r.duration)
            .sum();
        output.push_str(&format!("TOTAL: {:.3}s\n", grand_total.as_secs_f64()));

        output
    }

    /// Format as JSON
    pub fn to_json(&self) -> serde_json::Value {
        let mut result = serde_json::Map::new();

        for (category, records) in &self.by_category {
            let items: Vec<serde_json::Value> = records
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "name": r.name,
                        "duration_ms": r.duration.as_millis() as u64,
                        "duration_s": r.duration.as_secs_f64(),
                    })
                })
                .collect();

            let total: Duration = records.iter().map(|r| r.duration).sum();
            result.insert(
                category.as_str().to_string(),
                serde_json::json!({
                    "total_ms": total.as_millis() as u64,
                    "total_s": total.as_secs_f64(),
                    "items": items,
                }),
            );
        }

        serde_json::Value::Object(result)
    }
}

/// RAII guard for timing a scope
pub struct TimingGuard<'a> {
    collector: &'a TimingCollector,
    name: String,
    category: TimingCategory,
    start: Instant,
}

impl<'a> TimingGuard<'a> {
    pub fn new(
        collector: &'a TimingCollector,
        name: impl Into<String>,
        category: TimingCategory,
    ) -> Self {
        Self {
            collector,
            name: name.into(),
            category,
            start: Instant::now(),
        }
    }
}

impl Drop for TimingGuard<'_> {
    fn drop(&mut self) {
        self.collector.record(
            std::mem::take(&mut self.name),
            self.start.elapsed(),
            self.category,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_timing_collector() {
        let collector = TimingCollector::new();

        collector.time("test_op", TimingCategory::Loading, || {
            thread::sleep(Duration::from_millis(10));
        });

        let records = collector.get_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "test_op");
        assert!(records[0].duration >= Duration::from_millis(10));
    }

    #[test]
    fn test_timing_guard() {
        let collector = TimingCollector::new();

        {
            let _guard = TimingGuard::new(&collector, "scoped_op", TimingCategory::Validation);
            thread::sleep(Duration::from_millis(5));
        }

        let records = collector.get_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "scoped_op");
    }
}
