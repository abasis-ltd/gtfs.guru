use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{PickupDropOffType, StopTime};

const CODE_OVERLAPPING_ZONE_AND_WINDOW: &str = "overlapping_zone_and_pickup_drop_off_window";

#[derive(Debug, Default)]
pub struct OverlappingPickupDropOffZoneValidator;

impl Validator for OverlappingPickupDropOffZoneValidator {
    fn name(&self) -> &'static str {
        "overlapping_pickup_drop_off_zone"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let locations = feed.locations.as_ref();

        // Collect stops by trip (sequential grouping is fine as O(N))
        let mut by_trip: HashMap<gtfs_guru_model::StringId, Vec<(u64, &StopTime)>> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id;
            if trip_id.0 == 0 {
                continue;
            }
            by_trip
                .entry(trip_id)
                .or_default()
                .push((row_number, stop_time));
        }

        // Parallelize validation of trips
        #[cfg(feature = "parallel")]
        let results: Vec<NoticeContainer> = {
            use rayon::prelude::*;
            let ctx = crate::ValidationContextState::capture();
            by_trip
                .par_iter()
                .map(|(_, stop_times)| {
                    let _guards = ctx.apply();
                    validate_trip(stop_times, locations, feed)
                })
                .collect()
        };

        #[cfg(feature = "parallel")]
        {
            for result in results {
                notices.merge(result);
            }
        }

        #[cfg(not(feature = "parallel"))]
        {
            for stop_times in by_trip.values() {
                let result = validate_trip(stop_times, locations, feed);
                notices.merge(result);
            }
        }
    }
}

fn validate_trip(
    stop_times: &[(u64, &StopTime)],
    locations: Option<&crate::geojson::LocationsGeoJson>,
    feed: &GtfsFeed,
) -> NoticeContainer {
    let mut notices = NoticeContainer::new();
    for i in 0..stop_times.len() {
        for j in (i + 1)..stop_times.len() {
            let (_row_a, stop_time_a) = stop_times[i];
            let (_row_b, stop_time_b) = stop_times[j];

            if should_skip_pair(stop_time_a, stop_time_b) {
                continue;
            }

            let (Some(start_a), Some(end_a)) = (
                stop_time_a.start_pickup_drop_off_window,
                stop_time_a.end_pickup_drop_off_window,
            ) else {
                continue;
            };
            let (Some(start_b), Some(end_b)) = (
                stop_time_b.start_pickup_drop_off_window,
                stop_time_b.end_pickup_drop_off_window,
            ) else {
                continue;
            };

            if !windows_overlap(start_a, end_a, start_b, end_b) {
                continue;
            }

            let location_id_a = stop_time_a.location_id.filter(|id| id.0 != 0);
            let location_id_b = stop_time_b.location_id.filter(|id| id.0 != 0);
            let group_id_a = stop_time_a.location_group_id.filter(|id| id.0 != 0);
            let group_id_b = stop_time_b.location_group_id.filter(|id| id.0 != 0);

            let mut overlap = false;

            // Case 1: Same location_id or location_group_id
            if (location_id_a.is_some() && location_id_a == location_id_b)
                || (group_id_a.is_some() && group_id_a == group_id_b)
            {
                overlap = true;
            }
            // Case 2: Geospatial overlap of different location_ids
            else if let (Some(loc_a), Some(loc_b), Some(locs)) =
                (location_id_a, location_id_b, locations)
            {
                if loc_a != loc_b {
                    if let (Some(bounds_a), Some(bounds_b)) =
                        (locs.bounds_by_id.get(&loc_a), locs.bounds_by_id.get(&loc_b))
                    {
                        if bounds_a.overlaps(bounds_b) {
                            overlap = true;
                        }
                    }
                }
            }

            if overlap {
                let location_id1_value =
                    location_id_a.or(group_id_a).map(|id| feed.pool.resolve(id));
                let location_id2_value =
                    location_id_b.or(group_id_b).map(|id| feed.pool.resolve(id));
                let location_id1 = location_id1_value
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or("");
                let location_id2 = location_id2_value
                    .as_ref()
                    .map(|value| value.as_str())
                    .unwrap_or("");
                let trip_id_value = feed.pool.resolve(stop_time_a.trip_id);
                let mut notice = ValidationNotice::new(
                    CODE_OVERLAPPING_ZONE_AND_WINDOW,
                    NoticeSeverity::Error,
                    "overlapping pickup/drop-off windows and zones for the same trip",
                );
                notice.insert_context_field("endPickupDropOffWindow1", time_value(end_a));
                notice.insert_context_field("endPickupDropOffWindow2", time_value(end_b));
                notice.insert_context_field("locationId1", location_id1);
                notice.insert_context_field("locationId2", location_id2);
                notice.insert_context_field("startPickupDropOffWindow1", time_value(start_a));
                notice.insert_context_field("startPickupDropOffWindow2", time_value(start_b));
                notice.insert_context_field("stopSequence1", stop_time_a.stop_sequence);
                notice.insert_context_field("stopSequence2", stop_time_b.stop_sequence);
                notice.insert_context_field("tripId", trip_id_value.as_str());
                notice.field_order = vec![
                    "endPickupDropOffWindow1".into(),
                    "endPickupDropOffWindow2".into(),
                    "locationId1".into(),
                    "locationId2".into(),
                    "startPickupDropOffWindow1".into(),
                    "startPickupDropOffWindow2".into(),
                    "stopSequence1".into(),
                    "stopSequence2".into(),
                    "tripId".into(),
                ];
                notices.push(notice);
            }
        }
    }
    notices
}

