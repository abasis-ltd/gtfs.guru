use crate::{
    google_rules_enabled, GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator,
};
use chrono::Datelike;
use gtfs_guru_model::TransferType;

const MAX_ROUTE_SHORT_NAME_LENGTH: usize = 6;
const MAX_HEADWAY_SECS: u32 = 3600; // 1 hour warning
                                    // const MIN_HEADWAY_SECS: u32 = 60; // Unused for now

// ===================================
// GoogleTransferTypeValidator
// ===================================
#[derive(Debug, Default)]
pub struct GoogleTransferTypeValidator;

impl Validator for GoogleTransferTypeValidator {
    fn name(&self) -> &'static str {
        "google_transfer_type"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        if let Some(transfers) = &feed.transfers {
            for (index, transfer) in transfers.rows.iter().enumerate() {
                if let Some(t_type) = transfer.transfer_type {
                    if matches!(
                        t_type,
                        TransferType::InSeat | TransferType::InSeatNotAllowed
                    ) {
                        let row_number = transfers.row_number(index);
                        let mut notice = ValidationNotice::new(
                            "google_transfer_type_check",
                            NoticeSeverity::Warning,
                            format!(
                                "Google Transit ignores transfer_type {:?} (values 4 and 5)",
                                t_type
                            ),
                        );
                        notice.set_location("transfers.txt", "transfer_type", row_number);
                        notices.push(notice);
                    }
                }
            }
        }
    }
}

// ===================================
// GoogleFareAttributesValidator
// ===================================
#[derive(Debug, Default)]
pub struct GoogleFareAttributesValidator;

impl Validator for GoogleFareAttributesValidator {
    fn name(&self) -> &'static str {
        "google_fare_attributes"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        if let Some(fares) = &feed.fare_attributes {
            for (index, fare) in fares.rows.iter().enumerate() {
                let row_number = fares.row_number(index);

                // Check ic_price
                if let Some(ic_price) = fare.ic_price {
                    if ic_price != -1.0 && ic_price < 0.0 {
                        let mut notice = ValidationNotice::new(
                            "google_ic_price_check",
                            NoticeSeverity::Error,
                            "ic_price must be -1 or a positive discounted value",
                        );
                        notice.set_location("fare_attributes.txt", "ic_price", row_number);
                        notices.push(notice);
                    }
                }
            }
        }
    }
}

// ===================================
// AgencyPhoneValidator
// ===================================
#[derive(Debug, Default)]
pub struct AgencyPhoneValidator;

impl Validator for AgencyPhoneValidator {
    fn name(&self) -> &'static str {
        "agency_phone_format"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        // Simple heuristic check for now: should contain digits, maybe some punctuation
        let is_valid_phone = |phone: &str| -> bool {
            let digits = phone.chars().filter(|c| c.is_ascii_digit()).count();
            digits >= 5 // Very basic check
        };

        for (index, agency) in feed.agency.rows.iter().enumerate() {
            if let Some(phone) = &agency.agency_phone {
                if !is_valid_phone(phone) {
                    let row_number = feed.agency.row_number(index);
                    let mut notice = ValidationNotice::new(
                        "agency_phone_invalid",
                        NoticeSeverity::Warning,
                        format!("Agency phone seems invalid: {}", phone),
                    );
                    notice.set_location("agency.txt", "agency_phone", row_number);
                    notices.push(notice);
                }
            }
        }

        if let Some(_attributions) = &feed.attributions {
            // attributions check if implemented
        }
    }
}

// ===================================
// RouteShortNameLengthValidator
// ===================================
#[derive(Debug, Default)]
pub struct RouteShortNameLengthValidator;

impl Validator for RouteShortNameLengthValidator {
    fn name(&self) -> &'static str {
        "route_short_name_length"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            if let Some(short_name) = &route.route_short_name {
                if short_name.len() > MAX_ROUTE_SHORT_NAME_LENGTH {
                    let row_number = feed.routes.row_number(index);
                    let mut notice = ValidationNotice::new(
                        "route_short_name_too_long",
                        NoticeSeverity::Warning,
                        format!("Route short name '{}' exceeds 6 characters", short_name),
                    );
                    notice.set_location("routes.txt", "route_short_name", row_number);
                    notices.push(notice);
                }
            }
        }
    }
}

// ===================================
// StopHeadsignFormatValidator
// ===================================
#[derive(Debug, Default)]
pub struct StopHeadsignFormatValidator;

