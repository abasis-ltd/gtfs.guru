#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::manual_range_patterns)]
#![allow(clippy::type_complexity)]
#![allow(clippy::vec_init_then_push)]
#![allow(clippy::new_without_default)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::manual_pattern_char_comparison)]
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{Local, NaiveDate, SecondsFormat};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use serde_json::{Number, Value};

use gtfs_guru_core::feed::{
    AGENCY_FILE, AREAS_FILE, ATTRIBUTIONS_FILE, BOOKING_RULES_FILE, CALENDAR_DATES_FILE,
    CALENDAR_FILE, FARE_ATTRIBUTES_FILE, FARE_LEG_JOIN_RULES_FILE, FARE_LEG_RULES_FILE,
    FARE_MEDIA_FILE, FARE_PRODUCTS_FILE, FARE_RULES_FILE, FARE_TRANSFER_RULES_FILE, FEED_INFO_FILE,
    FREQUENCIES_FILE, LEVELS_FILE, LOCATIONS_GEOJSON_FILE, LOCATION_GROUPS_FILE,
    LOCATION_GROUP_STOPS_FILE, NETWORKS_FILE, PATHWAYS_FILE, RIDER_CATEGORIES_FILE, ROUTES_FILE,
    ROUTE_NETWORKS_FILE, SHAPES_FILE, STOPS_FILE, STOP_AREAS_FILE, STOP_TIMES_FILE,
    TIMEFRAMES_FILE, TRANSFERS_FILE, TRANSLATIONS_FILE, TRIPS_FILE,
};
use gtfs_guru_core::{CsvTable, GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_guru_model::{ExceptionType, StringId};

mod html;
pub use html::{generate_html_report_string, write_html_report, HtmlReportContext};

mod sarif;
pub use sarif::SarifReport;

const DEFAULT_COUNTRY_CODE: &str = "ZZ";
const DEFAULT_HTML_REPORT_NAME: &str = "report.html";
const DEFAULT_SYSTEM_ERRORS_REPORT_NAME: &str = "system_errors.json";
const DEFAULT_VALIDATION_REPORT_NAME: &str = "report.json";
const MAX_EXPORTS_PER_NOTICE_TYPE_AND_SEVERITY: usize = 1000;

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReportSummary>,
    pub notices: Vec<NoticeReport>,
}

impl ValidationReport {
    pub fn from_container(container: &NoticeContainer) -> Self {
        Self {
            summary: None,
            notices: build_notice_reports(container),
        }
    }

    pub fn from_container_with_summary(
        container: &NoticeContainer,
        summary: ReportSummary,
    ) -> Self {
        Self {
            summary: Some(summary),
            notices: build_notice_reports(container),
        }
    }

    pub fn with_summary(mut self, summary: ReportSummary) -> Self {
        self.summary = Some(summary);
        self
    }

    pub fn empty() -> Self {
        Self {
            summary: None,
            notices: Vec::new(),
        }
    }

    pub fn write_json<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        self.write_json_with_format(path, true)
    }

    pub fn write_json_with_format<P: AsRef<Path>>(
        &self,
        path: P,
        pretty: bool,
    ) -> anyhow::Result<()> {
        let path = path.as_ref();
        let json = if pretty {
            serde_json::to_string_pretty(self)
        } else {
            serde_json::to_string(self)
        }
        .context("serialize report")?;
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, format!("{}\n", json))
            .with_context(|| format!("write report to {}", temp_path.display()))?;
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "replace report at {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportSeverity {
    Info,
    Warning,
    Error,
}

