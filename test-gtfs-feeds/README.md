# GTFS Validator Test Feeds

This directory contains test GTFS feeds designed to trigger specific validation errors.
Each subdirectory represents a specific error case with all necessary GTFS files.

## Directory Structure

```
test-gtfs-feeds/
├── base-valid/           # Valid GTFS feed for reference
├── errors/               # Error test cases by category
│   ├── booking-rules/    # booking_rules.txt errors (GTFS-Flex)
│   ├── core-files/       # File-level errors
│   ├── fares/            # Fares-v2 errors
│   ├── field-validation/ # Field validation errors
│   ├── foreign-keys/     # Foreign key errors
│   ├── geojson/          # locations.geojson errors (GTFS-Flex)
│   ├── stops/            # stops.txt errors
│   ├── stop-times/       # stop_times.txt errors
│   ├── routes/           # routes.txt errors
│   ├── shapes/           # shapes.txt errors
│   ├── pathways/         # pathways.txt errors
│   ├── calendar-frequencies/ # calendar/frequencies errors
│   ├── timeframes/       # timeframes.txt errors (Fares-v2)
│   ├── flex/             # GTFS-Flex errors
│   ├── transfers/        # transfers.txt errors
│   ├── translations/     # translations.txt errors
│   └── trips/            # trips.txt errors
├── warnings/             # Warning test cases by category
│   ├── agency/           # agency.txt warnings
│   ├── core/             # Core/general warnings
│   ├── feed-info/        # feed_info.txt warnings
│   ├── fare-media/       # fare_media.txt warnings
│   ├── flex/             # GTFS-Flex warnings
│   ├── routes/           # routes.txt warnings
│   ├── stops/            # stops.txt warnings
│   ├── shapes/           # shapes.txt warnings
│   ├── pathways/         # pathways.txt warnings
│   ├── transfers/        # transfers.txt warnings
│   ├── calendar/         # calendar.txt warnings
│   ├── stop-times/       # stop_times.txt warnings
│   ├── trips/            # trips.txt warnings
│   ├── csv-parsing/      # CSV parsing warnings
│   ├── attributions/     # attributions.txt warnings
│   └── translations/     # translations.txt warnings
└── info/                 # Info test cases by category
    ├── geojson/          # locations.geojson info notices
    ├── stops/            # stops.txt info notices
    ├── transfers/        # transfers.txt info notices
    └── csv-parsing/      # CSV parsing info notices
```


## Test Cases

### Core File Errors (`errors/core-files/`)

| Error Code | Description |
|------------|-------------|
| `missing_required_file` | Required file (stops.txt) is missing |
| `empty_file` | Required file has headers but no data |
| `duplicated_column` | CSV header has duplicate column names |
| `empty_column_name` | CSV header has empty column name |
| `invalid_row_length` | Row has different number of values than header |
| `missing_required_column` | Required column (agency_name) is missing |
| `new_line_in_value` | Field value contains newline character |
| `duplicate_key` | Primary key (agency_id) is duplicated |
| `missing_required_field` | Required field value is empty |
| `missing_stop_times_record` | Trip defined but has no stop_times entries |
| `u_r_i_syntax_error` | URI has invalid syntax that cannot be parsed |
| `invalid_input_files_in_subfolder` | GTFS files located in subfolder instead of root |

### Field Validation Errors (`errors/field-validation/`)

| Error Code | Description |
|------------|-------------|
| `invalid_date` | Date not in YYYYMMDD format |
| `invalid_time` | Time not in H:MM:SS format |
| `invalid_color` | Color contains # symbol (should be RRGGBB only) |
| `invalid_url` | URL is malformed |
| `invalid_email` | Email address is malformed |
| `invalid_timezone` | Invalid IANA timezone |
| `invalid_language_code` | Invalid BCP 47 language code |
| `invalid_float` | Cannot parse as floating point |
| `invalid_integer` | Cannot parse as integer |
| `number_out_of_range` | Latitude > 90 degrees |
| `point_near_origin` | Coordinates near (0, 0) |
| `point_near_pole` | Coordinates near North/South Pole |
| `start_and_end_range_out_of_order` | Start date after end date |
| `start_and_end_range_equal` | Start time equals end time |

### Foreign Key Errors (`errors/foreign-keys/`)

| Error Code | Description |
|------------|-------------|
| `foreign_key_violation` | Trip references non-existent route_id |
| `inconsistent_agency_timezone` | Multiple agencies with different timezones |

### Stops Errors (`errors/stops/`)