fn should_skip_pair(a: &StopTime, b: &StopTime) -> bool {
    if has_unknown_type(a) || has_unknown_type(b) {
        return true;
    }

    let pickup_match = a.pickup_type == b.pickup_type;
    let drop_off_match = a.drop_off_type == b.drop_off_type;
    !pickup_match && !drop_off_match
}

fn has_unknown_type(stop_time: &StopTime) -> bool {
    matches!(stop_time.pickup_type, Some(PickupDropOffType::Other))
        || matches!(stop_time.drop_off_type, Some(PickupDropOffType::Other))
}

fn windows_overlap(
    start_a: gtfs_guru_model::GtfsTime,
    end_a: gtfs_guru_model::GtfsTime,
    start_b: gtfs_guru_model::GtfsTime,
    end_b: gtfs_guru_model::GtfsTime,
) -> bool {
    start_a.total_seconds() < end_b.total_seconds()
        && end_a.total_seconds() > start_b.total_seconds()
}

fn time_value(value: gtfs_guru_model::GtfsTime) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsTime, PickupDropOffType, StopTime};

    #[test]
    fn detects_overlapping_windows_for_same_location() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "location_id".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
                "pickup_type".into(),
                "drop_off_type".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    location_id: Some(feed.pool.intern("L1")),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                    pickup_type: Some(PickupDropOffType::Regular),
                    drop_off_type: Some(PickupDropOffType::Regular),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    location_id: Some(feed.pool.intern("L1")),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(7000)), // Overlaps
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(10000)),
                    pickup_type: Some(PickupDropOffType::Regular),
                    drop_off_type: Some(PickupDropOffType::Regular),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        OverlappingPickupDropOffZoneValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_OVERLAPPING_ZONE_AND_WINDOW
        );
    }

    #[test]
    fn passes_when_no_temporal_overlap() {
        let mut feed = GtfsFeed::default();
        feed.stop_times = CsvTable {
            headers: vec![
                "trip_id".into(),
                "stop_sequence".into(),
                "location_id".into(),
                "start_pickup_drop_off_window".into(),
                "end_pickup_drop_off_window".into(),
                "pickup_type".into(),
                "drop_off_type".into(),
            ],
            rows: vec![
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 1,
                    location_id: Some(feed.pool.intern("L1")),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(3600)),
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)),
                    pickup_type: Some(PickupDropOffType::Regular),
                    drop_off_type: Some(PickupDropOffType::Regular),
                    ..Default::default()
                },
                StopTime {
                    trip_id: feed.pool.intern("T1"),
                    stop_sequence: 2,
                    location_id: Some(feed.pool.intern("L1")),
                    start_pickup_drop_off_window: Some(GtfsTime::from_seconds(7200)), // Starts at end
                    end_pickup_drop_off_window: Some(GtfsTime::from_seconds(10000)),
                    pickup_type: Some(PickupDropOffType::Regular),
                    drop_off_type: Some(PickupDropOffType::Regular),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };

        let mut notices = NoticeContainer::new();
        OverlappingPickupDropOffZoneValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
