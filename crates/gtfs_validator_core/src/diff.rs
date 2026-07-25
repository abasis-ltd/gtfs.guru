//! Semantic comparison of two parsed GTFS feeds.
//!
//! IDs are resolved through each feed's own [`StringPool`](crate::StringPool).
//! A `StringId` from one feed must never be compared with a `StringId` from
//! another feed because the pools are independent.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gtfs_guru_model::{Agency, Frequency, Route, Stop};
use serde::Serialize;

use crate::feed::GTFS_FILE_NAMES;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, StringPool, TableStatus};

const MOVED_STOP_THRESHOLD_METERS: f64 = 10.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedDiff {
    pub files: FileDiff,
    pub feed_info: FeedInfoDiff,
    pub agencies: EntityDiff,
    pub routes: EntityDiff,
    pub stops: StopDiff,
    pub trips_by_route: Vec<CountChange>,
    pub frequencies_by_route: Vec<FrequencyChange>,
    pub notices: NoticeDiff,
}

impl FeedDiff {
    pub fn has_changes(&self) -> bool {
        !self.files.added.is_empty()
            || !self.files.removed.is_empty()
            || self.feed_info.changed
            || self.agencies.has_changes()
            || self.routes.has_changes()
            || self.stops.has_changes()
            || !self.trips_by_route.is_empty()
            || !self.frequencies_by_route.is_empty()
            || !self.notices.changes.is_empty()
    }