impl Validator for StopHeadsignFormatValidator {
    fn name(&self) -> &'static str {
        "stop_headsign_format"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        let invalid_chars = ['!', '$', '%', '\\', '*', '=', '_'];

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            if let Some(headsign) = &stop_time.stop_headsign {
                if headsign.chars().any(|c| invalid_chars.contains(&c)) {
                    let row_number = feed.stop_times.row_number(index);
                    let mut notice = ValidationNotice::new(
                        "stop_headsign_invalid_char",
                        NoticeSeverity::Warning,
                        format!("Stop headsign contains invalid characters: {}", headsign),
                    );
                    notice.set_location("stop_times.txt", "stop_headsign", row_number);
                    notices.push(notice);
                }
            }
        }
    }
}

// ===================================
// GoogleServiceGapValidator
// ===================================

#[derive(Debug, Default)]
pub struct GoogleServiceGapValidator;

const CODE_TOO_MANY_DAYS_WITHOUT_SERVICE: &str = "too_many_days_without_service";

impl Validator for GoogleServiceGapValidator {
    fn name(&self) -> &'static str {
        "google_service_gap"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        let mut active_services: std::collections::HashMap<
            gtfs_guru_model::StringId,
            std::collections::HashSet<chrono::NaiveDate>,
        > = std::collections::HashMap::new();

        if let Some(calendar) = &feed.calendar {
            for row in &calendar.rows {
                // Convert start/end to NaiveDate
                let start_opt = chrono::NaiveDate::from_ymd_opt(
                    row.start_date.year(),
                    row.start_date.month() as u32,
                    row.start_date.day() as u32,
                );
                let end_opt = chrono::NaiveDate::from_ymd_opt(
                    row.end_date.year(),
                    row.end_date.month() as u32,
                    row.end_date.day() as u32,
                );

                if let (Some(start), Some(end)) = (start_opt, end_opt) {
                    let mut dates = std::collections::HashSet::new();
                    let mut current = start;
                    while current <= end {
                        if is_service_active(row, current) {
                            dates.insert(current);
                        }
                        current = current.succ_opt().unwrap_or(end); // Should not wrap around
                    }
                    active_services.insert(row.service_id, dates);
                }
            }
        }

        if let Some(calendar_dates) = &feed.calendar_dates {
            for row in &calendar_dates.rows {
                if let Some(date) = chrono::NaiveDate::from_ymd_opt(
                    row.date.year(),
                    row.date.month() as u32,
                    row.date.day() as u32,
                ) {
                    if row.exception_type == gtfs_guru_model::ExceptionType::Added {
                        active_services
                            .entry(row.service_id)
                            .or_default()
                            .insert(date);
                    } else if row.exception_type == gtfs_guru_model::ExceptionType::Removed {
                        if let Some(dates) = active_services.get_mut(&row.service_id) {
                            dates.remove(&date);
                        }
                    }
                }
            }
        }

        let mut all_dates: Vec<chrono::NaiveDate> = active_services
            .values()
            .flat_map(|dates| dates.iter())
            .cloned()
            .collect();

        all_dates.sort();
        all_dates.dedup();

        if all_dates.len() < 2 {
            return;
        }

        for i in 0..all_dates.len() - 1 {
            let current = all_dates[i];
            let next = all_dates[i + 1];

            // diff is duration. .num_days()
            let diff = (next - current).num_days();

            // Gap of 13 DAYS means:
            // Day 1: Service
            // Day 2..14: No service (13 days)
            // Day 15: Service
            // 15 - 1 = 14.
            // So if diff >= 14, gap >= 13.
            if diff >= 14 {
                let mut notice = ValidationNotice::new(
                     CODE_TOO_MANY_DAYS_WITHOUT_SERVICE,
                     NoticeSeverity::Warning,
                     "Reference: https://developers.google.com/transit/gtfs/guides/static-errors-warnings#too_many_days_without_service_1",
                 );
                notice.insert_context_field("firstDate", current.to_string());
                notice.insert_context_field("nextDate", next.to_string());
                notice.field_order = vec!["firstDate".into(), "nextDate".into()];
                notices.push(notice);
            }
        }
    }
}