| Error Code | Description |
|------------|-------------|
| `missing_stop_name` | Stop (location_type=0) has no stop_name |
| `location_without_parent_station` | Entrance (location_type=2) has no parent_station |
| `station_with_parent_station` | Station (location_type=1) has parent_station |
| `stop_without_location` | Stop missing stop_lat coordinate |
| `wrong_parent_location_type` | Stop's parent is another stop, not a station |
| `location_with_unexpected_stop_time` | stop_times references a station, not a stop |

### Stop Times Errors (`errors/stop-times/`)

| Error Code | Description |
|------------|-------------|
| `stop_time_with_arrival_before_previous_departure_time` | Arrival before previous departure |
| `stop_time_with_only_arrival_or_departure_time` | Only arrival_time provided, no departure_time |
| `stop_time_timepoint_without_times` | timepoint=1 but no times provided |
| `missing_trip_edge` | First stop missing arrival/departure time |
| `block_trips_with_overlapping_stop_times` | Same block_id with overlapping trips |
| `decreasing_or_equal_stop_time_distance` | shape_dist_traveled decreases |

### Routes Errors (`errors/routes/`)

| Error Code | Description |
|------------|-------------|
| `route_both_short_and_long_name_missing` | Both route names are empty |

### Shapes Errors (`errors/shapes/`)

| Error Code | Description |
|------------|-------------|
| `decreasing_shape_distance` | shape_dist_traveled decreases along shape |

### Pathways Errors (`errors/pathways/`)

| Error Code | Description |
|------------|-------------|
| `bidirectional_exit_gate` | Exit gate (pathway_mode=7) is bidirectional |
| `missing_level_id` | Pathway with stairs requires level_id on connected stops |
| `pathway_to_platform_with_boarding_areas` | Pathway leads to platform that has boarding areas |

### Calendar/Frequencies Errors (`errors/calendar-frequencies/`)

| Error Code | Description |
|------------|-------------|
| `missing_calendar_and_calendar_date_files` | Neither calendar.txt nor calendar_dates.txt present |
| `overlapping_frequency` | Same trip has overlapping frequency time ranges |

### Transfers Errors (`errors/transfers/`)

| Error Code | Description |
|------------|-------------|
| `transfer_with_invalid_stop_location_type` | Transfer references an entrance, not a stop |
| `transfer_with_invalid_trip_and_route` | Transfer references trip that does not belong to specified route |

### Booking Rules Errors (`errors/booking-rules/`)

| Error Code | Description |
|------------|-------------|
| `forbidden_prior_day_booking_field_value` | Prior-day booking has forbidden same-day fields |
| `forbidden_prior_notice_start_day` | prior_notice_start_day set with prior_notice_duration_max |
| `forbidden_prior_notice_start_time` | prior_notice_start_time set without prior_notice_start_day |
| `forbidden_real_time_booking_field_value` | Real-time booking has prior notice fields |
| `forbidden_same_day_booking_field_value` | Same-day booking has prior-day fields |
| `invalid_prior_notice_duration_min` | prior_notice_duration_min > prior_notice_duration_max |
| `missing_prior_notice_duration_min` | Same-day booking missing prior_notice_duration_min |
| `missing_prior_notice_last_day` | Prior-day booking missing prior_notice_last_day |
| `missing_prior_notice_last_time` | Prior-day booking missing prior_notice_last_time |
| `missing_prior_notice_start_time` | prior_notice_start_day set without prior_notice_start_time |
| `prior_notice_last_day_after_start_day` | prior_notice_last_day > prior_notice_start_day |

### GeoJSON Errors (`errors/geojson/`)

| Error Code | Description |
|------------|-------------|
| `duplicate_geo_json_key` | Duplicate feature ID in locations.geojson |
| `duplicate_geography_id` | GeoJSON ID conflicts with stops.stop_id |
| `geo_json_duplicated_element` | Duplicated elements in locations.geojson |
| `invalid_geometry` | Invalid/self-intersecting polygon |
| `malformed_json` | Malformed JSON syntax |
| `missing_required_element` | Missing required "features" array |
| `unsupported_feature_type` | Feature type is not "Feature" |
| `unsupported_geo_json_type` | Root type is not "FeatureCollection" |
| `unsupported_geometry_type` | Geometry type is not Polygon/MultiPolygon |

### Timeframe Errors (`errors/timeframes/`)

| Error Code | Description |
|------------|-------------|
| `timeframe_only_start_or_end_time_specified` | Only start_time or end_time provided |
| `timeframe_overlap` | Overlapping timeframes with same group/service |
| `timeframe_start_or_end_time_greater_than_twenty_four_hours` | Time exceeds 24:00:00 |

### Fare Transfer Rule Errors (`errors/fares/`)