    pub fn new_error_count(&self) -> usize {
        self.notices.new_errors
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedInfoDiff {
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_service_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_service_range: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EntityDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

impl EntityDiff {
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StopDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub renamed: Vec<String>,
    pub moved: Vec<MovedStop>,
    pub changed: Vec<String>,
}

impl StopDiff {
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty()
            || !self.removed.is_empty()
            || !self.renamed.is_empty()
            || !self.moved.is_empty()
            || !self.changed.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedStop {
    pub stop_id: String,
    pub distance_meters: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountChange {
    pub route_id: String,
    pub old_count: usize,
    pub new_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyChange {
    pub route_id: String,
    pub old_windows: Vec<String>,
    pub new_windows: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeDiff {
    pub new_errors: usize,
    pub resolved_errors: usize,
    pub changes: Vec<NoticeCountChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeCountChange {
    pub code: String,
    pub severity: NoticeSeverity,
    pub old_count: usize,
    pub new_count: usize,
}

/// Compare two feeds and their validation notices.
pub fn diff_feeds(
    old: &GtfsFeed,
    new: &GtfsFeed,
    old_notices: &NoticeContainer,
    new_notices: &NoticeContainer,
) -> FeedDiff {
    FeedDiff {
        files: diff_files(old, new),
        feed_info: diff_feed_info(old, new),
        agencies: diff_agencies(old, new),
        routes: diff_routes(old, new),
        stops: diff_stops(old, new),
        trips_by_route: diff_trip_counts(old, new),
        frequencies_by_route: diff_frequencies(old, new),
        notices: diff_notices(old_notices, new_notices),
    }
}

fn diff_files(old: &GtfsFeed, new: &GtfsFeed) -> FileDiff {
    let old_files = present_files(old);
    let new_files = present_files(new);
    FileDiff {
        added: new_files.difference(&old_files).cloned().collect(),
        removed: old_files.difference(&new_files).cloned().collect(),
    }
}

fn present_files(feed: &GtfsFeed) -> BTreeSet<String> {
    GTFS_FILE_NAMES
        .iter()
        .filter(|name| feed.table_status(name) != TableStatus::MissingFile)
        .map(|name| (*name).to_string())
        .collect()
}

fn diff_feed_info(old: &GtfsFeed, new: &GtfsFeed) -> FeedInfoDiff {
    let old_info = old.feed_info.as_ref().and_then(|table| table.rows.first());
    let new_info = new.feed_info.as_ref().and_then(|table| table.rows.first());
    let old_version = old_info.and_then(|info| info.feed_version.as_ref().map(ToString::to_string));
    let new_version = new_info.and_then(|info| info.feed_version.as_ref().map(ToString::to_string));
    let old_service_range = old_info.map(|info| {
        format!(
            "{}..{}",
            info.feed_start_date
                .map(|date| date.to_string())
                .unwrap_or_default(),
            info.feed_end_date
                .map(|date| date.to_string())
                .unwrap_or_default()
        )
    });
    let new_service_range = new_info.map(|info| {
        format!(
            "{}..{}",
            info.feed_start_date
                .map(|date| date.to_string())
                .unwrap_or_default(),
            info.feed_end_date
                .map(|date| date.to_string())
                .unwrap_or_default()
        )
    });
    let changed = old_version != new_version || old_service_range != new_service_range;

    FeedInfoDiff {
        changed,
        old_version,
        new_version,
        old_service_range,
        new_service_range,
    }
}

fn diff_agencies(old: &GtfsFeed, new: &GtfsFeed) -> EntityDiff {
    let old_rows = keyed_agencies(&old.agency.rows, &old.pool);
    let new_rows = keyed_agencies(&new.agency.rows, &new.pool);
    diff_entities(old_rows, new_rows)
}

fn keyed_agencies(rows: &[Agency], pool: &StringPool) -> BTreeMap<String, String> {
    rows.iter()
        .map(|agency| {
            let id = agency
                .agency_id
                .map(|id| pool.resolve(id))
                .unwrap_or_else(|| "<default>".to_string());
            let signature = format!(
                "{}|{}|{}|{:?}|{}",
                agency.agency_name,
                pool.resolve(agency.agency_url),
                pool.resolve(agency.agency_timezone),
                agency.agency_lang.map(|id| pool.resolve(id)),
                agency.agency_phone.as_deref().unwrap_or_default()
            );
            (id, signature)
        })
        .collect()
}

fn diff_routes(old: &GtfsFeed, new: &GtfsFeed) -> EntityDiff {
    let old_rows = keyed_routes(&old.routes.rows, &old.pool);
    let new_rows = keyed_routes(&new.routes.rows, &new.pool);
    diff_entities(old_rows, new_rows)
}

fn keyed_routes(rows: &[Route], pool: &StringPool) -> BTreeMap<String, String> {
    rows.iter()
        .map(|route| {
            let id = pool.resolve(route.route_id);
            let signature = format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                route.agency_id.map(|value| pool.resolve(value)),
                route.route_short_name,
                route.route_long_name,
                route.route_type,
                route.route_color,
                route.route_text_color
            );
            (id, signature)
        })
        .collect()
}

fn diff_entities(old: BTreeMap<String, String>, new: BTreeMap<String, String>) -> EntityDiff {
    let old_ids: BTreeSet<_> = old.keys().cloned().collect();
    let new_ids: BTreeSet<_> = new.keys().cloned().collect();
    EntityDiff {
        added: new_ids.difference(&old_ids).cloned().collect(),
        removed: old_ids.difference(&new_ids).cloned().collect(),
        changed: old_ids
            .intersection(&new_ids)
            .filter(|id| old.get(*id) != new.get(*id))
            .cloned()
            .collect(),
    }
}

fn diff_stops(old: &GtfsFeed, new: &GtfsFeed) -> StopDiff {
    let old_rows: BTreeMap<_, _> = old
        .stops
        .rows
        .iter()
        .map(|stop| (old.pool.resolve(stop.stop_id), stop))
        .collect();
    let new_rows: BTreeMap<_, _> = new
        .stops
        .rows
        .iter()
        .map(|stop| (new.pool.resolve(stop.stop_id), stop))
        .collect();
    let old_ids: BTreeSet<_> = old_rows.keys().cloned().collect();
    let new_ids: BTreeSet<_> = new_rows.keys().cloned().collect();
    let mut result = StopDiff {
        added: new_ids.difference(&old_ids).cloned().collect(),
        removed: old_ids.difference(&new_ids).cloned().collect(),
        ..StopDiff::default()
    };

    for id in old_ids.intersection(&new_ids) {
        let old_stop = old_rows[id];
        let new_stop = new_rows[id];
        if old_stop.stop_name != new_stop.stop_name {
            result.renamed.push(id.clone());
        }
        if let (Some(old_lat), Some(old_lon), Some(new_lat), Some(new_lon)) = (
            old_stop.stop_lat,
            old_stop.stop_lon,
            new_stop.stop_lat,
            new_stop.stop_lon,
        ) {
            let distance = haversine_meters(old_lat, old_lon, new_lat, new_lon);
            if distance >= MOVED_STOP_THRESHOLD_METERS {
                result.moved.push(MovedStop {
                    stop_id: id.clone(),
                    distance_meters: (distance * 10.0).round() / 10.0,
                });
            }
        }
        if stop_signature(old_stop, &old.pool) != stop_signature(new_stop, &new.pool)
            && old_stop.stop_name == new_stop.stop_name
            && !result.moved.iter().any(|item| item.stop_id == *id)
        {
            result.changed.push(id.clone());
        }
    }
    result
}

fn stop_signature(stop: &Stop, pool: &StringPool) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        stop.stop_code,
        stop.stop_name,
        stop.stop_lat,
        stop.stop_lon,
        stop.location_type,
        stop.parent_station.map(|id| pool.resolve(id))
    )
}

fn diff_trip_counts(old: &GtfsFeed, new: &GtfsFeed) -> Vec<CountChange> {
    let old_counts = trip_counts(old);
    let new_counts = trip_counts(new);
    old_counts
        .keys()
        .chain(new_counts.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|route_id| {
            let old_count = old_counts.get(&route_id).copied().unwrap_or_default();
            let new_count = new_counts.get(&route_id).copied().unwrap_or_default();
            (old_count != new_count).then_some(CountChange {
                route_id,
                old_count,
                new_count,
            })
        })
        .collect()
}

fn trip_counts(feed: &GtfsFeed) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for trip in &feed.trips.rows {
        *counts.entry(feed.pool.resolve(trip.route_id)).or_default() += 1;
    }
    counts
}

fn diff_frequencies(old: &GtfsFeed, new: &GtfsFeed) -> Vec<FrequencyChange> {
    let old_windows = frequency_windows(old);
    let new_windows = frequency_windows(new);
    old_windows
        .keys()
        .chain(new_windows.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|route_id| {
            let old_values = old_windows.get(&route_id).cloned().unwrap_or_default();
            let new_values = new_windows.get(&route_id).cloned().unwrap_or_default();
            (old_values != new_values).then_some(FrequencyChange {
                route_id,
                old_windows: old_values,
                new_windows: new_values,
            })
        })
        .collect()
}

fn frequency_windows(feed: &GtfsFeed) -> BTreeMap<String, Vec<String>> {
    let trip_routes: HashMap<_, _> = feed
        .trips
        .rows
        .iter()
        .map(|trip| (trip.trip_id, feed.pool.resolve(trip.route_id)))
        .collect();
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(table) = &feed.frequencies {
        for frequency in &table.rows {
            let route = trip_routes
                .get(&frequency.trip_id)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string());
            result
                .entry(route)
                .or_default()
                .push(frequency_signature(frequency));
        }
    }
    for values in result.values_mut() {
        values.sort();
    }
    result
}

fn frequency_signature(frequency: &Frequency) -> String {
    format!(
        "{}–{} every {}s",
        frequency.start_time, frequency.end_time, frequency.headway_secs
    )
}

fn diff_notices(old: &NoticeContainer, new: &NoticeContainer) -> NoticeDiff {
    let old_counts: HashMap<_, _> = old.group_counts().into_iter().collect();
    let new_counts: HashMap<_, _> = new.group_counts().into_iter().collect();
    let mut keys: Vec<_> = old_counts
        .keys()
        .chain(new_counts.keys())
        .cloned()
        .map(|key| (key, ()))
        .collect::<HashMap<_, ()>>()
        .into_keys()
        .collect();
    keys.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| notice_severity_rank(left.1).cmp(&notice_severity_rank(right.1)))
    });
    let mut result = NoticeDiff::default();

