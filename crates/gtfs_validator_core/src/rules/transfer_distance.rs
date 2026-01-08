use std::collections::HashMap;

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_TRANSFER_DISTANCE_TOO_LARGE: &str = "transfer_distance_too_large";
const CODE_TRANSFER_DISTANCE_ABOVE_2_KM: &str = "transfer_distance_above_2_km";
const MAX_DISTANCE_METERS: f64 = 10_000.0;
const INFO_DISTANCE_METERS: f64 = 2_000.0;

#[derive(Debug, Default)]
pub struct TransferDistanceValidator;

impl Validator for TransferDistanceValidator {
    fn name(&self) -> &'static str {
        "transfer_distance"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let Some(transfers) = &feed.transfers else {
            return;
        };
        if !has_transfer_stop_headers(&transfers.headers) {
            return;
        }

        let mut stops_by_id: HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop> =
            HashMap::new();
        for stop in &feed.stops.rows {
            let stop_id = stop.stop_id;
            if stop_id.0 == 0 {
                continue;
            }
            stops_by_id.insert(stop_id, stop);
        }

        for (index, transfer) in transfers.rows.iter().enumerate() {
            let row_number = transfers.row_number(index);
            let from_stop_id = transfer.from_stop_id.filter(|id| id.0 != 0);
            let to_stop_id = transfer.to_stop_id.filter(|id| id.0 != 0);
            let (Some(from_stop_id), Some(to_stop_id)) = (from_stop_id, to_stop_id) else {
                continue;
            };

            let Some((from_lat, from_lon)) = stop_or_parent_coords(from_stop_id, &stops_by_id)
            else {
                continue;
            };
            let Some((to_lat, to_lon)) = stop_or_parent_coords(to_stop_id, &stops_by_id) else {
                continue;
            };
            let distance_meters = haversine_meters(from_lat, from_lon, to_lat, to_lon);
            let distance_km = distance_meters / 1000.0;

            if distance_meters > MAX_DISTANCE_METERS {
                let from_stop_id_value = feed.pool.resolve(from_stop_id);
                let to_stop_id_value = feed.pool.resolve(to_stop_id);
                notices.push(transfer_distance_notice(
                    CODE_TRANSFER_DISTANCE_TOO_LARGE,
                    NoticeSeverity::Warning,
                    "transfer distance is larger than 10 km",
                    row_number,
                    from_stop_id_value.as_str(),
                    to_stop_id_value.as_str(),
                    distance_km,
                ));
            } else if distance_meters > INFO_DISTANCE_METERS {
                let from_stop_id_value = feed.pool.resolve(from_stop_id);
                let to_stop_id_value = feed.pool.resolve(to_stop_id);
                notices.push(transfer_distance_notice(
                    CODE_TRANSFER_DISTANCE_ABOVE_2_KM,
                    NoticeSeverity::Info,
                    "transfer distance is larger than 2 km",
                    row_number,
                    from_stop_id_value.as_str(),
                    to_stop_id_value.as_str(),
                    distance_km,
                ));
            }
        }
    }
}

fn has_transfer_stop_headers(headers: &[String]) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case("from_stop_id"))
        && headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("to_stop_id"))
}

fn stop_or_parent_coords(
    stop_id: gtfs_guru_model::StringId,
    stops_by_id: &HashMap<gtfs_guru_model::StringId, &gtfs_guru_model::Stop>,
) -> Option<(f64, f64)> {
    let mut current = stop_id;
    for _ in 0..3 {
        let stop = match stops_by_id.get(&current) {
            Some(stop) => *stop,
            None => break,
        };
        if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
            return Some((lat, lon));
        }
        if let Some(parent_station) = stop.parent_station.filter(|id| id.0 != 0) {
            current = parent_station;
        } else {
            break;
        }
    }
    None
}

fn transfer_distance_notice(
    code: &str,
    severity: NoticeSeverity,
    message: &str,
    row_number: u64,
    from_stop_id: &str,
    to_stop_id: &str,
    distance_km: f64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(code, severity, message);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fromStopId", from_stop_id);
    notice.insert_context_field("toStopId", to_stop_id);
    notice.insert_context_field("distanceKm", distance_km);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fromStopId".into(),
        "toStopId".into(),
        "distanceKm".into(),
    ];
    notice
}

fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius_meters = 6_371_000.0;
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    radius_meters * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{Stop, Transfer};

    #[test]
    fn detects_distance_too_large() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(45.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(45.1), // Approx 11km north
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransferDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_DISTANCE_TOO_LARGE));
    }

    #[test]
    fn detects_distance_above_2_km() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(45.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(45.02), // Approx 2.2km north
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransferDistanceValidator.validate(&feed, &mut notices);

        assert!(notices
            .iter()
            .any(|n| n.code == CODE_TRANSFER_DISTANCE_ABOVE_2_KM));
    }

    #[test]
    fn passes_when_short_distance() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec!["stop_id".into(), "stop_lat".into(), "stop_lon".into()],
            rows: vec![
                Stop {
                    stop_id: feed.pool.intern("S1"),
                    stop_lat: Some(45.0),
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
                Stop {
                    stop_id: feed.pool.intern("S2"),
                    stop_lat: Some(45.001), // Approx 111m north
                    stop_lon: Some(0.0),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        };
        feed.transfers = Some(CsvTable {
            headers: vec!["from_stop_id".into(), "to_stop_id".into()],
            rows: vec![Transfer {
                from_stop_id: Some(feed.pool.intern("S1")),
                to_stop_id: Some(feed.pool.intern("S2")),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        TransferDistanceValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