fn is_service_active(calendar: &gtfs_guru_model::Calendar, date: chrono::NaiveDate) -> bool {
    use gtfs_guru_model::ServiceAvailability;
    match date.weekday() {
        chrono::Weekday::Mon => calendar.monday == ServiceAvailability::Available,
        chrono::Weekday::Tue => calendar.tuesday == ServiceAvailability::Available,
        chrono::Weekday::Wed => calendar.wednesday == ServiceAvailability::Available,
        chrono::Weekday::Thu => calendar.thursday == ServiceAvailability::Available,
        chrono::Weekday::Fri => calendar.friday == ServiceAvailability::Available,
        chrono::Weekday::Sat => calendar.saturday == ServiceAvailability::Available,
        chrono::Weekday::Sun => calendar.sunday == ServiceAvailability::Available,
    }
}
// ===================================
#[derive(Debug, Default)]
pub struct DuplicateTripValidator;

impl Validator for DuplicateTripValidator {
    fn name(&self) -> &'static str {
        "duplicate_trip"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        // Group stop times by trip_id
        let mut stop_times_by_trip: std::collections::HashMap<
            gtfs_guru_model::StringId,
            Vec<(
                gtfs_guru_model::StringId,
                u32,
                Option<gtfs_guru_model::GtfsTime>,
                Option<gtfs_guru_model::GtfsTime>,
            )>,
        > = std::collections::HashMap::new();
        for stop_time in &feed.stop_times.rows {
            stop_times_by_trip
                .entry(stop_time.trip_id)
                .or_default()
                .push((
                    stop_time.stop_id,
                    stop_time.stop_sequence,
                    stop_time.arrival_time,
                    stop_time.departure_time,
                ));
        }

        // Create signatures
        let mut trips_by_signature: std::collections::HashMap<u64, Vec<gtfs_guru_model::StringId>> =
            std::collections::HashMap::new();

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        for trip in &feed.trips.rows {
            let trip_id = trip.trip_id;
            let mut hasher = DefaultHasher::new();

            // Hash trip non-unique fields
            trip.service_id.hash(&mut hasher);
            trip.route_id.hash(&mut hasher);
            trip.direction_id.hash(&mut hasher);
            trip.block_id.hash(&mut hasher);
            trip.shape_id.hash(&mut hasher);

            // Hash stop times
            if let Some(stop_times) = stop_times_by_trip.get(&trip_id) {
                for (stop_id, stop_seq, arr, dep) in stop_times {
                    stop_id.hash(&mut hasher);
                    stop_seq.hash(&mut hasher);
                    arr.hash(&mut hasher);
                    dep.hash(&mut hasher);
                }
            } else {
                // No stop times? Might technically be valid empty trip, but treat as empty
                0.hash(&mut hasher);
            }

            let signature = hasher.finish();
            trips_by_signature
                .entry(signature)
                .or_default()
                .push(trip_id);
        }

