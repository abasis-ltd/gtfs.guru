use crate::feed::{
    GtfsFeed, AGENCY_FILE, BOOKING_RULES_FILE, CALENDAR_DATES_FILE, CALENDAR_FILE,
    FARE_ATTRIBUTES_FILE, FARE_LEG_JOIN_RULES_FILE, FARE_MEDIA_FILE, FARE_PRODUCTS_FILE,
    FARE_RULES_FILE, FEED_INFO_FILE, FREQUENCIES_FILE, GTFS_FILE_NAMES, LOCATIONS_GEOJSON_FILE,
    LOCATION_GROUPS_FILE, LOCATION_GROUP_STOPS_FILE, NETWORKS_FILE, PATHWAYS_FILE,
    RIDER_CATEGORIES_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE, SHAPES_FILE, STOPS_FILE,
    STOP_TIMES_FILE, TIMEFRAMES_FILE, TRANSFERS_FILE, TRANSLATIONS_FILE, TRIPS_FILE,
};
use crate::{NoticeContainer, ValidationNotice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStatus {
    Ok,
    MissingFile,
    ParseError,
}

impl TableStatus {
    /// Returns true if the table was parsed without errors.
    /// Missing files are considered successfully parsed (nothing to parse).
    /// Only ParseError returns false.
    pub fn is_parsed_successfully(self) -> bool {
        !matches!(self, TableStatus::ParseError)
    }
}

/// What a notice's canonical validator needs parsed before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoticeDependencies {
    /// Raised by the loader or a single-entity validator (which runs on every
    /// clean row), or by a rule the canonical validator does not have.
    None,
    /// Raised by a validator injected with these tables.
    Tables(Vec<&'static str>),
    /// Raised by a validator injected with the whole feed.
    WholeFeed,
}

/// The canonical validator skips a multi-file validator when any table it is
/// injected with is not parsed successfully (`ValidatorLoader`'s dependency
/// check): a table with a header or row error, an empty file, or a missing
/// required file. The validators here do not map one to one onto the
/// canonical ones, so the gate works per notice: a notice is kept only when
/// the canonical validator that raises it would have run.
///
/// The table lists come from the canonical v8.0.1 validators' constructors.
pub(crate) struct DependencyGate {
    failed: Vec<&'static str>,
}

/// Tables whose absence the canonical validator counts as a parse failure
/// (`GtfsTableDescriptor.isRequired()`). `stops.txt` is not among them: since
/// GTFS-Flex a feed may locate stops in `locations.geojson` instead, and
/// `MissingStopsFileValidator` reports the gap.
const JAVA_REQUIRED_TABLES: &[&str] = &[AGENCY_FILE, ROUTES_FILE, STOP_TIMES_FILE, TRIPS_FILE];

impl DependencyGate {
    pub(crate) fn new(feed: &GtfsFeed) -> Self {
        Self {
            failed: GTFS_FILE_NAMES
                .iter()
                .copied()
                .filter(|file| match feed.table_status(file) {
                    TableStatus::Ok => false,
                    TableStatus::ParseError => true,
                    TableStatus::MissingFile => JAVA_REQUIRED_TABLES.contains(file),
                })
                .collect(),
        }
    }

    /// Every table parsed: nothing to gate.
    pub(crate) fn is_open(&self) -> bool {
        self.failed.is_empty()
    }

    pub(crate) fn allows(&self, validator: &str, notice: &ValidationNotice) -> bool {
        if self.is_open() {
            return true;
        }
        match notice_dependencies(validator, notice) {
            NoticeDependencies::None => true,
            NoticeDependencies::WholeFeed => false,
            NoticeDependencies::Tables(tables) => tables.iter().all(|table| {
                !self
                    .failed
                    .iter()
                    .any(|failed| failed.eq_ignore_ascii_case(table))
            }),
        }
    }

    /// Drop the notices of `validator` whose canonical validator would have
    /// been skipped. `notices` must hold every notice (no group cap), so the
    /// totals of what is kept stay exact.
    pub(crate) fn filter(&self, validator: &str, notices: NoticeContainer) -> NoticeContainer {
        let mut kept = NoticeContainer::new();
        for notice in notices.into_vec() {
            if self.allows(validator, &notice) {
                kept.push(notice);
            }
        }
        kept
    }
}

fn context_str<'a>(notice: &'a ValidationNotice, key: &str) -> Option<&'a str> {
    notice.context.get(key).and_then(|value| value.as_str())
}

fn notice_filename(notice: &ValidationNotice) -> Option<&str> {
    context_str(notice, "filename").or(notice.file.as_deref())
}

