use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_model::{PickupDropOffType, StopTime};

const CODE_OVERLAPPING_ZONE_AND_WINDOW: &str = "overlapping_zone_and_pickup_drop_off_window";

#[derive(Debug, Default)]
pub struct OverlappingPickupDropOffZoneValidator;

impl Validator for OverlappingPickupDropOffZoneValidator {
    fn name(&self) -> &'static str {
        "overlapping_pickup_drop_off_zone"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let locations = feed.locations.as_ref();

        let mut by_trip: HashMap<&str, Vec<(u64, &StopTime)>> = HashMap::new();
        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let trip_id = stop_time.trip_id.trim();
            if trip_id.is_empty() {
                continue;
            }
            by_trip
                .entry(trip_id)
                .or_default()
                .push((row_number, stop_time));
        }

        for stop_times in by_trip.values() {
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

                    let location_id_a = stop_time_a.location_id.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
                    let location_id_b = stop_time_b.location_id.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
                    let group_id_a = stop_time_a.location_group_id.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
                    let group_id_b = stop_time_b.location_group_id.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

                    let mut overlap = false;

                    // Case 1: Same location_id or location_group_id
                    if (location_id_a.is_some() && location_id_a == location_id_b) ||
                       (group_id_a.is_some() && group_id_a == group_id_b) {
                        overlap = true;
                    } 
                    // Case 2: Geospatial overlap of different location_ids
                    else if let (Some(loc_a), Some(loc_b), Some(locs)) = (location_id_a, location_id_b, locations) {
                        if loc_a != loc_b {
                            if let (Some(bounds_a), Some(bounds_b)) = (locs.bounds_by_id.get(loc_a), locs.bounds_by_id.get(loc_b)) {
                                if bounds_a.overlaps(bounds_b) {
                                    overlap = true;
                                }
                            }
                        }
                    }

                    if overlap {
                        let mut notice = ValidationNotice::new(
                            CODE_OVERLAPPING_ZONE_AND_WINDOW,
                            NoticeSeverity::Error,
                            "overlapping pickup/drop-off windows and zones for the same trip",
                        );
                        notice.insert_context_field("endPickupDropOffWindow1", time_value(end_a));
                        notice.insert_context_field("endPickupDropOffWindow2", time_value(end_b));
                        notice.insert_context_field("locationId1", location_id_a.or(group_id_a).unwrap_or(""));
                        notice.insert_context_field("locationId2", location_id_b.or(group_id_b).unwrap_or(""));
                        notice
                            .insert_context_field("startPickupDropOffWindow1", time_value(start_a));
                        notice
                            .insert_context_field("startPickupDropOffWindow2", time_value(start_b));
                        notice.insert_context_field("stopSequence1", stop_time_a.stop_sequence);
                        notice.insert_context_field("stopSequence2", stop_time_b.stop_sequence);
                        notice.insert_context_field("tripId", stop_time_a.trip_id.as_str());
                        notice.field_order = vec![
                            "endPickupDropOffWindow1".to_string(),
                            "endPickupDropOffWindow2".to_string(),
                            "locationId1".to_string(),
                            "locationId2".to_string(),
                            "startPickupDropOffWindow1".to_string(),
                            "startPickupDropOffWindow2".to_string(),
                            "stopSequence1".to_string(),
                            "stopSequence2".to_string(),
                            "tripId".to_string(),
                        ];
                        notices.push(notice);
                    }
                }
            }
        }
    }
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
    start_a: gtfs_model::GtfsTime,
    end_a: gtfs_model::GtfsTime,
    start_b: gtfs_model::GtfsTime,
    end_b: gtfs_model::GtfsTime,
) -> bool {
    start_a.total_seconds() < end_b.total_seconds()
        && end_a.total_seconds() > start_b.total_seconds()
}

fn time_value(value: gtfs_model::GtfsTime) -> String {
    value.to_string()
}