    for (code, severity) in keys {
        let old_count = old_counts
            .get(&(code.clone(), severity))
            .copied()
            .unwrap_or_default();
        let new_count = new_counts
            .get(&(code.clone(), severity))
            .copied()
            .unwrap_or_default();
        if old_count == new_count {
            continue;
        }
        if severity == NoticeSeverity::Error {
            result.new_errors += new_count.saturating_sub(old_count);
            result.resolved_errors += old_count.saturating_sub(new_count);
        }
        result.changes.push(NoticeCountChange {
            code,
            severity,
            old_count,
            new_count,
        });
    }
    result
}

fn notice_severity_rank(severity: NoticeSeverity) -> u8 {
    match severity {
        NoticeSeverity::Error => 0,
        NoticeSeverity::Warning => 1,
        NoticeSeverity::Info => 2,
    }
}

fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let earth_radius_meters = 6_371_000.0;
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let delta_lat = (lat2 - lat1) / 2.0;
    let delta_lon = (lon2 - lon1).to_radians() / 2.0;
    let a = delta_lat.sin().powi(2) + lat1.cos() * lat2.cos() * delta_lon.sin().powi(2);
    2.0 * earth_radius_meters * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_diff_is_stable_and_sorted() {
        let old = BTreeMap::from([
            ("b".to_string(), "same".to_string()),
            ("a".to_string(), "old".to_string()),
        ]);
        let new = BTreeMap::from([
            ("b".to_string(), "same".to_string()),
            ("a".to_string(), "new".to_string()),
            ("c".to_string(), "added".to_string()),
        ]);

        assert_eq!(
            diff_entities(old, new),
            EntityDiff {
                added: vec!["c".to_string()],
                removed: vec![],
                changed: vec!["a".to_string()],
            }
        );
    }

    #[test]
    fn movement_threshold_uses_real_distance() {
        let distance = haversine_meters(35.0, 33.0, 35.0001, 33.0);
        assert!((11.0..11.2).contains(&distance));
    }

    #[test]
    fn notice_diff_counts_only_positive_error_delta_as_new() {
        let mut old = NoticeContainer::new();
        old.push(crate::ValidationNotice::new(
            "broken_stop",
            NoticeSeverity::Error,
            "old",
        ));
        old.push(crate::ValidationNotice::new(
            "old_warning",
            NoticeSeverity::Warning,
            "old",
        ));
        let mut new = NoticeContainer::new();
        for _ in 0..3 {
            new.push(crate::ValidationNotice::new(
                "broken_stop",
                NoticeSeverity::Error,
                "new",
            ));
        }

        let result = diff_notices(&old, &new);
        assert_eq!(result.new_errors, 2);
        assert_eq!(result.resolved_errors, 0);
        assert_eq!(result.changes.len(), 2);
    }
}