impl From<NoticeSeverity> for ReportSeverity {
    fn from(value: NoticeSeverity) -> Self {
        match value {
            NoticeSeverity::Info => ReportSeverity::Info,
            NoticeSeverity::Warning => ReportSeverity::Warning,
            NoticeSeverity::Error => ReportSeverity::Error,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NoticeReport {
    pub code: String,
    pub severity: ReportSeverity,
    #[serde(rename = "totalNotices")]
    pub total_notices: usize,
    #[serde(rename = "sampleNotices")]
    pub sample_notices: Vec<NoticeContext>,
}

#[derive(Debug, Clone)]
pub struct NoticeContext {
    fields: Vec<(String, Value)>,
}

impl Serialize for NoticeContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (key, value) in &self.fields {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtfs_input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_errors_report_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_report_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_report_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_for_validation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_info: Option<ReportFeedInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agencies: Option<Vec<ReportAgency>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_time_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_records: Option<Vec<MemoryUsageRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<ReportCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtfs_features: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUsageRecord {
    pub key: String,
    pub total_memory: u64,
    pub free_memory: u64,
    pub max_memory: u64,
    pub diff_memory: Option<i64>,
}

impl ReportSummary {
    pub fn from_context(context: ReportSummaryContext<'_>) -> Self {
        let now = Local::now();
        let validated_at = context
            .validated_at
            .or_else(|| Some(now.to_rfc3339_opts(SecondsFormat::Secs, true)));
        let date_for_validation = context
            .date_for_validation
            .or_else(|| Some(now.date_naive().format("%Y-%m-%d").to_string()));
        let validator_version = context
            .validator_version
            .or_else(|| Some(env!("CARGO_PKG_VERSION").to_string()));
        let gtfs_input = context
            .gtfs_input_uri
            .or_else(|| context.gtfs_input.and_then(path_to_file_url));
        let output_directory = context
            .output_directory
            .map(|path| path.to_string_lossy().to_string());
        let feed_info = context.feed.map(build_feed_info);
        let agencies = context.feed.map(build_agencies);
        let files = context.feed.map(build_files);
        let counts = context.feed.map(build_counts);
        let gtfs_features = context.feed.map(build_gtfs_features);
        let memory_usage_records = context.memory_usage_records.or_else(|| Some(Vec::new()));

        ReportSummary {
            validator_version,
            validated_at,
            gtfs_input,
            threads: Some(context.threads),
            output_directory,
            system_errors_report_name: Some(
                context
                    .system_errors_report_name
                    .unwrap_or_else(|| DEFAULT_SYSTEM_ERRORS_REPORT_NAME.to_string()),
            ),
            validation_report_name: Some(
                context
                    .validation_report_name
                    .unwrap_or_else(|| DEFAULT_VALIDATION_REPORT_NAME.to_string()),
            ),
            html_report_name: Some(
                context
                    .html_report_name
                    .unwrap_or_else(|| DEFAULT_HTML_REPORT_NAME.to_string()),
            ),
            country_code: Some(
                context
                    .country_code
                    .unwrap_or_else(|| DEFAULT_COUNTRY_CODE.to_string()),
            ),
            date_for_validation,
            feed_info,
            agencies,
            files,
            validation_time_seconds: context.validation_time_seconds,
            memory_usage_records,
            counts,
            gtfs_features,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportFeedInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_service_window_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_service_window_end: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportAgency {
    pub name: String,
    pub url: String,
    pub phone: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct ReportCounts {
    #[serde(rename = "Shapes")]
    pub shapes: usize,
    #[serde(rename = "Stops")]
    pub stops: usize,
    #[serde(rename = "Routes")]
    pub routes: usize,
    #[serde(rename = "Trips")]
    pub trips: usize,
    #[serde(rename = "Agencies")]
    pub agencies: usize,
    #[serde(rename = "Blocks")]
    pub blocks: usize,
}

pub struct ReportSummaryContext<'a> {
    pub feed: Option<&'a GtfsFeed>,
    pub gtfs_input: Option<&'a Path>,
    pub gtfs_input_uri: Option<String>,
    pub output_directory: Option<&'a Path>,
    pub validation_report_name: Option<String>,
    pub html_report_name: Option<String>,
    pub system_errors_report_name: Option<String>,
    pub validation_time_seconds: Option<f64>,
    pub memory_usage_records: Option<Vec<MemoryUsageRecord>>,
    pub validator_version: Option<String>,
    pub validated_at: Option<String>,
    pub country_code: Option<String>,
    pub date_for_validation: Option<String>,
    pub threads: u32,
}

impl<'a> ReportSummaryContext<'a> {
    pub fn new() -> Self {
        Self {
            feed: None,
            gtfs_input: None,
            gtfs_input_uri: None,
            output_directory: None,
            validation_report_name: None,
            html_report_name: None,
            system_errors_report_name: None,
            validation_time_seconds: None,
            memory_usage_records: None,
            validator_version: None,
            validated_at: None,
            country_code: None,
            date_for_validation: None,
            threads: 1,
        }
    }

    pub fn with_feed(mut self, feed: &'a GtfsFeed) -> Self {
        self.feed = Some(feed);
        self
    }

    pub fn with_gtfs_input(mut self, path: &'a Path) -> Self {
        self.gtfs_input = Some(path);
        self
    }

    pub fn with_gtfs_input_uri(mut self, uri: impl Into<String>) -> Self {
        self.gtfs_input_uri = Some(uri.into());
        self
    }

    pub fn with_output_directory(mut self, path: &'a Path) -> Self {
        self.output_directory = Some(path);
        self
    }

    pub fn with_validation_report_name(mut self, name: impl Into<String>) -> Self {
        self.validation_report_name = Some(name.into());
        self
    }

    pub fn with_html_report_name(mut self, name: impl Into<String>) -> Self {
        self.html_report_name = Some(name.into());
        self
    }

    pub fn with_system_errors_report_name(mut self, name: impl Into<String>) -> Self {
        self.system_errors_report_name = Some(name.into());
        self
    }

    pub fn with_validation_time_seconds(mut self, seconds: f64) -> Self {
        self.validation_time_seconds = Some(seconds);
        self
    }

    pub fn with_memory_usage_records(mut self, records: Vec<MemoryUsageRecord>) -> Self {
        self.memory_usage_records = Some(records);
        self
    }

    pub fn with_validator_version(mut self, version: impl Into<String>) -> Self {
        self.validator_version = Some(version.into());
        self
    }

    pub fn with_validated_at(mut self, validated_at: impl Into<String>) -> Self {
        self.validated_at = Some(validated_at.into());
        self
    }

    pub fn with_country_code(mut self, code: impl Into<String>) -> Self {
        self.country_code = Some(code.into());
        self
    }

    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = threads;
        self
    }

    pub fn with_date_for_validation(mut self, date: impl Into<String>) -> Self {
        self.date_for_validation = Some(date.into());
        self
    }
}

fn build_notice_reports(container: &NoticeContainer) -> Vec<NoticeReport> {
    let mut grouped: BTreeMap<(String, u8), Vec<&ValidationNotice>> = BTreeMap::new();
    for notice in container.iter() {
        let key = (notice.code.clone(), severity_ordinal(notice.severity));
        grouped.entry(key).or_default().push(notice);
    }

    let mut reports = Vec::new();
    for notices in grouped.values() {
        let first = notices[0];
        let sample_notices = notices
            .iter()
            .take(MAX_EXPORTS_PER_NOTICE_TYPE_AND_SEVERITY)
            .map(|notice| notice_context(notice))
            .collect();
        reports.push(NoticeReport {
            code: first.code.clone(),
            severity: first.severity.into(),
            total_notices: notices.len(),
            sample_notices,
        });
    }
    reports
}

fn notice_context(notice: &ValidationNotice) -> NoticeContext {
    let mut fields = Vec::new();

    if notice.field_order.is_empty() {
        let mut inserted = HashSet::new();
        if let Some(file) = &notice.file {
            fields.push(("filename".to_string(), Value::String(file.clone())));
            inserted.insert("filename");
        }
        if let Some(row) = notice.row {
            fields.push(("csvRowNumber".to_string(), Value::Number(Number::from(row))));
            inserted.insert("csvRowNumber");
        }
        if let Some(field) = &notice.field {
            fields.push(("fieldName".to_string(), Value::String(field.clone())));
            inserted.insert("fieldName");
        }
        for (key, value) in &notice.context {
            if inserted.contains(key.as_str()) {
                continue;
            }
            fields.push((key.clone(), value.clone()));
        }
        return NoticeContext { fields };
    }

    let mut remaining = notice.context.clone();
    let mut inserted = HashSet::new();
    let mut insert_by_key = |key: &str, fields: &mut Vec<(String, Value)>| {
        if inserted.contains(key) {
            return;
        }

        let mut inserted_value = None;
        match key {
            "filename" => {
                if let Some(file) = &notice.file {
                    inserted_value = Some(Value::String(file.clone()));
                }
            }
            "csvRowNumber" => {
                if let Some(row) = notice.row {
                    inserted_value = Some(Value::Number(Number::from(row)));
                }
            }
            "fieldName" => {
                if let Some(field) = &notice.field {
                    inserted_value = Some(Value::String(field.clone()));
                }
            }
            _ => {}
        }

        if inserted_value.is_none() {
            inserted_value = remaining.remove(key);
        } else {
            remaining.remove(key);
        }

        if let Some(value) = inserted_value {
            fields.push((key.to_string(), value));
            inserted.insert(key.to_string());
        }
    };

    for key in &notice.field_order {
        insert_by_key(key, &mut fields);
    }

    for key in ["filename", "csvRowNumber", "fieldName"] {
        insert_by_key(key, &mut fields);
    }

    for (key, value) in remaining {
        if !inserted.contains(&key) {
            inserted.insert(key.clone());
            fields.push((key, value));
        }
    }

    NoticeContext { fields }
}

fn severity_ordinal(severity: NoticeSeverity) -> u8 {
    match severity {
        NoticeSeverity::Info => 0,
        NoticeSeverity::Warning => 1,
        NoticeSeverity::Error => 2,
    }
}

fn build_feed_info(feed: &GtfsFeed) -> ReportFeedInfo {
    let info_table = feed.feed_info.as_ref();
    let info_row = info_table.and_then(|table| table.rows.first());
    let has_feed_info = info_table.is_some();
    let has_start_date = info_table
        .map(|table| has_header(table, "feed_start_date"))
        .unwrap_or(false);
    let has_end_date = info_table
        .map(|table| has_header(table, "feed_end_date"))
        .unwrap_or(false);
    let has_service_window = feed.calendar.is_some() || feed.calendar_dates.is_some();
    let (service_start, service_end) = compute_service_window(feed);
    let (service_start_str, service_end_str) = format_service_window(service_start, service_end);

    ReportFeedInfo {
        publisher_name: has_feed_info.then(|| {
            info_row
                .map(|row| row.feed_publisher_name.to_string())
                .unwrap_or_default()
        }),
        publisher_url: has_feed_info.then(|| {
            info_row
                .map(|row| feed.pool.resolve(row.feed_publisher_url))
                .unwrap_or_default()
        }),
        feed_language: has_feed_info.then(|| {
            info_row
                .map(|row| {
                    let language = feed.pool.resolve(row.feed_lang);
                    language_display_name(&language)
                })
                .unwrap_or_default()
        }),
        feed_start_date: has_start_date.then(|| {
            info_row
                .and_then(|row| row.feed_start_date)
                .map(format_gtfs_date)
                .unwrap_or_default()
        }),
        feed_end_date: has_end_date.then(|| {
            info_row
                .and_then(|row| row.feed_end_date)
                .map(format_gtfs_date)
                .unwrap_or_default()
        }),
        feed_email: has_feed_info.then(|| {
            info_row
                .and_then(|row| row.feed_contact_email.as_ref().map(|s| s.to_string()))
                .unwrap_or_default()
        }),
        feed_service_window_start: has_service_window.then(|| service_start_str),
        feed_service_window_end: has_service_window.then(|| service_end_str),
    }
}

fn build_agencies(feed: &GtfsFeed) -> Vec<ReportAgency> {
    feed.agency
        .rows
        .iter()
        .map(|agency| ReportAgency {
            name: agency.agency_name.to_string(),
            url: feed.pool.resolve(agency.agency_url),
            phone: agency.agency_phone.clone().unwrap_or_default().to_string(),
            email: agency.agency_email.clone().unwrap_or_default().to_string(),
        })
        .collect()
}

fn build_counts(feed: &GtfsFeed) -> ReportCounts {
    let shapes = feed
        .shapes
        .as_ref()
        .map(|table| unique_id_count(table.rows.iter(), |row| Some(row.shape_id)))
        .unwrap_or(0);
    let stops = unique_id_count(feed.stops.rows.iter(), |row| Some(row.stop_id));
    let routes = unique_id_count(feed.routes.rows.iter(), |row| Some(row.route_id));
    let trips = unique_id_count(feed.trips.rows.iter(), |row| Some(row.trip_id));
    let agencies = if feed.agency.rows.iter().any(|row| has_id_opt(row.agency_id)) {
        unique_id_count(feed.agency.rows.iter(), |row| row.agency_id)
    } else {
        feed.agency.rows.len()
    };
    let blocks = unique_id_count(feed.trips.rows.iter(), |row| row.block_id);

    ReportCounts {
        shapes,
        stops,
        routes,
        trips,
        agencies,
        blocks,
    }
}

fn unique_id_count<'a, T: 'a, F>(items: impl Iterator<Item = &'a T>, key: F) -> usize
where
    F: Fn(&'a T) -> Option<StringId>,
{
    let mut unique = HashSet::new();
    for item in items {
        if let Some(value) = key(item) {
            if has_id(value) {
                unique.insert(value);
            }
        }
    }
    unique.len()
}

fn build_files(feed: &GtfsFeed) -> Vec<String> {
    let mut files = BTreeMap::new();
    files.insert(AGENCY_FILE, ());
    if !feed.stops.headers.is_empty() || !feed.stops.rows.is_empty() {
        files.insert(STOPS_FILE, ());
    }
    files.insert(ROUTES_FILE, ());
    files.insert(TRIPS_FILE, ());
    files.insert(STOP_TIMES_FILE, ());

    if feed.calendar.is_some() {
        files.insert(CALENDAR_FILE, ());
    }
    if feed.calendar_dates.is_some() {
        files.insert(CALENDAR_DATES_FILE, ());
    }
    if feed.fare_attributes.is_some() {
        files.insert(FARE_ATTRIBUTES_FILE, ());
    }
    if feed.fare_rules.is_some() {
        files.insert(FARE_RULES_FILE, ());
    }
    if feed.fare_media.is_some() {
        files.insert(FARE_MEDIA_FILE, ());
    }
    if feed.fare_products.is_some() {
        files.insert(FARE_PRODUCTS_FILE, ());
    }
    if feed.fare_leg_rules.is_some() {
        files.insert(FARE_LEG_RULES_FILE, ());
    }
    if feed.fare_transfer_rules.is_some() {
        files.insert(FARE_TRANSFER_RULES_FILE, ());
    }
    if feed.fare_leg_join_rules.is_some() {
        files.insert(FARE_LEG_JOIN_RULES_FILE, ());
    }
    if feed.areas.is_some() {
        files.insert(AREAS_FILE, ());
    }
    if feed.stop_areas.is_some() {
        files.insert(STOP_AREAS_FILE, ());
    }
    if feed.timeframes.is_some() {
        files.insert(TIMEFRAMES_FILE, ());
    }
    if feed.rider_categories.is_some() {
        files.insert(RIDER_CATEGORIES_FILE, ());
    }
    if feed.shapes.is_some() {
        files.insert(SHAPES_FILE, ());
    }
    if feed.frequencies.is_some() {
        files.insert(FREQUENCIES_FILE, ());
    }
    if feed.transfers.is_some() {
        files.insert(TRANSFERS_FILE, ());
    }
    if feed.location_groups.is_some() {
        files.insert(LOCATION_GROUPS_FILE, ());
    }
    if feed.location_group_stops.is_some() {
        files.insert(LOCATION_GROUP_STOPS_FILE, ());
    }
    if feed.locations.is_some() {
        files.insert(LOCATIONS_GEOJSON_FILE, ());
    }
    if feed.booking_rules.is_some() {
        files.insert(BOOKING_RULES_FILE, ());
    }
    if feed.networks.is_some() {
        files.insert(NETWORKS_FILE, ());
    }
    if feed.route_networks.is_some() {
        files.insert(ROUTE_NETWORKS_FILE, ());
    }
    if feed.feed_info.is_some() {
        files.insert(FEED_INFO_FILE, ());
    }
    if feed.attributions.is_some() {
        files.insert(ATTRIBUTIONS_FILE, ());
    }
    if feed.levels.is_some() {
        files.insert(LEVELS_FILE, ());
    }
    if feed.pathways.is_some() {
        files.insert(PATHWAYS_FILE, ());
    }
    if feed.translations.is_some() {
        files.insert(TRANSLATIONS_FILE, ());
    }

    files.keys().map(|name| name.to_string()).collect()
}

fn build_gtfs_features(feed: &GtfsFeed) -> Vec<String> {
    let mut ordered = Vec::new();
    let mut index = HashMap::new();

    let add = |name: &str,
               value: bool,
               ordered: &mut Vec<(String, bool)>,
               index: &mut HashMap<String, usize>| {
        if let Some(&idx) = index.get(name) {
            ordered[idx].1 = value;
        } else {
            index.insert(name.to_string(), ordered.len());
            ordered.push((name.to_string(), value));
        }
    };

    add(
        "Pathway Connections",
        has_rows(&feed.pathways),
        &mut ordered,
        &mut index,
    );
    add(
        "Pathway Signs",
        has_rows(&feed.pathways),
        &mut ordered,
        &mut index,
    );
    add(
        "Pathway Details",
        has_rows(&feed.pathways),
        &mut ordered,
        &mut index,
    );
    add("Levels", has_rows(&feed.levels), &mut ordered, &mut index);
    add(
        "Transfers",
        has_rows(&feed.transfers),
        &mut ordered,
        &mut index,
    );
    add("Shapes", has_rows(&feed.shapes), &mut ordered, &mut index);
    add(
        "Frequencies",
        has_rows(&feed.frequencies),
        &mut ordered,
        &mut index,
    );
    add(
        "Feed Information",
        has_rows(&feed.feed_info),
        &mut ordered,
        &mut index,
    );
    add(
        "Attributions",
        has_rows(&feed.attributions),
        &mut ordered,
        &mut index,
    );
    add(
        "Translations",
        has_rows(&feed.translations),
        &mut ordered,
        &mut index,
    );
    add(
        "Fares V1",
        has_rows(&feed.fare_attributes),
        &mut ordered,
        &mut index,
    );
    add(
        "Fare Products",
        has_rows(&feed.fare_products),
        &mut ordered,
        &mut index,
    );
    add(
        "Fare Media",
        has_rows(&feed.fare_media),
        &mut ordered,
        &mut index,
    );
    add(
        "Zone-Based Fares",
        has_rows(&feed.areas),
        &mut ordered,
        &mut index,
    );
    add(
        "Fare Transfers",
        has_rows(&feed.fare_transfer_rules),
        &mut ordered,
        &mut index,
    );
    add(
        "Time-Based Fares",
        has_rows(&feed.timeframes),
        &mut ordered,
        &mut index,
    );
    add(
        "Rider Categories",
        has_rows(&feed.rider_categories),
        &mut ordered,
        &mut index,
    );
    add(
        "Booking Rules",
        has_rows(&feed.booking_rules),
        &mut ordered,
        &mut index,
    );
    add(
        "Fixed-Stops Demand Responsive Transit",
        has_rows(&feed.location_groups),
        &mut ordered,
        &mut index,
    );

    add(
        "Route Colors",
        has_route_colors(feed),
        &mut ordered,
        &mut index,
    );
    add("Headsigns", has_headsigns(feed), &mut ordered, &mut index);
    add(
        "Stops Wheelchair Accessibility",
        feed.stops
            .rows
            .iter()
            .any(|stop| stop.wheelchair_boarding.is_some()),
        &mut ordered,
        &mut index,
    );
    add(
        "Trips Wheelchair Accessibility",
        feed.trips
            .rows
            .iter()
            .any(|trip| trip.wheelchair_accessible.is_some()),
        &mut ordered,
        &mut index,
    );
    add(
        "Text-to-Speech",
        feed.stops
            .rows
            .iter()
            .any(|stop| has_text(&stop.tts_stop_name)),
        &mut ordered,
        &mut index,
    );
    add(
        "Bike Allowed",
        feed.trips
            .rows
            .iter()
            .any(|trip| trip.bikes_allowed.is_some()),
        &mut ordered,
        &mut index,
    );
    add(
        "Location Types",
        feed.stops
            .rows
            .iter()
            .any(|stop| stop.location_type.is_some()),
        &mut ordered,
        &mut index,
    );
    add(
        "In-station Traversal Time",
        feed.pathways
            .as_ref()
            .map(|table| {
                table
                    .rows
                    .iter()
                    .any(|pathway| pathway.traversal_time.is_some())
            })
            .unwrap_or(false),
        &mut ordered,
        &mut index,
    );
    add(
        "Pathway Signs",
        feed.pathways
            .as_ref()
            .map(|table| {
                table.rows.iter().any(|pathway| {
                    has_text(&pathway.signposted_as) || has_text(&pathway.reversed_signposted_as)
                })
            })
            .unwrap_or(false),
        &mut ordered,
        &mut index,
    );
    add(
        "Pathway Details",
        feed.pathways
            .as_ref()
            .map(|table| {
                table.rows.iter().any(|pathway| {
                    pathway.max_slope.is_some()
                        || pathway.min_width.is_some()
                        || pathway.length.is_some()
                        || pathway.stair_count.is_some()
                })
            })
            .unwrap_or(false),
        &mut ordered,
        &mut index,
    );
    add(
        "Pathway Connections",
        has_rows(&feed.pathways),
        &mut ordered,
        &mut index,
    );
    add(
        "Route-Based Fares",
        feed.routes
            .rows
            .iter()
            .any(|route| has_id_opt(route.network_id))
            || has_rows(&feed.networks),
        &mut ordered,
        &mut index,
    );
    add(
        "Continuous Stops",
        feed.routes
            .rows
            .iter()
            .any(|route| route.continuous_pickup.is_some() || route.continuous_drop_off.is_some())
            || feed.stop_times.rows.iter().any(|stop_time| {
                stop_time.continuous_pickup.is_some() || stop_time.continuous_drop_off.is_some()
            }),
        &mut ordered,
        &mut index,
    );
    add(
        "Zone-Based Demand Responsive Services",
        has_trip_with_only_location_id(feed),
        &mut ordered,
        &mut index,
    );
    add(
        "Predefined Routes with Deviation",
        has_trip_with_all_fields(feed),
        &mut ordered,
        &mut index,
    );

    ordered
        .into_iter()
        .filter_map(|(name, enabled)| enabled.then_some(name))
        .collect()
}

fn has_route_colors(feed: &GtfsFeed) -> bool {
    feed.routes
        .rows
        .iter()
        .any(|route| route.route_color.is_some() || route.route_text_color.is_some())
}

fn has_headsigns(feed: &GtfsFeed) -> bool {
    feed.trips
        .rows
        .iter()
        .any(|trip| has_text(&trip.trip_headsign))
        || feed
            .stop_times
            .rows
            .iter()
            .any(|stop_time| has_text(&stop_time.stop_headsign))
}

fn has_trip_with_only_location_id(feed: &GtfsFeed) -> bool {
    feed.stop_times.rows.iter().any(|stop_time| {
        has_id(stop_time.trip_id) && has_id_opt(stop_time.location_id) && !has_id(stop_time.stop_id)
    })
}

fn has_trip_with_all_fields(feed: &GtfsFeed) -> bool {
    #[derive(Default)]
    struct Flags {
        has_trip_id: bool,
        has_location_id: bool,
        has_stop_id: bool,
        has_arrival_time: bool,
        has_departure_time: bool,
    }

    let mut by_trip: HashMap<StringId, Flags> = HashMap::new();
    for stop_time in &feed.stop_times.rows {
        let trip_id = stop_time.trip_id;
        if !has_id(trip_id) {
            continue;
        }
        let flags = by_trip.entry(trip_id).or_default();
        flags.has_trip_id = true;
        flags.has_location_id |= has_id_opt(stop_time.location_id);
        flags.has_stop_id |= has_id(stop_time.stop_id);
        flags.has_arrival_time |= stop_time.arrival_time.is_some();
        flags.has_departure_time |= stop_time.departure_time.is_some();
        if flags.has_trip_id
            && flags.has_location_id
            && flags.has_stop_id
            && flags.has_arrival_time
            && flags.has_departure_time
        {
            return true;
        }
    }
    false
}

fn has_rows<T>(table: &Option<CsvTable<T>>) -> bool {
    table
        .as_ref()
        .map(|table| !table.rows.is_empty())
        .unwrap_or(false)
}

fn has_header<T>(table: &CsvTable<T>, header: &str) -> bool {
    table
        .headers
        .iter()
        .any(|value| value.eq_ignore_ascii_case(header))
}

fn has_text(value: &Option<compact_str::CompactString>) -> bool {
    value
        .as_deref()
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false)
}

fn has_id(value: StringId) -> bool {
    value.0 != 0
}

fn has_id_opt(value: Option<StringId>) -> bool {
    value.map_or(false, |id| id.0 != 0)
}

fn language_display_name(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let language = trimmed
        .split(|ch| ch == '-' || ch == '_')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let name = match language.as_str() {
        "en" => "English",
        "fr" => "French",
        "es" => "Spanish",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "ar" => "Arabic",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "no" => "Norwegian",
        "da" => "Danish",
        "fi" => "Finnish",
        "pl" => "Polish",
        "cs" => "Czech",
        "el" => "Greek",
        "tr" => "Turkish",
        "he" => "Hebrew",
        "hi" => "Hindi",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        "ms" => "Malay",
        "uk" => "Ukrainian",
        "ro" => "Romanian",
        "hu" => "Hungarian",
        "bg" => "Bulgarian",
        "hr" => "Croatian",
        "sk" => "Slovak",
        "sl" => "Slovenian",
        "sr" => "Serbian",
        "ca" => "Catalan",
        "et" => "Estonian",
        "lt" => "Lithuanian",
        "lv" => "Latvian",
        "ga" => "Irish",
        "is" => "Icelandic",
        "mk" => "Macedonian",
        "sq" => "Albanian",
        "sw" => "Swahili",
        "fa" => "Persian",
        "ur" => "Urdu",
        "bn" => "Bengali",
        "ta" => "Tamil",
        "te" => "Telugu",
        "ml" => "Malayalam",
        "mr" => "Marathi",
        "gu" => "Gujarati",
        "pa" => "Punjabi",
        "ne" => "Nepali",
        "si" => "Sinhala",
        "km" => "Khmer",
        "lo" => "Lao",
        "my" => "Burmese",
        "mn" => "Mongolian",
        "uz" => "Uzbek",
        "kk" => "Kazakh",
        "az" => "Azerbaijani",
        "ka" => "Georgian",
        "hy" => "Armenian",
        "af" => "Afrikaans",
        "am" => "Amharic",
        "eu" => "Basque",
        "be" => "Belarusian",
        "bs" => "Bosnian",
        "cy" => "Welsh",
        "gl" => "Galician",
        "ky" => "Kyrgyz",
        "lb" => "Luxembourgish",
        "mi" => "Maori",
        "ps" => "Pashto",
        "sm" => "Samoan",
        "so" => "Somali",
        "tg" => "Tajik",
        "tk" => "Turkmen",
        "tt" => "Tatar",
        "xh" => "Xhosa",
        "yo" => "Yoruba",
        "zu" => "Zulu",
        _ => return trimmed.to_string(),
    };
    name.to_string()
}

fn compute_service_window(feed: &GtfsFeed) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let has_calendar = feed.calendar.is_some();
    let has_calendar_dates = feed.calendar_dates.is_some();

    // Build set of service_ids used by trips ONCE - O(trips)
    let used_service_ids: FxHashSet<StringId> = feed
        .trips
        .rows
        .iter()
        .filter(|trip| has_id(trip.service_id))
        .map(|trip| trip.service_id)
        .collect();

    if used_service_ids.is_empty() {
        return (None, None);
    }

    let mut earliest_start = None;
    let mut latest_end = None;

    if has_calendar && !has_calendar_dates {
        // Single pass through calendar - O(calendar)
        if let Some(calendar) = &feed.calendar {
            for row in &calendar.rows {
                let service_id = row.service_id;
                if !has_id(service_id) || !used_service_ids.contains(&service_id) {
                    continue;
                }
                let start = gtfs_date_to_naive(row.start_date);
                let end = gtfs_date_to_naive(row.end_date);
                if is_epoch(start) || is_epoch(end) {
                    continue;
                }
                update_naive_bounds(start, &mut earliest_start, &mut latest_end);
                update_naive_bounds(end, &mut earliest_start, &mut latest_end);
            }
        }
    } else if has_calendar_dates && !has_calendar {
        // Single pass through calendar_dates - O(calendar_dates)
        if let Some(calendar_dates) = &feed.calendar_dates {
            for row in &calendar_dates.rows {
                let service_id = row.service_id;
                if !has_id(service_id) || !used_service_ids.contains(&service_id) {
                    continue;
                }
                let date = gtfs_date_to_naive(row.date);
                if is_epoch(date) {
                    continue;
                }
                update_naive_bounds(date, &mut earliest_start, &mut latest_end);
            }
        }
    } else if has_calendar && has_calendar_dates {
        let calendar = feed.calendar.as_ref().unwrap();
        let calendar_dates = feed.calendar_dates.as_ref().unwrap();

        // Build index by service_id - O(calendar_dates)
        let mut dates_by_service: FxHashMap<StringId, Vec<&gtfs_guru_model::CalendarDate>> =
            FxHashMap::default();
        for row in &calendar_dates.rows {
            let service_id = row.service_id;
            if !has_id(service_id) || !used_service_ids.contains(&service_id) {
                continue;
            }
            dates_by_service.entry(service_id).or_default().push(row);
        }

        // Build service periods - O(calendar)
        let mut service_periods: FxHashMap<StringId, ServicePeriodData> = FxHashMap::default();
        for row in &calendar.rows {
            let service_id = row.service_id;
            if !has_id(service_id) || !used_service_ids.contains(&service_id) {
                continue;
            }
            let dates = dates_by_service
                .get(&service_id)
                .map(|items| items.as_slice())
                .unwrap_or(&[]);
            service_periods.insert(service_id, create_service_period(Some(row), dates));
        }

        // Add calendar_dates-only services
        for (service_id, dates) in &dates_by_service {
            if service_periods.contains_key(service_id) {
                continue;
            }
            service_periods.insert(*service_id, create_service_period(None, dates));
        }

        // Single pass to compute bounds - O(service_periods)
        let mut removed_dates = Vec::new();
        for period in service_periods.values() {
            let start = period.service_start;
            let end = period.service_end;
            if !is_epoch(start) && !is_epoch(end) {
                update_naive_bounds(start, &mut earliest_start, &mut latest_end);
                update_naive_bounds(end, &mut earliest_start, &mut latest_end);
            }
            removed_dates.extend(period.removed_days.iter().copied());
        }

        for date in removed_dates {
            if let Some(earliest) = earliest_start {
                if date == earliest {
                    earliest_start = earliest.checked_add_signed(chrono::Duration::days(1));
                }
            }
            if let Some(latest) = latest_end {
                if date == latest {
                    latest_end = latest.checked_sub_signed(chrono::Duration::days(1));
                }
            }
        }
    }

    (earliest_start, latest_end)
}

fn update_naive_bounds(
    date: NaiveDate,
    min_date: &mut Option<NaiveDate>,
    max_date: &mut Option<NaiveDate>,
) {
    if min_date.map_or(true, |current| date < current) {
        *min_date = Some(date);
    }
    if max_date.map_or(true, |current| date > current) {
        *max_date = Some(date);
    }
}

fn format_gtfs_date(date: gtfs_guru_model::GtfsDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn format_naive_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn format_service_window(start: Option<NaiveDate>, end: Option<NaiveDate>) -> (String, String) {
    match (start, end) {
        (None, None) => (String::new(), String::new()),
        (Some(start), Some(end)) => (format_naive_date(start), format_naive_date(end)),
        (Some(start), None) => (format_naive_date(start), String::new()),
        (None, Some(end)) => (String::new(), format_naive_date(end)),
    }
}

fn gtfs_date_to_naive(date: gtfs_guru_model::GtfsDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
        .expect("valid gtfs date")
}

fn is_epoch(date: NaiveDate) -> bool {
    date == NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch")
}

#[derive(Debug, Clone)]
struct ServicePeriodData {
    service_start: NaiveDate,
    service_end: NaiveDate,
    removed_days: Vec<NaiveDate>,
}

fn create_service_period(
    calendar: Option<&gtfs_guru_model::Calendar>,
    calendar_dates: &[&gtfs_guru_model::CalendarDate],
) -> ServicePeriodData {
    let mut service_start = calendar.map(|row| gtfs_date_to_naive(row.start_date));
    let mut service_end = calendar.map(|row| gtfs_date_to_naive(row.end_date));

    if let (Some(start), Some(end)) = (service_start, service_end) {
        if start > end {
            service_end = Some(start);
        }
    }

    let mut removed_days = Vec::new();
    for row in calendar_dates {
        let date = gtfs_date_to_naive(row.date);
        if calendar.is_none() && row.exception_type == ExceptionType::Added {
            if service_start.map_or(true, |current| date < current) {
                service_start = Some(date);
            }
            if service_end.map_or(true, |current| date > current) {
                service_end = Some(date);
            }
        }
        if row.exception_type == ExceptionType::Removed {
            removed_days.push(date);
        }
    }

    let service_start =
        service_start.unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"));
    let service_end = service_end.unwrap_or(service_start);

    ServicePeriodData {
        service_start,
        service_end,
        removed_days,
    }
}

fn path_to_file_url(path: &Path) -> Option<String> {
    let absolute = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    Some(format!("file://{}", absolute.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtfs_guru_core::{NoticeContainer, NoticeSeverity, ValidationNotice};
    use std::collections::HashMap;

    #[test]
    fn groups_notices_by_code_and_severity() {
        let mut container = NoticeContainer::new();
        let mut notice = ValidationNotice::new("alpha", NoticeSeverity::Warning, "first");
        notice.file = Some("stops.txt".to_string());
        notice.row = Some(10);
        notice.field = Some("stop_id".to_string());
        container.push(notice);

        container.push(ValidationNotice::new(
            "alpha",
            NoticeSeverity::Warning,
            "second",
        ));
        container.push(ValidationNotice::new(
            "alpha",
            NoticeSeverity::Error,
            "third",
        ));

        let report = ValidationReport::from_container(&container);
        assert_eq!(report.notices.len(), 2);
        assert_eq!(report.notices[0].code, "alpha");
        assert_eq!(report.notices[0].severity, ReportSeverity::Warning);
        assert_eq!(report.notices[0].total_notices, 2);

        let sample = report.notices[0]
            .sample_notices
            .first()
            .expect("sample notice");
        let sample_map: HashMap<&str, &Value> =
            sample.fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
        assert_eq!(
            sample_map.get("filename").and_then(|value| value.as_str()),
            Some("stops.txt")
        );
        assert_eq!(
            sample_map
                .get("csvRowNumber")
                .and_then(|value| value.as_u64()),
            Some(10)
        );
        assert_eq!(
            sample_map.get("fieldName").and_then(|value| value.as_str()),
            Some("stop_id")
        );
    }

    #[test]
    fn includes_notice_context_fields_from_field_order() {
        let mut container = NoticeContainer::new();
        let mut notice = ValidationNotice::new("alpha", NoticeSeverity::Warning, "first");
        notice.file = Some("stops.txt".to_string());
        notice.row = Some(5);
        notice.field = Some("stop_id".to_string());
        notice.insert_context_field("fieldValue", "STOP1");
        notice.insert_context_field("extra", "X");
        notice.field_order = vec![
            "fieldValue".to_string(),
            "csvRowNumber".to_string(),
            "filename".to_string(),
            "fieldName".to_string(),
        ];
        container.push(notice);

        let report = ValidationReport::from_container(&container);
        let sample = report.notices[0]
            .sample_notices
            .first()
            .expect("sample notice");
        let sample_map: HashMap<&str, &Value> =
            sample.fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let keys: Vec<&str> = sample.fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            sample_map
                .get("fieldValue")
                .and_then(|value| value.as_str()),
            Some("STOP1")
        );
        assert_eq!(
            sample_map
                .get("csvRowNumber")
                .and_then(|value| value.as_u64()),
            Some(5)
        );
        assert_eq!(
            sample_map.get("filename").and_then(|value| value.as_str()),
            Some("stops.txt")
        );
        assert_eq!(
            sample_map.get("fieldName").and_then(|value| value.as_str()),
            Some("stop_id")
        );
        assert_eq!(
            sample_map.get("extra").and_then(|value| value.as_str()),
            Some("X")
        );
        assert_eq!(
            keys,
            vec![
                "fieldValue",
                "csvRowNumber",
                "filename",
                "fieldName",
                "extra"
            ]
        );
    }
}