        for (_signature, trip_ids) in trips_by_signature {
            if trip_ids.len() > 1 {
                let mut notice = ValidationNotice::new(
                    "duplicate_trip",
                    NoticeSeverity::Warning,
                    "Duplicate trips found (same service, route, stop times)",
                );
                notice.insert_context_field("tripCount", trip_ids.len() as u64);
                // Just showing first 5 trip IDs to avoid spam
                let example_trips = trip_ids
                    .iter()
                    .take(5)
                    .map(|id| feed.pool.resolve(*id))
                    .collect::<Vec<_>>()
                    .join(", ");
                notice.insert_context_field("exampleTripIds", example_trips);
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{set_google_rules_enabled, CsvTable};
    use gtfs_guru_model::GtfsDate;

    fn enable_google_rules() -> crate::ValidationGoogleRulesGuard {
        set_google_rules_enabled(true)
    }

    #[test]
    fn test_google_service_gap() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        feed.calendar = Some(CsvTable {
            rows: vec![gtfs_guru_model::Calendar {
                service_id: feed.pool.intern("WD"),
                monday: gtfs_guru_model::ServiceAvailability::Available,
                tuesday: gtfs_guru_model::ServiceAvailability::Available,
                wednesday: gtfs_guru_model::ServiceAvailability::Available,
                thursday: gtfs_guru_model::ServiceAvailability::Available,
                friday: gtfs_guru_model::ServiceAvailability::Available,
                saturday: gtfs_guru_model::ServiceAvailability::Available,
                sunday: gtfs_guru_model::ServiceAvailability::Available,
                start_date: GtfsDate::parse("20240101").unwrap(), // Jan 1
                end_date: GtfsDate::parse("20240131").unwrap(),   // Jan 31
            }],
            ..Default::default()
        });

        // Add a gap via calendar_dates exception (removal)
        // Remove Jan 2 to Jan 14 (13 days of gap)
        let mut dates = vec![];
        for d in 2..=14 {
            dates.push(gtfs_guru_model::CalendarDate {
                service_id: feed.pool.intern("WD"),
                date: GtfsDate::parse(&format!("202401{:02}", d)).unwrap(),
                exception_type: gtfs_guru_model::ExceptionType::Removed,
            });
        }
        feed.calendar_dates = Some(CsvTable {
            rows: dates,
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        GoogleServiceGapValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            CODE_TOO_MANY_DAYS_WITHOUT_SERVICE
        );
    }

    #[test]
    fn test_duplicate_trip() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        feed.trips = CsvTable {
            rows: vec![
                gtfs_guru_model::Trip {
                    trip_id: feed.pool.intern("T1"),
                    route_id: feed.pool.intern("R1"),
                    service_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
                gtfs_guru_model::Trip {
                    trip_id: feed.pool.intern("T2"),
                    route_id: feed.pool.intern("R1"),
                    service_id: feed.pool.intern("S1"),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        DuplicateTripValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "duplicate_trip");
    }

    #[test]
    fn test_agency_phone_invalid() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            rows: vec![gtfs_guru_model::Agency {
                agency_name: "A".into(),
                agency_url: feed.pool.intern("u"),
                agency_timezone: feed.pool.intern("z"),
                agency_phone: Some("123".into()), // Invalid (less than 5 digits)
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        AgencyPhoneValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "agency_phone_invalid");
    }

    #[test]
    fn test_google_transfer_type() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        feed.transfers = Some(CsvTable {
            rows: vec![
                gtfs_guru_model::Transfer {
                    from_stop_id: Some(feed.pool.intern("S1")),
                    to_stop_id: Some(feed.pool.intern("S2")),
                    transfer_type: Some(gtfs_guru_model::TransferType::InSeat), // Invalid for Google
                    ..Default::default()
                },
                gtfs_guru_model::Transfer {
                    from_stop_id: Some(feed.pool.intern("S3")),
                    to_stop_id: Some(feed.pool.intern("S4")),
                    transfer_type: Some(gtfs_guru_model::TransferType::Recommended), // 0 - Valid
                    ..Default::default()
                },
            ],
            ..Default::default()
        });

        let mut notices = NoticeContainer::new();
        GoogleTransferTypeValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            "google_transfer_type_check"
        );
    }

    #[test]
    fn test_route_short_name_length() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            rows: vec![gtfs_guru_model::Route {
                route_id: feed.pool.intern("R1"),
                route_short_name: Some("CrazyLongName".into()), // Too long (>6 chars)
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        RouteShortNameLengthValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            "route_short_name_too_long"
        );
    }

    #[test]
    fn test_stop_headsign_format() {
        let _guard = enable_google_rules();
        let mut feed = GtfsFeed::default();
        // The validator checks for invalid characters: !, $, %, \, *, =, _
        feed.stop_times = CsvTable {
            rows: vec![
                gtfs_guru_model::StopTime {
                    stop_headsign: Some("Destination".into()), // Valid
                    ..Default::default()
                },
                gtfs_guru_model::StopTime {
                    stop_headsign: Some("Start vs End".into()), // Valid
                    ..Default::default()
                },
                gtfs_guru_model::StopTime {
                    stop_headsign: Some("Test!Special".into()), // Invalid - contains !
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut notices = NoticeContainer::new();
        StopHeadsignFormatValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices.iter().next().unwrap().code,
            "stop_headsign_invalid_char"
        );
    }
}

// ===================================
// HeadwayReasonablenessValidator
// ===================================
#[derive(Debug, Default)]
pub struct HeadwayReasonablenessValidator;

impl Validator for HeadwayReasonablenessValidator {
    fn name(&self) -> &'static str {
        "headway_reasonableness"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !google_rules_enabled() {
            return;
        }

        if let Some(frequencies) = &feed.frequencies {
            for (index, freq) in frequencies.rows.iter().enumerate() {
                if freq.headway_secs > MAX_HEADWAY_SECS {
                    let row_number = frequencies.row_number(index);
                    let mut notice = ValidationNotice::new(
                        "headway_too_large",
                        NoticeSeverity::Warning,
                        format!(
                            "Headway of {} seconds is unusually large (> 1 hour)",
                            freq.headway_secs
                        ),
                    );
                    notice.set_location("frequencies.txt", "headway_secs", row_number);
                    notices.push(notice);
                }
            }
        }
    }
}