fn tables(list: &[&'static str]) -> NoticeDependencies {
    NoticeDependencies::Tables(list.to_vec())
}

/// Map a file name as written in a notice to the feed's constant.
fn known_file(name: &str) -> Option<&'static str> {
    GTFS_FILE_NAMES
        .iter()
        .copied()
        .find(|file| file.eq_ignore_ascii_case(name.trim()))
}

pub(crate) fn notice_dependencies(
    validator: &str,
    notice: &ValidationNotice,
) -> NoticeDependencies {
    let code = notice.code.as_str();
    match code {
        // Shared codes: which canonical validator raised it decides.
        "foreign_key_violation" => {
            // `<child>` plus every parent; a parent may read
            // `calendar.txt or calendar_dates.txt`.
            let mut deps = Vec::new();
            for key in ["childFilename", "parentFilename"] {
                if let Some(names) = context_str(notice, key) {
                    deps.extend(names.split(" or ").filter_map(known_file));
                }
            }
            if deps.is_empty() {
                NoticeDependencies::None
            } else {
                NoticeDependencies::Tables(deps)
            }
        }
        "missing_recommended_field" | "missing_required_agency_id" => {
            match notice_filename(notice).and_then(known_file) {
                Some(AGENCY_FILE) => tables(&[AGENCY_FILE]),
                Some(ROUTES_FILE) => tables(&[AGENCY_FILE, ROUTES_FILE]),
                Some(FARE_ATTRIBUTES_FILE) => tables(&[AGENCY_FILE, FARE_ATTRIBUTES_FILE]),
                // feed_info (loader) and fare_media (single-entity).
                _ => NoticeDependencies::None,
            }
        }
        "missing_required_field" => match notice_filename(notice).and_then(known_file) {
            Some(TRANSFERS_FILE) if validator == "transfers_in_seat_transfer_type" => {
                tables(&[STOPS_FILE, STOP_TIMES_FILE, TRANSFERS_FILE])
            }
            Some(TRANSFERS_FILE) if validator == "transfer_stop_ids_conditional" => {
                tables(&[TRANSFERS_FILE])
            }
            Some(FARE_LEG_JOIN_RULES_FILE) if validator != "required_fields_non_empty" => {
                tables(&[FARE_LEG_JOIN_RULES_FILE, NETWORKS_FILE, ROUTES_FILE])
            }
            Some(TRANSLATIONS_FILE) if validator != "required_fields_non_empty" => {
                NoticeDependencies::WholeFeed
            }
            _ => NoticeDependencies::None,
        },
        "missing_required_file" => match notice_filename(notice).and_then(known_file) {
            Some(STOPS_FILE) => tables(&[LOCATIONS_GEOJSON_FILE, STOPS_FILE]),
            Some(FEED_INFO_FILE) => tables(&[FEED_INFO_FILE, TRANSLATIONS_FILE]),
            _ => NoticeDependencies::None,
        },
        "transfer_with_invalid_stop_location_type" => {
            if validator == "transfers_in_seat_transfer_type" {
                tables(&[STOPS_FILE, STOP_TIMES_FILE, TRANSFERS_FILE])
            } else {
                tables(&[STOPS_FILE, TRANSFERS_FILE])
            }
        }
        "translation_foreign_key_violation"
        | "translation_unexpected_value"
        | "translation_unknown_table_name" => NoticeDependencies::WholeFeed,
        // The canonical table container reports duplicate keys only for a
        // table it could build.
        "duplicate_key" => match notice_filename(notice).and_then(known_file) {
            Some(file) => tables(&[file]),
            None => NoticeDependencies::None,
        },
        _ => match single_validator_dependencies(code) {
            Some(list) => tables(list),
            None => NoticeDependencies::None,
        },
    }
}

