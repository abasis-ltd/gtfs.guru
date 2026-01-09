use std::collections::{HashMap, HashSet};

use crate::feed::{LOCATION_GROUP_STOPS_FILE, STOPS_FILE, STOP_TIMES_FILE};
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::LocationType;

const CODE_STOP_WITHOUT_STOP_TIME: &str = "stop_without_stop_time";
const CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME: &str = "location_with_unexpected_stop_time";

#[derive(Debug, Default)]
pub struct LocationHasStopTimesValidator;

impl Validator for LocationHasStopTimesValidator {
    fn name(&self) -> &'static str {
        "location_has_stop_times"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if feed.table_has_errors(STOPS_FILE)
            || feed.table_has_errors(STOP_TIMES_FILE)
            || feed.table_has_errors(LOCATION_GROUP_STOPS_FILE)
        {
            return;
        }

        let mut stop_ids_in_stop_times: HashSet<gtfs_guru_model::StringId> = HashSet::new();
        let mut stop_time_row_by_stop_id: HashMap<gtfs_guru_model::StringId, u64> = HashMap::new();
        let mut location_group_ids: HashSet<gtfs_guru_model::StringId> = HashSet::new();

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            let stop_id = stop_time.stop_id;
            if stop_id.0 != 0 {
                stop_ids_in_stop_times.insert(stop_id);
                stop_time_row_by_stop_id
                    .entry(stop_id)
                    .or_insert(row_number);
            }
            if let Some(group_id) = stop_time.location_group_id.filter(|id| id.0 != 0) {
                location_group_ids.insert(group_id);
            }
        }

        let mut stop_ids_in_stop_times_and_groups: HashSet<gtfs_guru_model::StringId> =
            stop_ids_in_stop_times.iter().copied().collect();

        if let Some(location_group_stops) = &feed.location_group_stops {
            let mut group_to_stops: HashMap<
                gtfs_guru_model::StringId,
                Vec<gtfs_guru_model::StringId>,
            > = HashMap::new();
            for row in &location_group_stops.rows {
                let group_id = row.location_group_id;
                if group_id.0 == 0 {
                    continue;
                }
                let stop_id = row.stop_id;
                if stop_id.0 == 0 {
                    continue;
                }
                group_to_stops.entry(group_id).or_default().push(stop_id);
            }
            for group_id in location_group_ids {
                if let Some(stops) = group_to_stops.get(&group_id) {
                    for stop_id in stops {
                        stop_ids_in_stop_times_and_groups.insert(*stop_id);
                    }
                }
            }
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            let stop_id_value = feed.pool.resolve(stop_id);
            let location_type = stop.location_type.unwrap_or(LocationType::StopOrPlatform);
            if location_type == LocationType::StopOrPlatform {
                if !stop_ids_in_stop_times_and_groups.contains(&stop_id) {
                    let mut notice = ValidationNotice::new(
                        CODE_STOP_WITHOUT_STOP_TIME,
                        NoticeSeverity::Warning,
                        "stop has no stop_times entries",
                    );
                    notice.insert_context_field("csvRowNumber", row_number);
                    notice.insert_context_field("stopId", stop_id_value.as_str());
                    notice
                        .insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                    notice.field_order =
                        vec!["csvRowNumber".into(), "stopId".into(), "stopName".into()];
                    notices.push(notice);
                }
            } else if stop_ids_in_stop_times.contains(&stop_id) {
                let mut notice = ValidationNotice::new(
                    CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME,
                    NoticeSeverity::Error,
                    "non-stop location has stop_times entries",
                );
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("stopId", stop_id_value.as_str());
                notice.insert_context_field("stopName", stop.stop_name.as_deref().unwrap_or(""));
                if let Some(stop_time_row) = stop_time_row_by_stop_id.get(&stop_id) {
                    notice.insert_context_field("stopTimeCsvRowNumber", *stop_time_row);
                }
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "stopId".into(),
                    "stopName".into(),
                    "stopTimeCsvRowNumber".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{LocationType, Stop, StopTime};

    #[test]
    fn detects_stop_without_stop_time() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                location_type: Some(LocationType::StopOrPlatform),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        // stop_times is empty

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_STOP_WITHOUT_STOP_TIME
        );
    }

    #[test]
    fn detects_location_with_unexpected_stop_time() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![Stop {
                stop_id: feed.pool.intern("S1"),
                location_type: Some(LocationType::Station),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_LOCATION_WITH_UNEXPECTED_STOP_TIME
        );
    }

    #[test]
    fn passes_valid_cases() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "location_type".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    location_type: Some(LocationType::StopOrPlatform),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("P1"),
                    location_type: Some(LocationType::Station),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.stop_times = CsvTable {
            headers: vec!["stop_id".into()],
            rows: vec![StopTime {
                stop_id: feed.pool.intern("S1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        LocationHasStopTimesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