| Error Code | Description |
|------------|-------------|
| `fare_product_with_multiple_default_rider_categories` | Multiple rider categories marked as default |
| `fare_transfer_rule_duration_limit_type_without_duration_limit` | duration_limit_type without duration_limit |
| `fare_transfer_rule_duration_limit_without_type` | duration_limit without duration_limit_type |
| `fare_transfer_rule_invalid_transfer_count` | Invalid transfer_count value |
| `fare_transfer_rule_missing_transfer_count` | Equal leg groups but missing transfer_count |
| `fare_transfer_rule_with_forbidden_transfer_count` | Different leg groups with transfer_count |

### Translation Errors (`errors/translations/`)

| Error Code | Description |
|------------|-------------|
| `translation_foreign_key_violation` | Translation references non-existent record_id |
| `translation_unexpected_value` | Both record_id and field_value provided |

### Trip Errors (`errors/trips/`)

| Error Code | Description |
|------------|-------------|
| `route_networks_specified_in_more_than_one_file` | network_id in routes.txt and route_networks.txt |
| `trip_distance_exceeds_shape_distance` | Trip stop distances exceed shape distance |

### GTFS-Flex Errors (`errors/flex/`)

| Error Code | Description |
|------------|-------------|
| `forbidden_arrival_or_departure_time` | arrival/departure_time forbidden with pickup/drop-off windows |
| `forbidden_continuous_pickup_drop_off` | continuous_pickup/drop_off forbidden for flex trips |
| `forbidden_drop_off_type` | drop_off_type forbidden with pickup/drop-off windows |
| `forbidden_geography_id` | location_id forbidden when stop_id is defined |
| `forbidden_pickup_type` | pickup_type forbidden with pickup/drop-off windows |
| `forbidden_shape_dist_traveled` | shape_dist_traveled forbidden with pickup/drop-off windows |
| `invalid_pickup_drop_off_window` | end_pickup_drop_off_window before start |
| `missing_pickup_or_drop_off_window` | Only one of start/end pickup_drop_off_window defined |
| `overlapping_zone_and_pickup_drop_off_window` | Same zone has overlapping windows in same trip |

## Warning Test Cases

### Agency Warnings (`warnings/agency/`)

| Warning Code | Description |
|--------------|-------------|
| `inconsistent_agency_lang` | Multiple agencies have different agency_lang values |

### Core Warnings (`warnings/core/`)

| Warning Code | Description |
|--------------|-------------|
| `missing_recommended_field` | Recommended field is present but empty |
| `missing_recommended_file` | Recommended file (feed_info.txt) is missing |
| `mixed_case_recommended_field` | Field uses mixed/upper case when lowercase is recommended |
| `unexpected_enum_value` | Enum field has value not defined in specification |

### Fare Media Warnings (`warnings/fare-media/`)

| Warning Code | Description |
|--------------|-------------|
| `duplicate_fare_media` | Two fare media have same name and type |

### Feed Info Warnings (`warnings/feed-info/`)


| Warning Code | Description |
|--------------|-------------|
| `feed_expiration_date7_days` | Dataset should be valid for at least the next 7 days |
| `feed_expiration_date30_days` | Dataset should cover at least the next 30 days |
| `feed_info_lang_and_agency_lang_mismatch` | Feed and agency language fields should align |
| `missing_feed_contact_email_and_url` | Best practices recommend providing contact methods |
| `missing_feed_info_date` | Both feed start and end dates should be specified together |
| `more_than_one_entity` | Single-entity files like feed_info.txt contain multiple rows |

### Routes Warnings (`warnings/routes/`)

| Warning Code | Description |
|--------------|-------------|
| `duplicate_route_name` | Routes with identical short/long names need differentiation |
| `route_color_contrast` | Route colors lack sufficient contrast with text colors |
| `route_long_name_contains_short_name` | Long names should not duplicate short name content |
| `route_short_name_too_long` | Short name exceeds 12 character limit |
| `same_name_and_description_for_route` | Route descriptions should provide distinct information |
| `same_route_and_agency_url` | Route URLs should differ from agency URLs |

### Stops Warnings (`warnings/stops/`)

| Warning Code | Description |
|--------------|-------------|
| `same_name_and_description_for_stop` | Stop descriptions should offer unique details beyond stop names |
| `same_stop_and_agency_url` | Stop URLs should differ from agency URLs |
| `same_stop_and_route_url` | Stop URLs should differ from route URLs |
| `stop_without_stop_time` | Stops defined but never used in trips |

### Shapes Warnings (`warnings/shapes/`)