/// Codes raised by exactly one canonical multi-file validator, with the tables
/// it is injected with.
fn single_validator_dependencies(code: &str) -> Option<&'static [&'static str]> {
    Some(match code {
        "big_gap_in_service" => &[CALENDAR_FILE, CALENDAR_DATES_FILE], // ServiceSpreadValidator
        "block_trips_with_overlapping_stop_times" => &[
            CALENDAR_FILE,
            CALENDAR_DATES_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // BlockTripsWithOverlappingStopTimesValidator
        "decreasing_or_equal_stop_time_distance" => &[STOP_TIMES_FILE], // StopTimeIncreasingDistanceValidator
        "decreasing_shape_distance" => &[SHAPES_FILE], // ShapeIncreasingDistanceValidator
        "duplicate_fare_media" => &[FARE_MEDIA_FILE],  // DuplicateFareMediaValidator
        "duplicate_geography_id" => &[LOCATIONS_GEOJSON_FILE, LOCATION_GROUPS_FILE, STOPS_FILE], // UniqueGeographyIdValidator
        "duplicate_route_name" => &[ROUTES_FILE], // DuplicateRouteNameValidator
        "equal_shape_distance_diff_coordinates" => &[SHAPES_FILE], // ShapeIncreasingDistanceValidator
        "equal_shape_distance_diff_coordinates_distance_below_threshold" => &[SHAPES_FILE], // ShapeIncreasingDistanceValidator
        "equal_shape_distance_same_coordinates" => &[SHAPES_FILE], // ShapeIncreasingDistanceValidator
        "expired_calendar" => &[CALENDAR_FILE, CALENDAR_DATES_FILE], // ExpiredCalendarValidator
        "fare_product_with_multiple_default_rider_categories" => {
            &[FARE_PRODUCTS_FILE, RIDER_CATEGORIES_FILE]
        } // FareProductDefaultRiderCategoriesValidator
        "fast_travel_between_consecutive_stops" => {
            &[ROUTES_FILE, STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE]
        } // StopTimeTravelSpeedValidator
        "fast_travel_between_far_stops" => &[ROUTES_FILE, STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE], // StopTimeTravelSpeedValidator
        "feed_info_lang_and_agency_lang_mismatch" => &[AGENCY_FILE, FEED_INFO_FILE], // MatchingFeedAndAgencyLangValidator
        "feed_valid_beyond_total_service_window" => &[
            CALENDAR_FILE,
            CALENDAR_DATES_FILE,
            FEED_INFO_FILE,
            TRIPS_FILE,
        ], // FeedServiceWindowValidator
        "forbidden_continuous_pickup_drop_off" => &[ROUTES_FILE, STOP_TIMES_FILE, TRIPS_FILE], // ContinuousPickupDropOffValidator
        "future_calendar" => &[
            CALENDAR_FILE,
            CALENDAR_DATES_FILE,
            FEED_INFO_FILE,
            TRIPS_FILE,
        ], // FeedServiceWindowValidator
        "future_feed" => &[FEED_INFO_FILE], // FeedValidTodayValidator
        "inconsistent_agency_lang" => &[AGENCY_FILE], // AgencyConsistencyValidator
        "inconsistent_agency_timezone" => &[AGENCY_FILE], // AgencyConsistencyValidator
        "inconsistent_route_type_for_block_id" => &[ROUTES_FILE, TRIPS_FILE], // InconsistentRouteTypeForBlockIdValidator
        "inconsistent_route_type_for_in_seat_transfer" => &[ROUTES_FILE, TRANSFERS_FILE], // InconsistentRouteTypeForInSeatTransferValidator
        "location_with_unexpected_stop_time" => {
            &[LOCATION_GROUP_STOPS_FILE, STOPS_FILE, STOP_TIMES_FILE]
        } // LocationHasStopTimesValidator
        "missing_bike_allowance" => &[ROUTES_FILE, TRIPS_FILE], // BikesAllowanceValidator
        "missing_calendar_and_calendar_date_files" => &[CALENDAR_FILE, CALENDAR_DATES_FILE], // MissingCalendarAndCalendarDateValidator
        "missing_level_id" => &[PATHWAYS_FILE, STOPS_FILE], // MissingLevelIdValidator
        "missing_pickup_drop_off_booking_rule_id" => &[BOOKING_RULES_FILE, STOP_TIMES_FILE], // PickupBookingRuleIdValidator
        "missing_recommended_file" => &[FEED_INFO_FILE, TRANSLATIONS_FILE], // MissingFeedInfoValidator
        "missing_stop_times_record" => &[STOP_TIMES_FILE], // StopTimesRecordValidator
        "missing_timepoint_value" => &[STOP_TIMES_FILE],   // TimepointTimeValidator
        "missing_trip_edge" => &[STOP_TIMES_FILE],         // MissingTripEdgeValidator
        "overlapping_frequency" => &[FREQUENCIES_FILE],    // OverlappingFrequencyValidator
        "overlapping_zone_and_pickup_drop_off_window" => &[LOCATIONS_GEOJSON_FILE, STOP_TIMES_FILE], // OverlappingPickupDropOffZoneValidator
        "pathway_dangling_generic_node" => &[PATHWAYS_FILE, STOPS_FILE], // PathwayDanglingGenericNodeValidator
        "pathway_to_platform_with_boarding_areas" => &[PATHWAYS_FILE, STOPS_FILE], // PathwayEndpointTypeValidator
        "pathway_to_stop_with_access_outside_of_station_pathways" => &[PATHWAYS_FILE, STOPS_FILE], // PathwayStopAccessValidator
        "pathway_to_wrong_location_type" => &[PATHWAYS_FILE, STOPS_FILE], // PathwayEndpointTypeValidator
        "pathway_unreachable_location" => &[PATHWAYS_FILE, STOPS_FILE], // PathwayReachableLocationValidator
        "route_networks_specified_in_more_than_one_file" => {
            &[NETWORKS_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE]
        } // NetworkIdConsistencyValidator
        "same_route_and_agency_url" => &[AGENCY_FILE, ROUTES_FILE, STOPS_FILE], // UrlConsistencyValidator
        "same_stop_and_agency_url" => &[AGENCY_FILE, ROUTES_FILE, STOPS_FILE], // UrlConsistencyValidator
        "same_stop_and_route_url" => &[AGENCY_FILE, ROUTES_FILE, STOPS_FILE], // UrlConsistencyValidator
        "service_extends_far_in_the_future" => &[CALENDAR_FILE, CALENDAR_DATES_FILE], // ServiceSpreadValidator
        "service_has_no_active_day_of_the_week" => &[CALENDAR_FILE], // ServiceHasNoActiveDayOfTheWeekValidator
        "service_window_outside_feed_period" => &[
            CALENDAR_FILE,
            CALENDAR_DATES_FILE,
            FEED_INFO_FILE,
            TRIPS_FILE,
        ], // FeedServiceWindowValidator
        "single_shape_point" => &[SHAPES_FILE],                      // SingleShapePointValidator
        "stop_has_too_many_matches_for_shape" => &[
            ROUTES_FILE,
            SHAPES_FILE,
            STOPS_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // ShapeToStopMatchingValidator
        "stop_time_timepoint_without_times" => &[STOP_TIMES_FILE],   // TimepointTimeValidator
        "stop_time_with_arrival_before_previous_departure_time" => &[STOP_TIMES_FILE], // StopTimeArrivalAndDepartureTimeValidator
        "stop_time_with_only_arrival_or_departure_time" => &[STOP_TIMES_FILE], // StopTimeArrivalAndDepartureTimeValidator
        "stop_too_far_from_shape" => &[
            ROUTES_FILE,
            SHAPES_FILE,
            STOPS_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // ShapeToStopMatchingValidator
        "stop_too_far_from_shape_using_user_distance" => &[
            ROUTES_FILE,
            SHAPES_FILE,
            STOPS_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // ShapeToStopMatchingValidator
        "stop_without_stop_time" => &[LOCATION_GROUP_STOPS_FILE, STOPS_FILE, STOP_TIMES_FILE], // LocationHasStopTimesValidator
        "stop_without_zone_id" => &[
            FARE_RULES_FILE,
            ROUTES_FILE,
            STOPS_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // StopZoneIdValidator
        "stops_match_shape_out_of_order" => &[
            ROUTES_FILE,
            SHAPES_FILE,
            STOPS_FILE,
            STOP_TIMES_FILE,
            TRIPS_FILE,
        ], // ShapeToStopMatchingValidator
        "timeframe_overlap" => &[TIMEFRAMES_FILE], // TimeframeOverlapValidator
        "transfer_distance_above_2_km" => &[STOPS_FILE, TRANSFERS_FILE], // TransferDistanceValidator
        "transfer_distance_too_large" => &[STOPS_FILE, TRANSFERS_FILE], // TransferDistanceValidator
        "transfer_with_invalid_trip_and_route" => {
            &[STOPS_FILE, STOP_TIMES_FILE, TRANSFERS_FILE, TRIPS_FILE]
        } // TransfersTripReferenceValidator
        "transfer_with_invalid_trip_and_stop" => {
            &[STOPS_FILE, STOP_TIMES_FILE, TRANSFERS_FILE, TRIPS_FILE]
        } // TransfersTripReferenceValidator
        "transfer_with_suspicious_mid_trip_in_seat" => {
            &[STOPS_FILE, STOP_TIMES_FILE, TRANSFERS_FILE]
        } // TransfersInSeatTransferTypeValidator
        "trip_coverage_not_active_for_next7_days" => &[
            CALENDAR_FILE,
            CALENDAR_DATES_FILE,
            FREQUENCIES_FILE,
            TRIPS_FILE,
        ], // DateTripsValidator
        "trip_distance_exceeds_shape_distance" => {
            &[SHAPES_FILE, STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE]
        } // TripAndShapeDistanceValidator
        "trip_distance_exceeds_shape_distance_below_threshold" => {
            &[SHAPES_FILE, STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE]
        } // TripAndShapeDistanceValidator
        "trip_headsign_matches_intermediate_stop" => &[STOPS_FILE, STOP_TIMES_FILE, TRIPS_FILE], // TripHeadsignValidator
        "trip_with_shape_dist_traveled_but_no_shape_distances" => {
            &[SHAPES_FILE, STOP_TIMES_FILE, TRIPS_FILE]
        } // TripWithShapeDistTraveledButNoShapeDistancesValidator
        "unsorted_stop_times" => &[STOP_TIMES_FILE], // StopTimesTripBlockOrderValidator
        "unusable_trip" => &[STOP_TIMES_FILE, TRIPS_FILE], // TripUsabilityValidator
        "unused_shape" => &[SHAPES_FILE, TRIPS_FILE], // ShapeUsageValidator
        "unused_station" => &[STOPS_FILE],           // ParentStationValidator
        "unused_trip" => &[STOP_TIMES_FILE, TRIPS_FILE], // TripUsageValidator
        "wrong_parent_location_type" => &[STOPS_FILE], // ParentStationValidator
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::LEVELS_FILE;
    use crate::NoticeSeverity;

    fn notice(code: &str, context: &[(&str, &str)]) -> ValidationNotice {
        let mut notice = ValidationNotice::new(code, NoticeSeverity::Error, "");
        for (key, value) in context {
            notice.insert_context_field(*key, *value);
        }
        notice
    }

    fn feed_with_failed(files: &[&'static str]) -> GtfsFeed {
        let mut feed = GtfsFeed::default();
        for file in files {
            feed.table_statuses.insert(file, TableStatus::ParseError);
        }
        feed
    }

    #[test]
    fn open_gate_keeps_everything() {
        let gate = DependencyGate::new(&feed_with_failed(&[]));
        assert!(gate.is_open());
        assert!(gate.allows("any", &notice("unused_station", &[])));
    }

    #[test]
    fn validator_notices_follow_their_tables() {
        let gate = DependencyGate::new(&feed_with_failed(&[STOPS_FILE]));
        assert!(!gate.allows("any", &notice("unused_station", &[])));
        assert!(!gate.allows("any", &notice("stops_match_shape_out_of_order", &[])));
        assert!(gate.allows("any", &notice("unsorted_stop_times", &[])));
        // Single-entity and loader notices are always kept.
        assert!(gate.allows("any", &notice("stop_without_location", &[])));
        assert!(gate.allows("any", &notice("number_out_of_range", &[])));
    }

    #[test]
    fn foreign_keys_depend_on_both_files() {
        let gate = DependencyGate::new(&feed_with_failed(&[CALENDAR_DATES_FILE]));
        let trip_service = notice(
            "foreign_key_violation",
            &[
                ("childFilename", "trips.txt"),
                ("parentFilename", "calendar.txt or calendar_dates.txt"),
            ],
        );
        assert!(!gate.allows("any", &trip_service));
        let trip_route = notice(
            "foreign_key_violation",
            &[
                ("childFilename", "trips.txt"),
                ("parentFilename", "routes.txt"),
            ],
        );
        assert!(gate.allows("any", &trip_route));
    }

    #[test]
    fn a_missing_required_file_fails_its_dependents() {
        let mut feed = GtfsFeed::default();
        feed.table_statuses
            .insert(TRIPS_FILE, TableStatus::MissingFile);
        feed.table_statuses
            .insert(SHAPES_FILE, TableStatus::MissingFile);
        let gate = DependencyGate::new(&feed);
        assert!(!gate.allows("any", &notice("unused_trip", &[])));
        assert!(gate.allows("any", &notice("unused_station", &[])));
        // A missing optional file is parsed successfully, and so is a missing
        // stops.txt (the canonical validator does not require it).
        let mut feed = GtfsFeed::default();
        feed.table_statuses
            .insert(SHAPES_FILE, TableStatus::MissingFile);
        feed.table_statuses
            .insert(STOPS_FILE, TableStatus::MissingFile);
        assert!(DependencyGate::new(&feed).is_open());
    }

    #[test]
    fn translation_checks_need_the_whole_feed() {
        let gate = DependencyGate::new(&feed_with_failed(&[LEVELS_FILE]));
        assert!(!gate.allows("any", &notice("translation_unknown_table_name", &[])));
        assert!(gate.allows("any", &notice("unused_station", &[])));
    }
}
