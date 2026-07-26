//! Deterministic, model-friendly facts derived from a parsed GTFS feed.
//!
//! The types in this crate deliberately contain no generated prose or
//! provider-specific LLM calls. CLI, MCP, web, and hosted products can share
//! the same facts and explanations without risking different calculations.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use gtfs_guru_core::{build_notice_schema_map, GtfsFeed, NoticeContainer, NoticeSeverity};
use gtfs_guru_model::{ExceptionType, GtfsDate, RouteType, ServiceAvailability, StringId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const SERVICE_HORIZON_DAYS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedProfile {
    pub schema_version: u32,
    pub analysis_date: String,
    pub counts: FeedCounts,
    pub service: ServiceProfile,
    pub route_types: Vec<RouteTypeCount>,
    pub completeness: CompletenessProfile,
    pub validation: ValidationOverview,
}

impl FeedProfile {
    pub fn build(feed: &GtfsFeed, notices: &NoticeContainer, analysis_date: NaiveDate) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            analysis_date: analysis_date.to_string(),
            counts: build_counts(feed),
            service: build_service_profile(feed, analysis_date),
            route_types: build_route_types(feed),
            completeness: build_completeness(feed),
            validation: ValidationOverview::from_notices(notices),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedCounts {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
    pub shapes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfile {
    pub coverage: Option<DateRange>,
    pub horizon_days: u32,
    pub days: Vec<ServiceDayProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDayProfile {
    pub date: String,
    pub weekday: String,
    pub active_service_ids: usize,
    pub trips: usize,
    pub first_departure: Option<String>,
    pub last_arrival: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteTypeCount {
    pub route_type: String,
    pub routes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessProfile {
    pub unnamed_stops: usize,
    pub stops_without_coordinates: usize,
    pub routes_without_names: usize,
    pub trips_without_shape_id: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Errors,
    Warnings,
    Clean,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationOverview {
    pub status: ValidationStatus,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub stored_occurrences: usize,
    pub total_occurrences: usize,
    pub truncated: bool,
    pub issue_groups: Vec<NoticeGroupProfile>,
}

impl ValidationOverview {
    pub fn from_notices(notices: &NoticeContainer) -> Self {
        let (errors, warnings, infos) = notices.severity_counts();
        let schemas = build_notice_schema_map();
        let mut samples = HashMap::new();
        let mut fixable = HashSet::new();

        for notice in notices.iter() {
            let key = (notice.code.clone(), notice.severity);
            samples
                .entry(key.clone())
                .or_insert_with(|| notice.message.clone());
            if notice.fix.is_some() {
                fixable.insert(key);
            }
        }

        let mut issue_groups = notices
            .group_counts()
            .into_iter()
            .map(|((code, severity), occurrences)| {
                let key = (code.clone(), severity);
                let summary = schemas
                    .get(&code)
                    .and_then(|schema| schema.short_summary.clone())
                    .or_else(|| samples.get(&key).cloned())
                    .unwrap_or_else(|| code.replace('_', " "));
                NoticeGroupProfile {
                    code,
                    severity: severity.into(),
                    occurrences,
                    summary,
                    fix_available: fixable.contains(&key),
                }
            })
            .collect::<Vec<_>>();
        issue_groups.sort_by(|left, right| {
            right
                .severity
                .rank()
                .cmp(&left.severity.rank())
                .then_with(|| right.occurrences.cmp(&left.occurrences))
                .then_with(|| left.code.cmp(&right.code))
        });

        let status = if errors > 0 {
            ValidationStatus::Errors
        } else if warnings > 0 {
            ValidationStatus::Warnings
        } else {
            ValidationStatus::Clean
        };

        Self {
            status,
            errors,
            warnings,
            infos,
            stored_occurrences: notices.len(),
            total_occurrences: notices.total_len(),
            truncated: notices.is_truncated(),
            issue_groups,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileSeverity {
    Info,
    Warning,
    Error,
}

impl ProfileSeverity {
    fn rank(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }
}

impl From<NoticeSeverity> for ProfileSeverity {
    fn from(value: NoticeSeverity) -> Self {
        match value {
            NoticeSeverity::Info => Self::Info,
            NoticeSeverity::Warning => Self::Warning,
            NoticeSeverity::Error => Self::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeGroupProfile {
    pub code: String,
    pub severity: ProfileSeverity,
    pub occurrences: usize,
    pub summary: String,
    pub fix_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedExplanation {
    pub verdict: String,
    pub overview: Vec<String>,
    pub service: Vec<String>,
    pub priorities: Vec<ExplanationPriority>,
    pub caveats: Vec<String>,
}

impl FeedExplanation {
    pub fn from_profile(profile: &FeedProfile) -> Self {
        let verdict = match profile.validation.status {
            ValidationStatus::Errors => format!(
                "The feed has {} and should be fixed before publication.",
                counted(
                    profile.validation.errors,
                    "validation error",
                    "validation errors"
                )
            ),
            ValidationStatus::Warnings => format!(
                "The feed has no validation errors, but {} should be reviewed.",
                counted(profile.validation.warnings, "warning", "warnings")
            ),
            ValidationStatus::Clean => {
                "The validator found no errors or warnings in the feed.".to_string()
            }
        };

        let mut overview = vec![format!(
            "The feed describes {}, {}, and {} across {}.",
            counted(profile.counts.routes, "route", "routes"),
            counted(profile.counts.stops, "stop", "stops"),
            counted(profile.counts.trips, "scheduled trip", "scheduled trips"),
            counted(profile.counts.agencies, "agency", "agencies")
        )];
        if profile.completeness.unnamed_stops > 0 {
            overview.push(format!(
                "{} {} no name.",
                counted(
                    profile.completeness.unnamed_stops,
                    "stop record",
                    "stop records"
                ),
                if profile.completeness.unnamed_stops == 1 {
                    "has"
                } else {
                    "have"
                }
            ));
        }
        if profile.completeness.stops_without_coordinates > 0 {
            overview.push(format!(
                "{} {} incomplete coordinates.",
                counted(
                    profile.completeness.stops_without_coordinates,
                    "stop record",
                    "stop records"
                ),
                if profile.completeness.stops_without_coordinates == 1 {
                    "has"
                } else {
                    "have"
                }
            ));
        }
        if profile.completeness.routes_without_names > 0 {
            overview.push(format!(
                "{} {} neither a short nor a long name.",
                counted(
                    profile.completeness.routes_without_names,
                    "route record",
                    "route records"
                ),
                if profile.completeness.routes_without_names == 1 {
                    "has"
                } else {
                    "have"
                }
            ));
        }

        let mut service = Vec::new();
        if let Some(coverage) = &profile.service.coverage {
            service.push(format!(
                "Declared service coverage runs from {} through {}.",
                coverage.start, coverage.end
            ));
        } else {
            service.push("No service date range could be derived.".to_string());
        }
        for day in &profile.service.days {
            if day.trips == 0 {
                service.push(format!(
                    "{} ({}): no active scheduled trips.",
                    day.weekday, day.date
                ));
            } else {
                let time_window = match (&day.first_departure, &day.last_arrival) {
                    (Some(first), Some(last)) => format!(", service times {first}–{last}"),
                    _ => String::new(),
                };
                service.push(format!(
                    "{} ({}): {}{}.",
                    day.weekday,
                    day.date,
                    counted(day.trips, "trip", "trips"),
                    time_window
                ));
            }
        }

        let priorities = profile
            .validation
            .issue_groups
            .iter()
            .filter(|issue| issue.severity != ProfileSeverity::Info)
            .take(5)
            .cloned()
            .map(ExplanationPriority::from)
            .collect();

        Self {
            verdict,
            overview,
            service,
            priorities,
            caveats: vec![
                format!(
                    "Service facts use the seven-day window beginning {} and include calendar date exceptions.",
                    profile.analysis_date
                ),
                "GTFS service times are agency-local schedule times and may legitimately exceed 24:00:00."
                    .to_string(),
                "Validation cannot guarantee acceptance by Google Maps or any other downstream consumer."
                    .to_string(),
            ],
        }
    }

    pub fn for_unreadable_feed(validation: ValidationOverview, analysis_date: NaiveDate) -> Self {
        let priorities = validation
            .issue_groups
            .iter()
            .take(5)
            .cloned()
            .map(ExplanationPriority::from)
            .collect();
        Self {
            verdict: "The feed could not be parsed, so schedule facts are unavailable.".to_string(),
            overview: vec![format!(
                "The validator reported {} errors while loading the feed.",
                validation.errors
            )],
            service: Vec::new(),
            priorities,
            caveats: vec![
                format!("The attempted analysis date was {analysis_date}."),
                "Fix the loading errors before relying on any feed-level summary.".to_string(),
            ],
        }
    }

    pub fn render_markdown(&self) -> String {
        let mut output = format!("# GTFS feed explanation\n\n{}\n", self.verdict);
        append_bullets(&mut output, "Overview", &self.overview);
        append_bullets(&mut output, "Service", &self.service);

        output.push_str("\n## Priorities\n\n");
        if self.priorities.is_empty() {
            output.push_str("- No error or warning groups to prioritize.\n");
        } else {
            for priority in &self.priorities {
                let fix = if priority.fix_available {
                    " An automatic fix is available."
                } else {
                    ""
                };
                output.push_str(&format!(
                    "- **{} — `{}`**: {} ({}).{}\n",
                    severity_label(priority.severity),
                    priority.code,
                    priority.summary,
                    counted(priority.occurrences, "occurrence", "occurrences"),
                    fix
                ));
            }
        }
        append_bullets(&mut output, "Caveats", &self.caveats);
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationPriority {
    pub code: String,
    pub severity: ProfileSeverity,
    pub occurrences: usize,
    pub summary: String,
    pub fix_available: bool,
}

impl From<NoticeGroupProfile> for ExplanationPriority {
    fn from(value: NoticeGroupProfile) -> Self {
        Self {
            code: value.code,
            severity: value.severity,
            occurrences: value.occurrences,
            summary: value.summary,
            fix_available: value.fix_available,
        }
    }
}

fn append_bullets(output: &mut String, heading: &str, values: &[String]) {
    output.push_str(&format!("\n## {heading}\n\n"));
    if values.is_empty() {
        output.push_str("- No data available.\n");
    } else {
        for value in values {
            output.push_str(&format!("- {value}\n"));
        }
    }
}

fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

fn severity_label(severity: ProfileSeverity) -> &'static str {
    match severity {
        ProfileSeverity::Info => "INFO",
        ProfileSeverity::Warning => "WARNING",
        ProfileSeverity::Error => "ERROR",
    }
}

fn build_counts(feed: &GtfsFeed) -> FeedCounts {
    let agencies = if feed.agency.rows.iter().any(|row| has_id(row.agency_id)) {
        unique_optional_ids(feed.agency.rows.iter().map(|row| row.agency_id))
    } else {
        feed.agency.rows.len()
    };
    let shapes = feed
        .shapes
        .as_ref()
        .map(|table| unique_ids(table.rows.iter().map(|row| row.shape_id)))
        .unwrap_or(0);

    FeedCounts {
        agencies,
        routes: unique_ids(feed.routes.rows.iter().map(|row| row.route_id)),
        stops: unique_ids(feed.stops.rows.iter().map(|row| row.stop_id)),
        trips: unique_ids(feed.trips.rows.iter().map(|row| row.trip_id)),
        stop_times: feed.stop_times.rows.len(),
        shapes,
    }
}

fn unique_ids(values: impl Iterator<Item = StringId>) -> usize {
    values
        .filter(|value| value.0 != 0)
        .collect::<HashSet<_>>()
        .len()
}

fn unique_optional_ids(values: impl Iterator<Item = Option<StringId>>) -> usize {
    values
        .flatten()
        .filter(|value| value.0 != 0)
        .collect::<HashSet<_>>()
        .len()
}

fn has_id(value: Option<StringId>) -> bool {
    value.is_some_and(|id| id.0 != 0)
}

fn build_completeness(feed: &GtfsFeed) -> CompletenessProfile {
    CompletenessProfile {
        unnamed_stops: feed
            .stops
            .rows
            .iter()
            .filter(|stop| {
                stop.stop_name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty())
            })
            .count(),
        stops_without_coordinates: feed
            .stops
            .rows
            .iter()
            .filter(|stop| stop.stop_lat.is_none() || stop.stop_lon.is_none())
            .count(),
        routes_without_names: feed
            .routes
            .rows
            .iter()
            .filter(|route| {
                let no_short = route
                    .route_short_name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty());
                let no_long = route
                    .route_long_name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty());
                no_short && no_long
            })
            .count(),
        trips_without_shape_id: feed
            .trips
            .rows
            .iter()
            .filter(|trip| !has_id(trip.shape_id))
            .count(),
    }
}

fn build_route_types(feed: &GtfsFeed) -> Vec<RouteTypeCount> {
    let mut counts = BTreeMap::new();
    let mut seen = HashSet::new();
    for route in &feed.routes.rows {
        if route.route_id.0 == 0 || !seen.insert(route.route_id) {
            continue;
        }
        *counts.entry(route_type_name(route.route_type)).or_insert(0) += 1;
    }
    let mut result = counts
        .into_iter()
        .map(|(route_type, routes)| RouteTypeCount { route_type, routes })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .routes
            .cmp(&left.routes)
            .then_with(|| left.route_type.cmp(&right.route_type))
    });
    result
}

fn route_type_name(route_type: RouteType) -> String {
    match route_type {
        RouteType::Tram => "tram".to_string(),
        RouteType::Subway => "subway".to_string(),
        RouteType::Rail => "rail".to_string(),
        RouteType::Bus => "bus".to_string(),
        RouteType::Ferry => "ferry".to_string(),
        RouteType::CableCar => "cable_car".to_string(),
        RouteType::Gondola => "gondola".to_string(),
        RouteType::Funicular => "funicular".to_string(),
        RouteType::Trolleybus => "trolleybus".to_string(),
        RouteType::Monorail => "monorail".to_string(),
        RouteType::Extended(code) => format!("extended_{code}"),
        RouteType::Unknown => "unknown".to_string(),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ServiceSchedule {
    trips: usize,
    first_departure: Option<i32>,
    last_arrival: Option<i32>,
}

fn build_service_profile(feed: &GtfsFeed, analysis_date: NaiveDate) -> ServiceProfile {
    let schedules = build_service_schedules(feed);
    let days = (0..SERVICE_HORIZON_DAYS)
        .map(|offset| {
            let date = analysis_date + Duration::days(offset);
            let active = active_services_on(feed, date);
            let mut trips = 0;
            let mut first_departure = None;
            let mut last_arrival = None;
            for service_id in &active {
                if let Some(schedule) = schedules.get(service_id) {
                    trips += schedule.trips;
                    min_assign(&mut first_departure, schedule.first_departure);
                    max_assign(&mut last_arrival, schedule.last_arrival);
                }
            }
            ServiceDayProfile {
                date: date.to_string(),
                weekday: weekday_name(date.weekday()).to_string(),
                active_service_ids: active.len(),
                trips,
                first_departure: first_departure.map(format_gtfs_time),
                last_arrival: last_arrival.map(format_gtfs_time),
            }
        })
        .collect();

    ServiceProfile {
        coverage: service_coverage(feed),
        horizon_days: SERVICE_HORIZON_DAYS as u32,
        days,
    }
}

fn build_service_schedules(feed: &GtfsFeed) -> HashMap<StringId, ServiceSchedule> {
    let mut schedules = HashMap::<StringId, ServiceSchedule>::new();
    let mut seen_trips = HashSet::new();
    let mut frequencies_by_trip = HashMap::new();
    if let Some(frequencies) = &feed.frequencies {
        for frequency in &frequencies.rows {
            frequencies_by_trip
                .entry(frequency.trip_id)
                .or_insert_with(Vec::new)
                .push(frequency);
        }
    }

    for trip in &feed.trips.rows {
        if trip.trip_id.0 == 0 || !seen_trips.insert(trip.trip_id) {
            continue;
        }
        let schedule = schedules.entry(trip.service_id).or_default();
        schedule.trips += 1;
        let mut trip_first = None;
        let mut trip_last = None;
        if let Some(indices) = feed.stop_times_by_trip.get(&trip.trip_id) {
            for index in indices {
                if let Some(stop_time) = feed.stop_times.rows.get(*index) {
                    let departure = stop_time
                        .departure_time
                        .or(stop_time.arrival_time)
                        .or(stop_time.start_pickup_drop_off_window);
                    let arrival = stop_time
                        .arrival_time
                        .or(stop_time.departure_time)
                        .or(stop_time.end_pickup_drop_off_window);
                    min_assign(&mut trip_first, departure.map(|time| time.total_seconds()));
                    max_assign(&mut trip_last, arrival.map(|time| time.total_seconds()));
                }
            }
        }
        if let Some(frequencies) = frequencies_by_trip.get(&trip.trip_id) {
            let trip_duration = match (trip_first, trip_last) {
                (Some(first), Some(last)) => last.saturating_sub(first).max(0),
                _ => 0,
            };
            for frequency in frequencies {
                min_assign(
                    &mut schedule.first_departure,
                    Some(frequency.start_time.total_seconds()),
                );
                max_assign(
                    &mut schedule.last_arrival,
                    Some(
                        frequency
                            .end_time
                            .total_seconds()
                            .saturating_add(trip_duration),
                    ),
                );
            }
        } else {
            min_assign(&mut schedule.first_departure, trip_first);
            max_assign(&mut schedule.last_arrival, trip_last);
        }
    }
    schedules
}

fn min_assign(target: &mut Option<i32>, candidate: Option<i32>) {
    if let Some(candidate) = candidate {
        *target = Some(target.map_or(candidate, |current| current.min(candidate)));
    }
}

fn max_assign(target: &mut Option<i32>, candidate: Option<i32>) {
    if let Some(candidate) = candidate {
        *target = Some(target.map_or(candidate, |current| current.max(candidate)));
    }
}

fn active_services_on(feed: &GtfsFeed, date: NaiveDate) -> HashSet<StringId> {
    let mut active = HashSet::new();
    if let Some(calendar) = &feed.calendar {
        for row in &calendar.rows {
            let Some(start) = naive_date(row.start_date) else {
                continue;
            };
            let Some(end) = naive_date(row.end_date) else {
                continue;
            };
            if start <= date && date <= end && available_on(row, date.weekday()) {
                active.insert(row.service_id);
            }
        }
    }
    if let Some(calendar_dates) = &feed.calendar_dates {
        for row in &calendar_dates.rows {
            if naive_date(row.date) != Some(date) {
                continue;
            }
            match row.exception_type {
                ExceptionType::Added => {
                    active.insert(row.service_id);
                }
                ExceptionType::Removed => {
                    active.remove(&row.service_id);
                }
                ExceptionType::Other => {}
            }
        }
    }
    active
}

fn available_on(calendar: &gtfs_guru_model::Calendar, weekday: Weekday) -> bool {
    let availability = match weekday {
        Weekday::Mon => calendar.monday,
        Weekday::Tue => calendar.tuesday,
        Weekday::Wed => calendar.wednesday,
        Weekday::Thu => calendar.thursday,
        Weekday::Fri => calendar.friday,
        Weekday::Sat => calendar.saturday,
        Weekday::Sun => calendar.sunday,
    };
    availability == ServiceAvailability::Available
}

fn service_coverage(feed: &GtfsFeed) -> Option<DateRange> {
    let mut start = None;
    let mut end = None;
    if let Some(calendar) = &feed.calendar {
        for row in &calendar.rows {
            assign_date_range(&mut start, &mut end, naive_date(row.start_date));
            assign_date_range(&mut start, &mut end, naive_date(row.end_date));
        }
    }
    if let Some(calendar_dates) = &feed.calendar_dates {
        for row in &calendar_dates.rows {
            if row.exception_type == ExceptionType::Added {
                assign_date_range(&mut start, &mut end, naive_date(row.date));
            }
        }
    }
    start.zip(end).map(|(start, end)| DateRange {
        start: start.to_string(),
        end: end.to_string(),
    })
}

fn assign_date_range(
    start: &mut Option<NaiveDate>,
    end: &mut Option<NaiveDate>,
    candidate: Option<NaiveDate>,
) {
    if let Some(candidate) = candidate {
        *start = Some(start.map_or(candidate, |current| current.min(candidate)));
        *end = Some(end.map_or(candidate, |current| current.max(candidate)));
    }
}

fn naive_date(date: GtfsDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32)
}

fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

fn format_gtfs_time(total_seconds: i32) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtfs_guru_core::{CsvTable, StringPool, ValidationNotice};
    use gtfs_guru_model::{
        Agency, Calendar, CalendarDate, Frequency, GtfsTime, Route, Stop, StopTime, Trip,
    };

    #[test]
    fn profile_uses_calendar_exceptions_and_extended_service_times() {
        let mut feed = sample_feed();
        feed.rebuild_stop_times_index();
        let notices = NoticeContainer::new();
        let monday = NaiveDate::from_ymd_opt(2026, 7, 27).unwrap();

        let profile = FeedProfile::build(&feed, &notices, monday);

        assert_eq!(profile.counts.routes, 1);
        assert_eq!(profile.counts.stops, 2);
        assert_eq!(profile.counts.trips, 2);
        assert_eq!(profile.service.days[0].weekday, "Monday");
        assert_eq!(profile.service.days[0].trips, 1);
        assert_eq!(
            profile.service.days[0].last_arrival.as_deref(),
            Some("25:15:00")
        );
        assert_eq!(profile.service.days[1].trips, 0);
        assert_eq!(profile.service.days[5].trips, 1);
    }

    #[test]
    fn notice_groups_are_ranked_and_keep_exact_totals() {
        let feed = sample_feed();
        let mut notices = NoticeContainer::with_group_limit(Some(1));
        for _ in 0..3 {
            notices.push(ValidationNotice::new(
                "missing_required_field",
                NoticeSeverity::Error,
                "missing",
            ));
        }
        notices.push(ValidationNotice::new(
            "missing_recommended_file",
            NoticeSeverity::Warning,
            "recommended",
        ));

        let profile = FeedProfile::build(
            &feed,
            &notices,
            NaiveDate::from_ymd_opt(2026, 7, 27).unwrap(),
        );

        assert_eq!(profile.validation.total_occurrences, 4);
        assert_eq!(profile.validation.stored_occurrences, 2);
        assert!(profile.validation.truncated);
        assert_eq!(profile.validation.issue_groups[0].occurrences, 3);
        assert_eq!(
            profile.validation.issue_groups[0].severity,
            ProfileSeverity::Error
        );
    }

    #[test]
    fn frequency_windows_expand_the_reported_service_time() {
        let mut feed = sample_feed();
        let weekday_trip = feed.pool.intern("weekday-trip");
        for stop_time in &mut feed.stop_times.rows {
            if stop_time.trip_id == weekday_trip && stop_time.stop_sequence == 2 {
                stop_time.arrival_time = Some(GtfsTime::from_seconds(7 * 3600));
                stop_time.departure_time = Some(GtfsTime::from_seconds(7 * 3600));
            }
        }
        feed.frequencies = Some(CsvTable {
            rows: vec![Frequency {
                trip_id: weekday_trip,
                start_time: GtfsTime::from_seconds(5 * 3600),
                end_time: GtfsTime::from_seconds(10 * 3600),
                headway_secs: 900,
                exact_times: None,
            }],
            ..CsvTable::default()
        });
        feed.rebuild_stop_times_index();

        let profile = FeedProfile::build(
            &feed,
            &NoticeContainer::new(),
            NaiveDate::from_ymd_opt(2026, 7, 27).unwrap(),
        );

        assert_eq!(
            profile.service.days[0].first_departure.as_deref(),
            Some("05:00:00")
        );
        assert_eq!(
            profile.service.days[0].last_arrival.as_deref(),
            Some("11:00:00")
        );
    }

    #[test]
    fn explanation_is_deterministic_and_carries_caveats() {
        let feed = sample_feed();
        let profile = FeedProfile::build(
            &feed,
            &NoticeContainer::new(),
            NaiveDate::from_ymd_opt(2026, 7, 27).unwrap(),
        );
        let explanation = FeedExplanation::from_profile(&profile);
        let markdown = explanation.render_markdown();

        assert!(markdown.contains("1 route, 2 stops, and 2 scheduled trips"));
        assert!(markdown.contains("25:15:00"));
        assert!(markdown.contains("cannot guarantee acceptance by Google Maps"));
    }

    #[test]
    fn removed_calendar_dates_do_not_expand_service_coverage() {
        let mut feed = sample_feed();
        feed.calendar_dates
            .as_mut()
            .unwrap()
            .rows
            .push(CalendarDate {
                service_id: feed.pool.intern("weekday"),
                date: GtfsDate::parse("20270101").unwrap(),
                exception_type: ExceptionType::Removed,
            });

        let profile = FeedProfile::build(
            &feed,
            &NoticeContainer::new(),
            NaiveDate::from_ymd_opt(2026, 7, 27).unwrap(),
        );

        assert_eq!(
            profile.service.coverage,
            Some(DateRange {
                start: "2026-07-01".to_string(),
                end: "2026-08-31".to_string(),
            })
        );
    }

    fn sample_feed() -> GtfsFeed {
        let pool = StringPool::new();
        let agency_id = pool.intern("agency");
        let route_id = pool.intern("route");
        let weekday_service = pool.intern("weekday");
        let weekend_service = pool.intern("weekend");
        let weekday_trip = pool.intern("weekday-trip");
        let weekend_trip = pool.intern("weekend-trip");
        let stop_a = pool.intern("stop-a");
        let stop_b = pool.intern("stop-b");

        let mut feed = GtfsFeed {
            pool,
            agency: CsvTable {
                rows: vec![Agency {
                    agency_id: Some(agency_id),
                    agency_name: "Example Transit".into(),
                    ..Agency::default()
                }],
                ..CsvTable::default()
            },
            routes: CsvTable {
                rows: vec![Route {
                    route_id,
                    agency_id: Some(agency_id),
                    route_short_name: Some("1".into()),
                    route_type: RouteType::Bus,
                    ..Route::default()
                }],
                ..CsvTable::default()
            },
            stops: CsvTable {
                rows: vec![
                    Stop {
                        stop_id: stop_a,
                        stop_name: Some("First".into()),
                        stop_lat: Some(35.0),
                        stop_lon: Some(33.0),
                        ..Stop::default()
                    },
                    Stop {
                        stop_id: stop_b,
                        ..Stop::default()
                    },
                ],
                ..CsvTable::default()
            },
            trips: CsvTable {
                rows: vec![
                    Trip {
                        route_id,
                        service_id: weekday_service,
                        trip_id: weekday_trip,
                        ..Trip::default()
                    },
                    Trip {
                        route_id,
                        service_id: weekend_service,
                        trip_id: weekend_trip,
                        ..Trip::default()
                    },
                ],
                ..CsvTable::default()
            },
            stop_times: CsvTable {
                rows: vec![
                    stop_time(weekday_trip, stop_a, 1, 6 * 3600),
                    stop_time(weekday_trip, stop_b, 2, 25 * 3600 + 15 * 60),
                    stop_time(weekend_trip, stop_a, 1, 8 * 3600),
                    stop_time(weekend_trip, stop_b, 2, 9 * 3600),
                ],
                ..CsvTable::default()
            },
            calendar: Some(CsvTable {
                rows: vec![
                    Calendar {
                        service_id: weekday_service,
                        monday: ServiceAvailability::Available,
                        tuesday: ServiceAvailability::Available,
                        wednesday: ServiceAvailability::Available,
                        thursday: ServiceAvailability::Available,
                        friday: ServiceAvailability::Available,
                        saturday: ServiceAvailability::Unavailable,
                        sunday: ServiceAvailability::Unavailable,
                        start_date: GtfsDate::parse("20260701").unwrap(),
                        end_date: GtfsDate::parse("20260831").unwrap(),
                    },
                    Calendar {
                        service_id: weekend_service,
                        monday: ServiceAvailability::Unavailable,
                        tuesday: ServiceAvailability::Unavailable,
                        wednesday: ServiceAvailability::Unavailable,
                        thursday: ServiceAvailability::Unavailable,
                        friday: ServiceAvailability::Unavailable,
                        saturday: ServiceAvailability::Available,
                        sunday: ServiceAvailability::Available,
                        start_date: GtfsDate::parse("20260701").unwrap(),
                        end_date: GtfsDate::parse("20260831").unwrap(),
                    },
                ],
                ..CsvTable::default()
            }),
            calendar_dates: Some(CsvTable {
                rows: vec![
                    CalendarDate {
                        service_id: weekday_service,
                        date: GtfsDate::parse("20260728").unwrap(),
                        exception_type: ExceptionType::Removed,
                    },
                    CalendarDate {
                        service_id: weekend_service,
                        date: GtfsDate::parse("20260801").unwrap(),
                        exception_type: ExceptionType::Added,
                    },
                ],
                ..CsvTable::default()
            }),
            ..GtfsFeed::default()
        };
        feed.rebuild_stop_times_index();
        feed
    }

    fn stop_time(trip_id: StringId, stop_id: StringId, sequence: u32, seconds: i32) -> StopTime {
        StopTime {
            trip_id,
            stop_id,
            stop_sequence: sequence,
            arrival_time: Some(GtfsTime::from_seconds(seconds)),
            departure_time: Some(GtfsTime::from_seconds(seconds)),
            ..StopTime::default()
        }
    }
}