| Warning Code | Description |
|--------------|-------------|
| `equal_shape_distance_diff_coordinates_distance_below_threshold` | Two shape points have equal distance but different coordinates (below threshold) |
| `equal_shape_distance_same_coordinates` | Duplicate shape points with identical coordinates and distance |
| `single_shape_point` | Shapes require multiple points for proper visualization |
| `stop_has_too_many_matches_for_shape` | Stop matches multiple shape points within distance threshold |
| `stop_too_far_from_shape` | Stops should be within 100 meters of shape definitions |
| `stops_match_shape_out_of_order` | Stops match shape points but in wrong order |
| `trip_distance_exceeds_shape_distance_below_threshold` | Trip distance exceeds shape distance (within threshold) |
| `unused_shape` | Shape definitions exist but aren't referenced in trips |

### Pathways Warnings (`warnings/pathways/`)

| Warning Code | Description |
|--------------|-------------|
| `pathway_dangling_generic_node` | Generic nodes with only one pathway connection serve no purpose |
| `pathway_loop` | Pathways should not share identical start and end locations |

### Transfers Warnings (`warnings/transfers/`)

| Warning Code | Description |
|--------------|-------------|
| `transfer_distance_too_large` | Transfer distances exceed 10 km threshold |
| `transfer_with_suspicious_mid_trip_in_seat` | In-seat transfer at stop that is not first/last stop |

### Calendar Warnings (`warnings/calendar/`)

| Warning Code | Description |
|--------------|-------------|
| `expired_calendar` | Datasets should exclude already-expired service date ranges |

### Stop Times Warnings (`warnings/stop-times/`)

| Warning Code | Description |
|--------------|-------------|
| `fast_travel_between_consecutive_stops` | Transit vehicles exceeding speed thresholds |
| `fast_travel_between_far_stops` | Vehicle travels unrealistically fast between far stops |
| `missing_timepoint_value` | When times exist, stop_times.timepoint should be defined |

### Trips Warnings (`warnings/trips/`)

| Warning Code | Description |
|--------------|-------------|
| `missing_bike_allowance` | trips.txt has bikes_allowed column but value is empty |
| `unusable_trip` | Trips must have more than one stop to be usable |
| `unused_trip` | Trip definitions lack any stop_times references |

### CSV Parsing Warnings (`warnings/csv-parsing/`)

| Warning Code | Description |
|--------------|-------------|
| `empty_row` | CSV rows containing only spaces may be misinterpreted |
| `leading_or_trailing_whitespaces` | CSV values should not have extraneous spacing |
| `non_ascii_or_non_printable_char` | ID fields contain non-ASCII or unprintable characters |

### Attributions Warnings (`warnings/attributions/`)

| Warning Code | Description |
|--------------|-------------|
| `attribution_without_role` | At least one of is_producer, is_operator, or is_authority should be set |

### Translations Warnings (`warnings/translations/`)

| Warning Code | Description |
|--------------|-------------|
| `translation_unknown_table_name` | Translations reference non-existent GTFS tables |

### GTFS-Flex Warnings (`warnings/flex/`)

| Warning Code | Description |
|--------------|-------------|
| `missing_pickup_drop_off_booking_rule_id` | Flex trip with pickup/drop-off windows missing booking_rule_id |

## Info Test Cases

### GeoJSON Info (`info/geojson/`)

| Info Code | Description |
|-----------|-------------|
| `geo_json_unknown_element` | Unknown elements in locations.geojson file |

### Stops Info (`info/stops/`)

| Info Code | Description |
|-----------|-------------|
| `platform_without_parent_station` | Platform (location_type=4) has no parent_station |
| `stop_without_zone_id` | Stop missing zone_id in route with zone-dependent fare rules |
| `unused_station` | Station (location_type=1) not referenced as parent_station by any stops |

### Transfers Info (`info/transfers/`)

| Info Code | Description |
|-----------|-------------|
| `transfer_distance_above_2_km` | Transfer distance between stops exceeds 2 km |

### CSV Parsing Info (`info/csv-parsing/`)

| Info Code | Description |
|-----------|-------------|
| `unknown_column` | A column name is not defined in the GTFS specification |
| `unknown_file` | A file is not defined in the GTFS specification |

## Usage

To test a specific error case:

1. Navigate to the error directory
2. Create a ZIP archive of all files in that directory
3. Run the GTFS validator on the ZIP file

```bash
# Example: Test missing_required_file error
cd test-gtfs-feeds/errors/core-files/missing_required_file
zip -r test.zip *.txt
java -jar gtfs-validator.jar -i test.zip -o output
```

## Adding New Test Cases

1. Create a new directory under the appropriate category
2. Add all required GTFS files (agency.txt, stops.txt, routes.txt, trips.txt, stop_times.txt, calendar.txt)
3. Add a README.txt explaining the error being tested
4. Ensure only the specific error you want to test is present

## Reference

Full list of validation rules: https://gtfs-validator.mobilitydata.org/rules.html
