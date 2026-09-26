use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use csv::StringRecord;
use url::Url;

use crate::csv_schema::schema_for_file;
use crate::feed::FARE_PRODUCTS_FILE;
use crate::fix_suggest;
use crate::notice::FixSafety;
use crate::validation_context::{thorough_mode_enabled, validation_country_code};
use crate::{NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_guru_model::{GtfsColor, GtfsDate, GtfsTime};

const MAX_ROW_NUMBER: u64 = 1_000_000_000;

/// Columns the canonical schema marks `@Recommended`: its row parser reports
/// `missing_recommended_field` for each row without a value, errors or not.
const LOADER_RECOMMENDED_FIELDS: &[(&str, &[&str])] = &[(
    "feed_info.txt",
    &["feed_start_date", "feed_end_date", "feed_version"],
)];

const MIXED_CASE_FIELDS: &[&str] = &[
    "agency_name",
    "drop_off_message",
    "level_name",
    "location_group_name",
    "message",
    "network_name",
    "pickup_message",
    "reversed_signposted_as",
    "route_desc",
    "route_long_name",
    "route_short_name",
    "signposted_as",
    "stop_name",
    "trip_headsign",
    "trip_short_name",
];

const FLOAT_FIELDS: &[&str] = &[
    "amount",
    "length",
    "level_index",
    "max_slope",
    "min_width",
    "price",
    "shape_dist_traveled",
    "shape_pt_lat",
    "shape_pt_lon",
    "safe_duration_factor",
    "safe_duration_offset",
    "stop_lat",
    "stop_lon",
];
const LATITUDE_FIELDS: &[&str] = &["shape_pt_lat", "stop_lat"];
const LONGITUDE_FIELDS: &[&str] = &["shape_pt_lon", "stop_lon"];
const LATITUDE_FIELD_TYPE: &str = "latitude within [-90, 90]";
const LONGITUDE_FIELD_TYPE: &str = "longitude within [-180, 180]";

const INTEGER_FIELDS: &[&str] = &[
    "duration_limit",
    "headway_secs",
    "min_transfer_time",
    "prior_notice_duration_max",
    "prior_notice_duration_min",
    "prior_notice_last_day",
    "prior_notice_start_day",
    "route_sort_order",
    "rule_priority",
    "shape_pt_sequence",
    "stair_count",
    "stop_sequence",
    "transfer_count",
    "transfer_duration",
    "traversal_time",
];

const NON_NEGATIVE_INTEGER_FIELDS: &[&str] = &[
    "min_transfer_time",
    "route_sort_order",
    "rule_priority",
    "shape_pt_sequence",
    "stop_sequence",
    "transfer_duration",
];
const POSITIVE_INTEGER_FIELDS: &[&str] = &["duration_limit", "headway_secs", "traversal_time"];
const NON_ZERO_INTEGER_FIELDS: &[&str] = &["stair_count"];

const NON_NEGATIVE_FLOAT_FIELDS: &[&str] = &["length", "shape_dist_traveled"];
const POSITIVE_FLOAT_FIELDS: &[&str] = &["min_width"];

const NON_NEGATIVE_DECIMAL_FIELDS: &[&str] = &["amount", "price"];

const DATE_FIELDS: &[&str] = &[
    "date",
    "end_date",
    "feed_end_date",
    "feed_start_date",
    "start_date",
];

const TIME_FIELDS: &[&str] = &[
    "arrival_time",
    "departure_time",
    "end_pickup_drop_off_window",
    "end_time",
    "prior_notice_last_time",
    "prior_notice_start_time",
    "start_pickup_drop_off_window",
    "start_time",
];

const COLOR_FIELDS: &[&str] = &["route_color", "route_text_color"];

const TIMEZONE_FIELDS: &[&str] = &["agency_timezone", "stop_timezone"];

const LANGUAGE_FIELDS: &[&str] = &["agency_lang", "feed_lang", "language"];

const CURRENCY_FIELDS: &[&str] = &["currency", "currency_type"];

const URL_FIELDS: &[&str] = &[
    "agency_fare_url",
    "agency_url",
    "attribution_url",
    "booking_url",
    "eligibility_url",
    "feed_contact_url",
    "feed_publisher_url",
    "info_url",
    "route_branding_url",
    "route_url",
    "stop_url",
];

const EMAIL_FIELDS: &[&str] = &["agency_email", "attribution_email", "feed_contact_email"];

const PHONE_FIELDS: &[&str] = &[
    "agency_phone",
    "attribution_phone",
    "phone_number",
    "stop_phone",
];

const CURRENCY_CODES: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD",
    "CAD", "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUC", "CUP",
    "CVE", "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP",
    "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HRK", "HTG", "HUF", "IDR",
    "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW",
    "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA",
    "MKD", "MMK", "MNT", "MOP", "MRO", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD",
    "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG",
    "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLL",
    "SOS", "SRD", "SSP", "STD", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY",
    "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UZS", "VEF", "VND", "VUV",
    "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XDR", "XOF", "XPD", "XPF",
    "XPT", "XSU", "XTS", "XUA", "XXX", "YER", "ZAR", "ZMW", "ZWL",
];

const CURRENCY_ZERO_DECIMALS: &[&str] = &[
    "ADP", "AFN", "ALL", "BIF", "BYR", "CLP", "DJF", "ESP", "GNF", "IQD", "IRR", "ISK", "ITL",
    "JPY", "KMF", "KPW", "KRW", "LAK", "LBP", "LUF", "MGA", "MGF", "MMK", "MRO", "PYG", "RSD",
    "RWF", "SLL", "SOS", "STD", "SYP", "TMM", "TRL", "UGX", "UYI", "VND", "VUV", "XAF", "XOF",
    "XPF", "YER", "ZMK", "ZWD",
];

const CURRENCY_THREE_DECIMALS: &[&str] = &["BHD", "JOD", "KWD", "LYD", "OMR", "TND"];

const CURRENCY_FOUR_DECIMALS: &[&str] = &["CLF", "UYW"];

#[derive(Debug, Clone, Copy)]
enum NumberBounds {
    Positive,
    NonNegative,
    NonZero,
}

#[derive(Debug, Clone, Copy)]
enum NumberKind {
    Integer,
    Float,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnumKind {
    LocationType,
    WheelchairBoarding,
    RouteType,
    ContinuousPickupDropOff,
    PickupDropOffType,
    BookingType,
    DirectionId,
    WheelchairAccessible,
    BikesAllowed,
    CarsAllowed,
    ContactlessEmvSupport,
    StopAccess,
    ServiceAvailability,
    ExceptionType,
    PaymentMethod,
    Transfers,
    ExactTimes,
    TransferType,
    PathwayMode,
    Bidirectional,
    YesNo,
    Timepoint,
    FareMediaType,
    DurationLimitType,
    FareTransferType,
    RiderFareCategory,
}

#[cfg(test)]
impl EnumKind {
    /// Every variant, in declaration order. Kept honest by `position_in_all`.
    const ALL: &'static [EnumKind] = &[
        EnumKind::LocationType,
        EnumKind::WheelchairBoarding,
        EnumKind::RouteType,
        EnumKind::ContinuousPickupDropOff,
        EnumKind::PickupDropOffType,
        EnumKind::BookingType,
        EnumKind::DirectionId,
        EnumKind::WheelchairAccessible,
        EnumKind::BikesAllowed,
        EnumKind::CarsAllowed,
        EnumKind::ContactlessEmvSupport,
        EnumKind::StopAccess,
        EnumKind::ServiceAvailability,
        EnumKind::ExceptionType,
        EnumKind::PaymentMethod,
        EnumKind::Transfers,
        EnumKind::ExactTimes,
        EnumKind::TransferType,
        EnumKind::PathwayMode,
        EnumKind::Bidirectional,
        EnumKind::YesNo,
        EnumKind::Timepoint,
        EnumKind::FareMediaType,
        EnumKind::DurationLimitType,
        EnumKind::FareTransferType,
        EnumKind::RiderFareCategory,
    ];

    /// This variant's index in [`EnumKind::ALL`].
    ///
    /// The `match` is exhaustive, so a variant added to the enum but not to
    /// `ALL` fails to compile here rather than quietly escaping
    /// `enum_allowed_values_agree_with_the_cell_check`.
    fn position_in_all(self) -> usize {
        match self {
            EnumKind::LocationType => 0,
            EnumKind::WheelchairBoarding => 1,
            EnumKind::RouteType => 2,
            EnumKind::ContinuousPickupDropOff => 3,
            EnumKind::PickupDropOffType => 4,
            EnumKind::BookingType => 5,
            EnumKind::DirectionId => 6,
            EnumKind::WheelchairAccessible => 7,
            EnumKind::BikesAllowed => 8,
            EnumKind::CarsAllowed => 9,
            EnumKind::ContactlessEmvSupport => 10,
            EnumKind::StopAccess => 11,
            EnumKind::ServiceAvailability => 12,
            EnumKind::ExceptionType => 13,
            EnumKind::PaymentMethod => 14,
            EnumKind::Transfers => 15,
            EnumKind::ExactTimes => 16,
            EnumKind::TransferType => 17,
            EnumKind::PathwayMode => 18,
            EnumKind::Bidirectional => 19,
            EnumKind::YesNo => 20,
            EnumKind::Timepoint => 21,
            EnumKind::FareMediaType => 22,
            EnumKind::DurationLimitType => 23,
            EnumKind::FareTransferType => 24,
            EnumKind::RiderFareCategory => 25,
        }
    }
}

/// Which bounds-checked numeric interpretation a float column carries.
///
/// Resolved once per column so the per-cell path never re-derives it from the
/// header name.
#[derive(Debug, Clone, Copy)]
enum FloatCheck {
    Latitude,
    Longitude,
    Decimal(NumberBounds),
    Float(NumberBounds),
    Plain,
}

/// The single value check a column is subject to.
///
/// The variants mirror the order of the `is_*_field` chain the per-cell loop
/// used to walk: a column matches at most one of those predicates, so resolving
/// the first match up front is equivalent to re-testing them on every row.
#[derive(Debug, Clone, Copy)]
enum ValueCheck {
    None,
    Enum(EnumKind),
    Integer(Option<NumberBounds>),
    Float(FloatCheck),
    Date,
    Time,
    Color,
    Timezone,
    Language,
    Currency,
    Url,
    Email,
    Phone,
}

/// Everything `validate_row` needs to know about one column, resolved once when
/// the validator is built.
///
/// Deriving this per cell meant a linear scan of a dozen `&[&str]` tables for
/// every field of every row — on a feed with millions of `stop_times` rows that
/// dominated parsing. The plan turns it into an indexed lookup plus one match.
struct ColumnPlan {
    /// Trimmed header name, as it appears in notices.
    header: Box<str>,
    is_schema_field: bool,
    /// `is_schema_field` and an id-shaped column: candidates for the ASCII check.
    check_non_ascii: bool,
    is_mixed_case: bool,
    check: ValueCheck,
}

fn value_check_for(field: &str) -> ValueCheck {
    if let Some(kind) = enum_kind(field) {
        return ValueCheck::Enum(kind);
    }
    if is_integer_field(field) {
        return ValueCheck::Integer(integer_bounds(field));
    }
    if is_float_field(field) {
        let kind = if is_latitude_field(field) {
            FloatCheck::Latitude
        } else if is_longitude_field(field) {
            FloatCheck::Longitude
        } else if let Some(bounds) = decimal_bounds(field) {
            FloatCheck::Decimal(bounds)
        } else if let Some(bounds) = float_bounds(field) {
            FloatCheck::Float(bounds)
        } else {
            FloatCheck::Plain
        };
        return ValueCheck::Float(kind);
    }
    if is_date_field(field) {
        return ValueCheck::Date;
    }
    if is_time_field(field) {
        return ValueCheck::Time;
    }
    if is_color_field(field) {
        return ValueCheck::Color;
    }
    if is_timezone_field(field) {
        return ValueCheck::Timezone;
    }
    if is_language_field(field) {
        return ValueCheck::Language;
    }
    if is_currency_field(field) {
        return ValueCheck::Currency;
    }
    if is_url_field(field) {
        return ValueCheck::Url;
    }
    if is_email_field(field) {
        return ValueCheck::Email;
    }
    if is_phone_field(field) {
        return ValueCheck::Phone;
    }
    ValueCheck::None
}

pub struct RowValidator {
    pub file_name: String,
    pub header_index: HashMap<String, usize>,
    pub validate_phone_numbers: bool,
    columns: Vec<ColumnPlan>,
    /// Column indexes of the schema's required fields, in schema order.
    required_columns: Vec<usize>,
    /// Fields the canonical loader checks for a value on every row, with
    /// their column (`None` when the header lacks it: still missing).
    recommended_columns: Vec<(&'static str, Option<usize>)>,
    is_fare_products: bool,
    thorough: bool,
}

impl RowValidator {
    pub fn new(file_name: &str, headers: Vec<String>) -> Self {
        // Column names match the schema exactly, as in the canonical
        // validator: `Stop_Lat` is an unknown column, and its values are not
        // checked as latitudes.
        let normalized_headers: Vec<String> = headers
            .iter()
            .map(|value| trim_java_whitespace(value).to_string())
            .collect();
        let header_index: HashMap<String, usize> = normalized_headers
            .iter()
            .enumerate()
            .map(|(index, value)| (value.clone(), index))
            .collect();
        let schema = schema_for_file(file_name);
        let validate_phone_numbers = validation_country_code().is_some();

        let columns: Vec<ColumnPlan> = headers
            .iter()
            .zip(normalized_headers.iter())
            .map(|(raw, normalized)| {
                let normalized = normalized.as_str();
                let is_schema_field = schema
                    .map(|schema| schema.fields.contains(&normalized))
                    .unwrap_or(false);
                ColumnPlan {
                    header: trim_java_whitespace(raw).into(),
                    is_schema_field,
                    check_non_ascii: is_schema_field && is_id_field(file_name, normalized),
                    is_mixed_case: is_mixed_case_field(normalized),
                    check: value_check_for(normalized),
                }
            })
            .collect();

        let required_columns = schema
            .map(|schema| {
                schema
                    .required_fields
                    .iter()
                    .filter_map(|required| header_index.get(*required).copied())
                    .collect()
            })
            .unwrap_or_default();

        let recommended_columns = LOADER_RECOMMENDED_FIELDS
            .iter()
            .filter(|(file, _)| file.eq_ignore_ascii_case(file_name))
            .flat_map(|(_, fields)| fields.iter())
            .map(|field| (*field, header_index.get(*field).copied()))
            .collect();

        Self {
            file_name: file_name.to_string(),
            recommended_columns,
            header_index,
            validate_phone_numbers,
            columns,
            required_columns,
            is_fare_products: file_name.eq_ignore_ascii_case(FARE_PRODUCTS_FILE),
            thorough: thorough_mode_enabled(),
        }
    }

    pub fn validate_row(&self, record: &StringRecord, row_number: u64) -> Vec<ValidationNotice> {
        let mut notices = Vec::new();
        let header_len = self.columns.len();

        if row_number > MAX_ROW_NUMBER {
            notices.push(too_many_rows_notice(&self.file_name, row_number));
            return notices;
        }

        // A one-field row with nothing in it: what univocity makes of a final
        // whitespace-only line, or of a line holding only `""`. The canonical
        // validator warns and does not load it.
        if record.len() == 1 && record.get(0).is_some_and(str::is_empty) {
            notices.push(empty_row_notice(&self.file_name, row_number));
            return notices;
        }

        // A row of empty values is a row like any other (each empty required
        // field is reported); `--thorough` also flags it as empty.
        if self.thorough
            && record.len() > 1
            && record
                .iter()
                .all(|value| trim_java_whitespace(value).is_empty())
        {
            notices.push(empty_row_notice(&self.file_name, row_number));
        }

        if record.len() != header_len {
            notices.push(invalid_row_length_notice(
                &self.file_name,
                row_number,
                header_len,
                record.len(),
            ));
            return notices;
        }

        // A value is missing when univocity hands over nothing; a quoted
        // `" "` is present, and fails its type check once trimmed instead.
        for &index in &self.required_columns {
            let raw = record.get(index).unwrap_or("");
            if raw.is_empty() {
                notices.push(missing_required_field_notice(
                    &self.file_name,
                    &self.columns[index].header,
                    row_number,
                ));
            }
        }
        for (field, index) in &self.recommended_columns {
            let raw = index.and_then(|index| record.get(index)).unwrap_or("");
            if raw.is_empty() {
                notices.push(missing_recommended_field_notice(
                    &self.file_name,
                    field,
                    row_number,
                ));
            }
        }

        // Java raises these from generated single-entity validators, which
        // only run on rows that parsed without an error, so they are held back
        // until the row's field checks are done.
        let mut entity_level = Vec::new();
        if self.is_fare_products {
            validate_currency_amount(
                &self.file_name,
                record,
                &self.header_index,
                row_number,
                &mut entity_level,
            );
        }

        // `record.len() == self.columns.len()` was checked above, so the zip
        // covers every field.
        for (plan, value) in self.columns.iter().zip(record.iter()) {
            let header_name: &str = &plan.header;
            let is_schema_field = plan.is_schema_field;
            if is_schema_field && (value.contains('\n') || value.contains('\r')) {
                notices.push(new_line_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }
            if is_schema_field && value.contains('\u{FFFD}') {
                notices.push(invalid_character_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }
            let trimmed = trim_java_whitespace(value);

            // The canonical validator trims every declared field and warns when
            // trimming changed it, so the notice has to be raised before the
            // empty check below: a field of only spaces trims down to nothing
            // and still counts.
            //
            // It only ever sees whitespace that sat *inside* quotes, because its
            // CSV parser strips whitespace around unquoted fields before
            // validation runs. Every reader applies the same pass
            // (`csv_univocity`) before the csv crate, so whatever whitespace is
            // left here was quoted in the source and the check matches Java.
            if is_schema_field && trimmed.len() < value.len() {
                notices.push(leading_or_trailing_whitespaces_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }

            if value.is_empty() {
                continue;
            }

            if plan.check_non_ascii && !has_only_printable_ascii(trimmed) {
                notices.push(non_ascii_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }

            if plan.is_mixed_case && is_mixed_case_violation(trimmed) {
                entity_level.push(mixed_case_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }

            match plan.check {
                ValueCheck::None => {}
                ValueCheck::Enum(kind) => {
                    validate_enum_value(
                        &self.file_name,
                        header_name,
                        row_number,
                        trimmed,
                        kind,
                        &mut notices,
                    );
                }
                // Java's `Integer.parseInt`: 32 bits, an optional sign.
                ValueCheck::Integer(bounds) => match trimmed.parse::<i32>().map(i64::from) {
                    Ok(value) => {
                        if let Some(bounds) = bounds {
                            if violates_bounds_i64(value, bounds) {
                                notices.push(number_out_of_range_notice_int(
                                    &self.file_name,
                                    header_name,
                                    row_number,
                                    bounds_field_type(bounds, NumberKind::Integer),
                                    value,
                                ));
                            }
                        }
                    }
                    Err(_) => {
                        notices.push(invalid_integer_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                },
                ValueCheck::Float(kind) => match trimmed.parse::<f64>() {
                    Ok(value) => {
                        let out_of_range =
                            match kind {
                                FloatCheck::Latitude => (!(-90.0..=90.0).contains(&value))
                                    .then_some(LATITUDE_FIELD_TYPE),
                                FloatCheck::Longitude => (!(-180.0..=180.0).contains(&value))
                                    .then_some(LONGITUDE_FIELD_TYPE),
                                FloatCheck::Decimal(bounds) => violates_bounds_f64(value, bounds)
                                    .then(|| bounds_field_type(bounds, NumberKind::Decimal)),
                                FloatCheck::Float(bounds) => violates_bounds_f64(value, bounds)
                                    .then(|| bounds_field_type(bounds, NumberKind::Float)),
                                FloatCheck::Plain => None,
                            };
                        if let Some(field_type) = out_of_range {
                            notices.push(number_out_of_range_notice(
                                &self.file_name,
                                header_name,
                                row_number,
                                field_type,
                                value,
                            ));
                        }
                    }
                    Err(_) => {
                        notices.push(invalid_float_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                },
                ValueCheck::Date => {
                    if GtfsDate::parse(trimmed).is_err() {
                        notices.push(invalid_date_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Time => {
                    if GtfsTime::parse(trimmed).is_err() {
                        notices.push(invalid_time_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Color => {
                    if GtfsColor::parse(trimmed).is_err() {
                        notices.push(invalid_color_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Timezone => {
                    if !is_valid_timezone(trimmed) {
                        notices.push(invalid_timezone_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Language => {
                    if self.thorough && !trimmed.is_empty() && !is_valid_language_code(trimmed) {
                        notices.push(invalid_language_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Currency => {
                    if !is_valid_currency_code(trimmed) {
                        notices.push(invalid_currency_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Url => {
                    if !is_valid_url(trimmed) {
                        notices.push(invalid_url_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                        if self.thorough
                            && crate::rules::url_syntax::checks_field(&self.file_name, header_name)
                        {
                            notices.extend(crate::rules::url_syntax::uri_syntax_error_notice(
                                trimmed,
                                &self.file_name,
                                header_name,
                                row_number,
                            ));
                        }
                    }
                }
                ValueCheck::Email => {
                    if !is_valid_email(trimmed) {
                        notices.push(invalid_email_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
                ValueCheck::Phone => {
                    if self.validate_phone_numbers && !is_valid_phone_number(trimmed) {
                        notices.push(invalid_phone_notice(
                            &self.file_name,
                            header_name,
                            row_number,
                            trimmed,
                        ));
                    }
                }
            }
        }
        if !notices
            .iter()
            .any(|notice| notice.severity == NoticeSeverity::Error)
        {
            notices.extend(entity_level);
        }
        notices
    }
}

/// Header and row notices for one table, read as the loader reads it. Only
/// the unit tests use it; the loader goes through `csv_reader::load_table`.
#[cfg(test)]
pub fn validate_csv_data(file_name: &str, data: &[u8], notices: &mut NoticeContainer) {
    let _table: crate::CsvTable<IgnoredRow> =
        crate::csv_reader::load_table(data, file_name, notices, &crate::StringPool::new())
            .expect("in-memory read");
}

#[cfg(test)]
#[derive(serde::Deserialize)]
struct IgnoredRow {}

pub fn validate_headers(file_name: &str, headers: &[String], notices: &mut NoticeContainer) {
    let schema = schema_for_file(file_name);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut normalized_headers: Vec<String> = Vec::with_capacity(headers.len());
    for (index, header) in headers.iter().enumerate() {
        let column = trim_header_name(header);
        normalized_headers.push(column.to_string());
        let column_index = index + 1;
        if column.is_empty() {
            notices.push(empty_column_name_notice(file_name, column_index));
            continue;
        }
        if let Some(first_index) = seen.get(column) {
            notices.push(duplicated_column_notice(
                file_name,
                column,
                *first_index,
                column_index,
            ));
        } else {
            seen.insert(column.to_string(), column_index);
        }
        if let Some(schema) = schema {
            if !schema.fields.contains(&column) {
                notices.push(unknown_column_notice(file_name, column, column_index));
            }
        }
    }
    if let Some(schema) = schema {
        let thorough = thorough_mode_enabled();
        let header_set: HashSet<&str> = normalized_headers
            .iter()
            .map(|value| value.as_str())
            .collect();
        // The canonical validator walks the missing columns in a `TreeSet`.
        let mut required: Vec<&str> = schema.required_fields.to_vec();
        required.sort_unstable();
        for required in required {
            if !header_set.contains(required) {
                notices.push(missing_required_column_notice(file_name, required));
            }
        }
        if thorough {
            for recommended in schema.recommended_fields {
                if !header_set.contains(recommended) {
                    notices.push(missing_recommended_column_notice(file_name, recommended));
                }
            }
        }
    }
}

fn trim_header_name(value: &str) -> &str {
    trim_java_whitespace(value)
}

fn empty_column_name_notice(file: &str, index: usize) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "empty_column_name",
        NoticeSeverity::Error,
        "column name is empty",
    );
    notice.insert_context_field("filename", file);
    notice.insert_context_field("index", index);
    notice.field_order = vec!["filename".into(), "index".into()];
    notice
}

fn duplicated_column_notice(
    file: &str,
    field_name: &str,
    first_index: usize,
    second_index: usize,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "duplicated_column",
        NoticeSeverity::Error,
        "duplicated column name",
    );
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.insert_context_field("firstIndex", first_index);
    notice.insert_context_field("secondIndex", second_index);
    notice.field_order = vec![
        "fieldName".into(),
        "filename".into(),
        "firstIndex".into(),
        "secondIndex".into(),
    ];
    notice
}

fn unknown_column_notice(file: &str, field_name: &str, index: usize) -> ValidationNotice {
    let mut notice =
        ValidationNotice::new("unknown_column", NoticeSeverity::Info, "unknown column");
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.insert_context_field("index", index);
    notice.field_order = vec!["fieldName".into(), "filename".into(), "index".into()];
    notice
}

fn missing_required_column_notice(file: &str, field_name: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_required_column",
        NoticeSeverity::Error,
        "required column is missing",
    );
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.field_order = vec!["fieldName".into(), "filename".into()];
    notice
}

fn missing_recommended_column_notice(file: &str, field_name: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_recommended_column",
        NoticeSeverity::Warning,
        "recommended column is missing",
    );
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.field_order = vec!["fieldName".into(), "filename".into()];
    notice
}

#[allow(dead_code)]
pub fn empty_row_notice(file: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new("empty_row", NoticeSeverity::Warning, "row is empty");
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("filename", file);
    notice.field_order = vec!["csvRowNumber".into(), "filename".into()];
    notice
}

fn invalid_row_length_notice(
    file: &str,
    row_number: u64,
    header_len: usize,
    row_len: usize,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_row_length",
        NoticeSeverity::Error,
        "row has invalid length",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("filename", file);
    notice.insert_context_field("headerCount", header_len);
    notice.insert_context_field("rowLength", row_len);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "filename".into(),
        "headerCount".into(),
        "rowLength".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_u64(notice: &ValidationNotice, key: &str) -> u64 {
        notice
            .context
            .get(key)
            .and_then(|value| value.as_u64())
            .unwrap_or_default()
    }

    /// The exported enum lists feed the spec watcher, so a value accepted per
    /// cell but missing from the list would hide real drift, and vice versa.
    #[test]
    fn enum_allowed_values_agree_with_the_cell_check() {
        for (index, kind) in EnumKind::ALL.iter().enumerate() {
            assert_eq!(
                kind.position_in_all(),
                index,
                "{kind:?} is out of place in EnumKind::ALL"
            );
            let allowed = enum_allowed_values(*kind);
            assert!(
                allowed.windows(2).all(|pair| pair[0] < pair[1]),
                "{kind:?} values must be sorted and unique"
            );
            for value in -5..=2000 {
                assert_eq!(
                    enum_value_allowed(*kind, value),
                    allowed.contains(&value),
                    "{kind:?} disagrees about {value}"
                );
            }
        }
    }

    /// Every enum column must be reachable through the exported lookup, or the
    /// spec watcher would silently skip it.
    #[test]
    fn every_enum_column_exports_its_values() {
        for file in crate::feed::GTFS_FILE_NAMES {
            let Some(schema) = crate::csv_schema::schema_for_file(file) else {
                continue;
            };
            for field in schema.fields {
                if enum_kind(field).is_some() {
                    assert!(
                        enum_values_for_field(field).is_some_and(|values| !values.is_empty()),
                        "{file}:{field} is an enum with no exported values"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_row_notice_uses_csv_row_number() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut notices = NoticeContainer::new();
        let data = b"agency_name,agency_url,agency_timezone\n,,\n";

        validate_csv_data("agency.txt", data, &mut notices);

        let notice = notices
            .iter()
            .find(|notice| notice.code == "empty_row")
            .expect("empty row notice");
        assert_eq!(context_u64(notice, "csvRowNumber"), 2);
    }

    #[test]
    fn mixed_case_violation_matches_java_tokenization() {
        assert!(!is_mixed_case_violation("FOO"));
        assert!(is_mixed_case_violation("foo"));
        assert!(is_mixed_case_violation("'FOO"));
        assert!(is_mixed_case_violation("FOO BAR"));
        assert!(!is_mixed_case_violation("Foo Bar"));
        assert!(is_mixed_case_violation("'\u{05D0}\u{05D1}"));
        // Thai combining vowel signs and tone marks are Mn, not \p{L}: Java
        // splits on them, so one Thai word becomes several caseless tokens.
        assert!(is_mixed_case_violation(
            "\u{0E40}\u{0E04}\u{0E2B}\u{0E30}\u{0E23}\u{0E31}\u{0E07}\u{0E2A}\u{0E34}\u{0E15}"
        ));
        // ...while a word without marks stays one token and is never lowercase.
        assert!(!is_mixed_case_violation("\u{0E01}\u{0E02}\u{0E04}"));
        // No letters at all: Java's split returns an empty array.
        assert!(!is_mixed_case_violation("123 - 456"));
        assert!(!is_mixed_case_violation(""));
        // A single astral letter is two UTF-16 units, so it counts as long.
        assert!(is_mixed_case_violation("\u{1D41A}"));
        assert!(!is_mixed_case_violation("a"));
    }

    /// Java raises mixed_case from a SingleEntityValidator, which never sees
    /// a row whose fields failed to parse (Thailand, mdb-1831, agency.txt
    /// rows with agency_url "-").
    #[test]
    fn mixed_case_is_not_reported_on_rows_with_field_errors() {
        let mut notices = NoticeContainer::new();
        let data = b"agency_name,agency_url,agency_timezone\nlower name,-,Asia/Bangkok\nlower name,https://example.com,Asia/Bangkok\n";
        validate_csv_data("agency.txt", data, &mut notices);
        let rows: Vec<u64> = notices
            .iter()
            .filter(|n| n.code == "mixed_case_recommended_field")
            .map(|n| n.row.unwrap_or_default())
            .collect();
        assert_eq!(rows, vec![3]);
        assert!(notices.iter().any(|n| n.code == "invalid_url"));
    }

    #[test]
    fn number_out_of_range_uses_java_field_types() {
        let mut notices = NoticeContainer::new();
        let data = b"stop_id,stop_lat,stop_lon\nS1,91.0,181.0\n";

        validate_csv_data("stops.txt", data, &mut notices);

        let mut field_types: Vec<_> = notices
            .iter()
            .filter(|notice| notice.code == "number_out_of_range")
            .filter_map(|notice| notice.context.get("fieldType"))
            .filter_map(|value| value.as_str())
            .collect();
        field_types.sort();
        assert_eq!(
            field_types,
            vec!["latitude within [-90, 90]", "longitude within [-180, 180]"]
        );
    }

    #[test]
    fn missing_required_field_emits_notice() {
        let mut notices = NoticeContainer::new();
        let data = b"agency_name,agency_url,agency_timezone\n,https://example.com,UTC\n";

        validate_csv_data("agency.txt", data, &mut notices);

        assert!(notices
            .iter()
            .any(|notice| notice.code == "missing_required_field"));
    }

    #[test]
    fn non_negative_integer_out_of_range_uses_java_field_type() {
        let mut notices = NoticeContainer::new();
        let data = b"trip_id,stop_sequence\nT1,-2\n";

        validate_csv_data("stop_times.txt", data, &mut notices);

        let notice = notices
            .iter()
            .find(|notice| notice.code == "number_out_of_range")
            .expect("number_out_of_range notice");
        let field_type = notice
            .context
            .get("fieldType")
            .and_then(|value| value.as_str());
        assert_eq!(field_type, Some("non-negative integer"));
    }

    #[test]
    fn validates_v8_enum_fields() {
        let mut notices = NoticeContainer::new();
        validate_csv_data(
            "trips.txt",
            b"route_id,service_id,trip_id,cars_allowed\nR,S,T,3\n",
            &mut notices,
        );
        validate_csv_data(
            "agency.txt",
            b"agency_name,agency_url,agency_timezone,cemv_support\nA,https://example.com,UTC,3\n",
            &mut notices,
        );
        validate_csv_data("stops.txt", b"stop_id,stop_access\nS,2\n", &mut notices);

        let enum_notices = notices
            .iter()
            .filter(|notice| notice.code == "unexpected_enum_value")
            .count();
        assert_eq!(enum_notices, 3);
        assert!(!notices.iter().any(|notice| notice.code == "unknown_column"));
    }
}

fn new_line_notice(file: &str, field_name: &str, row_number: u64, value: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "new_line_in_value",
        NoticeSeverity::Error,
        "value contains new line",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("fieldValue", value);
    notice.insert_context_field("filename", file);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn leading_or_trailing_whitespaces_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "leading_or_trailing_whitespaces",
        NoticeSeverity::Warning,
        "value has leading or trailing whitespaces",
    );
    notice.insert_context_field("filename", file);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "filename".into(),
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
    ];
    fix_suggest::attach_fix(
        &mut notice,
        "Trim leading and trailing whitespace",
        FixSafety::Safe,
        file,
        row_number,
        field_name,
        value,
        trim_java_whitespace(value).to_string(),
    );
    notice
}

#[allow(dead_code)]
fn invalid_character_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_character",
        NoticeSeverity::Error,
        "value contains invalid characters",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("fieldValue", value);
    notice.insert_context_field("filename", file);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn non_ascii_notice(
    file: &str,
    column_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "non_ascii_or_non_printable_char",
        NoticeSeverity::Warning,
        "value contains non-ascii or non-printable characters",
    );
    notice.insert_context_field("columnName", column_name);
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldValue", value);
    notice.insert_context_field("filename", file);
    notice.field_order = vec![
        "columnName".into(),
        "csvRowNumber".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn too_many_rows_notice(file: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "too_many_rows",
        NoticeSeverity::Error,
        "csv file has too many rows",
    );
    notice.insert_context_field("filename", file);
    notice.insert_context_field("rowNumber", row_number);
    notice.field_order = vec!["filename".into(), "rowNumber".into()];
    notice
}

fn invalid_integer_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_integer",
        NoticeSeverity::Error,
        "field cannot be parsed as integer",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::whole_number(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Drop the redundant fractional part",
            FixSafety::RequiresConfirmation,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_float_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_float",
        NoticeSeverity::Error,
        "field cannot be parsed as float",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::decimal_comma(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Use a decimal point instead of a comma",
            FixSafety::RequiresConfirmation,
            value,
            replacement,
        );
    }
    notice
}

fn number_out_of_range_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    field_type: &str,
    value: f64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "number_out_of_range",
        NoticeSeverity::Error,
        "field value is out of range",
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("fieldType", field_type);
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldType".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn number_out_of_range_notice_int(
    file: &str,
    field_name: &str,
    row_number: u64,
    field_type: &str,
    value: i64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "number_out_of_range",
        NoticeSeverity::Error,
        "field value is out of range",
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("fieldType", field_type);
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldType".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn invalid_date_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_date",
        NoticeSeverity::Error,
        "field cannot be parsed as date",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::date(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Rewrite the date as YYYYMMDD",
            FixSafety::Safe,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_time_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_time",
        NoticeSeverity::Error,
        "field cannot be parsed as time",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::time(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Complete the time as HH:MM:SS",
            FixSafety::Safe,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_color_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_color",
        NoticeSeverity::Error,
        "field cannot be parsed as color",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::color(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Normalize the color to six hex digits",
            FixSafety::Safe,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_timezone_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_timezone",
        NoticeSeverity::Error,
        "field cannot be parsed as timezone",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    notice
}

fn invalid_language_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_language_code",
        NoticeSeverity::Error,
        "field contains invalid language code",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    notice
}

fn invalid_currency_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_currency",
        NoticeSeverity::Error,
        "field contains invalid currency code",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    notice
}

fn invalid_url_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_url",
        NoticeSeverity::Error,
        "field contains invalid url",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::url(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Add the https:// scheme",
            FixSafety::Safe,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_email_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_email",
        NoticeSeverity::Error,
        "field contains invalid email",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    if let Some(replacement) = fix_suggest::email(value) {
        fix_suggest::attach_field_fix(
            &mut notice,
            "Strip the mailto: prefix or angle brackets",
            FixSafety::Safe,
            value,
            replacement,
        );
    }
    notice
}

fn invalid_phone_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_phone_number",
        NoticeSeverity::Error,
        "field contains invalid phone number",
    );
    populate_field_notice(&mut notice, file, field_name, row_number, value);
    notice
}

fn populate_field_notice(
    notice: &mut ValidationNotice,
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) {
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
}

fn mixed_case_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let message = if is_mixed_case_field(field_name) {
        "field should use mixed case"
    } else {
        "field should use lower case"
    };
    let mut notice = ValidationNotice::new(
        "mixed_case_recommended_field",
        NoticeSeverity::Warning,
        message,
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn unexpected_enum_value_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: i64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unexpected_enum_value",
        NoticeSeverity::Warning,
        "unexpected enum value",
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn invalid_currency_amount_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    currency_code: &str,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_currency_amount",
        NoticeSeverity::Error,
        "currency amount does not match currency code",
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.insert_context_field("currencyCode", currency_code);
    notice.insert_context_field("fieldValue", value);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "currencyCode".into(),
        "fieldName".into(),
        "fieldValue".into(),
        "filename".into(),
    ];
    notice
}

fn validate_currency_amount(
    file: &str,
    record: &StringRecord,
    header_index: &HashMap<String, usize>,
    row_number: u64,
    notices: &mut Vec<ValidationNotice>,
) {
    let (Some(&amount_index), Some(&currency_index)) =
        (header_index.get("amount"), header_index.get("currency"))
    else {
        return;
    };

    let amount = trim_java_whitespace(record.get(amount_index).unwrap_or(""));
    let currency = trim_java_whitespace(record.get(currency_index).unwrap_or(""));
    if amount.is_empty() || currency.is_empty() {
        return;
    }
    let Some(scale) = decimal_scale(amount) else {
        return;
    };
    let Some(expected_scale) = currency_fraction_digits(currency) else {
        return;
    };

    if scale != expected_scale {
        notices.push(invalid_currency_amount_notice(
            file, "amount", row_number, currency, amount,
        ));
    }
}

fn validate_enum_value(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
    kind: EnumKind,
    notices: &mut Vec<ValidationNotice>,
) {
    // Java reads an enum with `Integer.parseInt`, so a value past 32 bits is
    // an invalid integer, not an unexpected enum value.
    match value.parse::<i32>().map(i64::from) {
        Ok(value) => {
            if !enum_value_allowed(kind, value) {
                notices.push(unexpected_enum_value_notice(
                    file, field_name, row_number, value,
                ));
            }
        }
        Err(_) => {
            notices.push(invalid_integer_notice(file, field_name, row_number, value));
        }
    }
}

fn enum_kind(field: &str) -> Option<EnumKind> {
    match field {
        "location_type" => Some(EnumKind::LocationType),
        "wheelchair_boarding" => Some(EnumKind::WheelchairBoarding),
        "route_type" => Some(EnumKind::RouteType),
        "continuous_pickup" | "continuous_drop_off" => Some(EnumKind::ContinuousPickupDropOff),
        "pickup_type" | "drop_off_type" => Some(EnumKind::PickupDropOffType),
        "booking_type" => Some(EnumKind::BookingType),
        "direction_id" => Some(EnumKind::DirectionId),
        "wheelchair_accessible" => Some(EnumKind::WheelchairAccessible),
        "bikes_allowed" => Some(EnumKind::BikesAllowed),
        "cars_allowed" => Some(EnumKind::CarsAllowed),
        "cemv_support" => Some(EnumKind::ContactlessEmvSupport),
        "stop_access" => Some(EnumKind::StopAccess),
        "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday" => {
            Some(EnumKind::ServiceAvailability)
        }
        "exception_type" => Some(EnumKind::ExceptionType),
        "payment_method" => Some(EnumKind::PaymentMethod),
        "transfers" => Some(EnumKind::Transfers),
        "exact_times" => Some(EnumKind::ExactTimes),
        "transfer_type" => Some(EnumKind::TransferType),
        "pathway_mode" => Some(EnumKind::PathwayMode),
        "is_bidirectional" => Some(EnumKind::Bidirectional),
        "is_producer" | "is_operator" | "is_authority" => Some(EnumKind::YesNo),
        "timepoint" => Some(EnumKind::Timepoint),
        "fare_media_type" => Some(EnumKind::FareMediaType),
        "duration_limit_type" => Some(EnumKind::DurationLimitType),
        "fare_transfer_type" => Some(EnumKind::FareTransferType),
        "is_default_fare_category" => Some(EnumKind::RiderFareCategory),
        _ => None,
    }
}

/// The values every enum column accepts, spelled out.
///
/// `enum_value_allowed` stays a `matches!` chain because it runs once per cell;
/// this list serves the machine-readable spec surface, which the spec watcher
/// diffs against the published specification. The two are kept in agreement by
/// `enum_allowed_values_agree_with_the_cell_check`.
fn enum_allowed_values(kind: EnumKind) -> &'static [i64] {
    match kind {
        EnumKind::LocationType => &[0, 1, 2, 3, 4],
        EnumKind::WheelchairBoarding => &[0, 1, 2],
        EnumKind::RouteType => &[0, 1, 2, 3, 4, 5, 6, 7, 11, 12],
        EnumKind::ContinuousPickupDropOff => &[0, 1, 2, 3],
        EnumKind::PickupDropOffType => &[0, 1, 2, 3],
        EnumKind::BookingType => &[0, 1, 2],
        EnumKind::DirectionId => &[0, 1],
        EnumKind::WheelchairAccessible => &[0, 1, 2],
        EnumKind::BikesAllowed => &[0, 1, 2],
        EnumKind::CarsAllowed => &[0, 1, 2],
        EnumKind::ContactlessEmvSupport => &[0, 1, 2],
        EnumKind::StopAccess => &[0, 1],
        EnumKind::ServiceAvailability => &[0, 1],
        EnumKind::ExceptionType => &[1, 2],
        EnumKind::PaymentMethod => &[0, 1],
        EnumKind::Transfers => &[0, 1, 2],
        EnumKind::ExactTimes => &[0, 1],
        EnumKind::TransferType => &[0, 1, 2, 3, 4, 5],
        EnumKind::PathwayMode => &[1, 2, 3, 4, 5, 6, 7],
        EnumKind::Bidirectional => &[0, 1],
        EnumKind::YesNo => &[0, 1],
        EnumKind::Timepoint => &[0, 1],
        EnumKind::FareMediaType => &[0, 1, 2, 3, 4],
        EnumKind::DurationLimitType => &[0, 1, 2, 3],
        EnumKind::FareTransferType => &[0, 1, 2],
        EnumKind::RiderFareCategory => &[0, 1],
    }
}

/// The enum values this build accepts for `field`, or `None` when the column is
/// not an enum.
pub(crate) fn enum_values_for_field(field: &str) -> Option<&'static [i64]> {
    enum_kind(field).map(enum_allowed_values)
}

fn enum_value_allowed(kind: EnumKind, value: i64) -> bool {
    match kind {
        EnumKind::LocationType => matches!(value, 0 | 1 | 2 | 3 | 4),
        EnumKind::WheelchairBoarding => matches!(value, 0 | 1 | 2),
        EnumKind::RouteType => matches!(value, 0..=7 | 11 | 12),
        EnumKind::ContinuousPickupDropOff => matches!(value, 0 | 1 | 2 | 3),
        EnumKind::PickupDropOffType => matches!(value, 0 | 1 | 2 | 3),
        EnumKind::BookingType => matches!(value, 0 | 1 | 2),
        EnumKind::DirectionId => matches!(value, 0 | 1),
        EnumKind::WheelchairAccessible => matches!(value, 0 | 1 | 2),
        EnumKind::BikesAllowed => matches!(value, 0 | 1 | 2),
        EnumKind::CarsAllowed => matches!(value, 0 | 1 | 2),
        EnumKind::ContactlessEmvSupport => matches!(value, 0 | 1 | 2),
        EnumKind::StopAccess => matches!(value, 0 | 1),
        EnumKind::ServiceAvailability => matches!(value, 0 | 1),
        EnumKind::ExceptionType => matches!(value, 1 | 2),
        EnumKind::PaymentMethod => matches!(value, 0 | 1),
        EnumKind::Transfers => matches!(value, 0 | 1 | 2),
        EnumKind::ExactTimes => matches!(value, 0 | 1),
        EnumKind::TransferType => matches!(value, 0 | 1 | 2 | 3 | 4 | 5),
        EnumKind::PathwayMode => matches!(value, 1 | 2 | 3 | 4 | 5 | 6 | 7),
        EnumKind::Bidirectional => matches!(value, 0 | 1),
        EnumKind::YesNo => matches!(value, 0 | 1),
        EnumKind::Timepoint => matches!(value, 0 | 1),
        EnumKind::FareMediaType => matches!(value, 0 | 1 | 2 | 3 | 4),
        EnumKind::DurationLimitType => matches!(value, 0 | 1 | 2 | 3),
        EnumKind::FareTransferType => matches!(value, 0 | 1 | 2),
        EnumKind::RiderFareCategory => matches!(value, 0 | 1),
    }
}

fn is_mixed_case_field(field: &str) -> bool {
    MIXED_CASE_FIELDS.contains(&field)
}

fn is_float_field(field: &str) -> bool {
    FLOAT_FIELDS.contains(&field)
}

fn is_latitude_field(field: &str) -> bool {
    LATITUDE_FIELDS.contains(&field)
}

fn is_longitude_field(field: &str) -> bool {
    LONGITUDE_FIELDS.contains(&field)
}

fn is_integer_field(field: &str) -> bool {
    INTEGER_FIELDS.contains(&field)
}

fn integer_bounds(field: &str) -> Option<NumberBounds> {
    if NON_NEGATIVE_INTEGER_FIELDS.contains(&field) {
        Some(NumberBounds::NonNegative)
    } else if POSITIVE_INTEGER_FIELDS.contains(&field) {
        Some(NumberBounds::Positive)
    } else if NON_ZERO_INTEGER_FIELDS.contains(&field) {
        Some(NumberBounds::NonZero)
    } else {
        None
    }
}

fn float_bounds(field: &str) -> Option<NumberBounds> {
    if NON_NEGATIVE_FLOAT_FIELDS.contains(&field) {
        Some(NumberBounds::NonNegative)
    } else if POSITIVE_FLOAT_FIELDS.contains(&field) {
        Some(NumberBounds::Positive)
    } else {
        None
    }
}

fn decimal_bounds(field: &str) -> Option<NumberBounds> {
    if NON_NEGATIVE_DECIMAL_FIELDS.contains(&field) {
        Some(NumberBounds::NonNegative)
    } else {
        None
    }
}

fn bounds_field_type(bounds: NumberBounds, kind: NumberKind) -> &'static str {
    match (bounds, kind) {
        (NumberBounds::Positive, NumberKind::Integer) => "positive integer",
        (NumberBounds::NonNegative, NumberKind::Integer) => "non-negative integer",
        (NumberBounds::NonZero, NumberKind::Integer) => "non-zero integer",
        (NumberBounds::Positive, NumberKind::Float) => "positive float",
        (NumberBounds::NonNegative, NumberKind::Float) => "non-negative float",
        (NumberBounds::NonZero, NumberKind::Float) => "non-zero float",
        (NumberBounds::Positive, NumberKind::Decimal) => "positive decimal",
        (NumberBounds::NonNegative, NumberKind::Decimal) => "non-negative decimal",
        (NumberBounds::NonZero, NumberKind::Decimal) => "non-zero decimal",
    }
}

fn violates_bounds_i64(value: i64, bounds: NumberBounds) -> bool {
    match bounds {
        NumberBounds::Positive => value <= 0,
        NumberBounds::NonNegative => value < 0,
        NumberBounds::NonZero => value == 0,
    }
}

fn violates_bounds_f64(value: f64, bounds: NumberBounds) -> bool {
    match bounds {
        NumberBounds::Positive => value <= 0.0,
        NumberBounds::NonNegative => value < 0.0,
        NumberBounds::NonZero => value == 0.0,
    }
}

fn is_date_field(field: &str) -> bool {
    DATE_FIELDS.contains(&field)
}

fn is_time_field(field: &str) -> bool {
    TIME_FIELDS.contains(&field)
}

fn is_color_field(field: &str) -> bool {
    COLOR_FIELDS.contains(&field)
}

fn is_timezone_field(field: &str) -> bool {
    TIMEZONE_FIELDS.contains(&field)
}

fn is_language_field(field: &str) -> bool {
    LANGUAGE_FIELDS.contains(&field)
}

fn is_currency_field(field: &str) -> bool {
    CURRENCY_FIELDS.contains(&field)
}

fn is_url_field(field: &str) -> bool {
    URL_FIELDS.contains(&field)
}

fn is_email_field(field: &str) -> bool {
    EMAIL_FIELDS.contains(&field)
}

fn is_phone_field(field: &str) -> bool {
    PHONE_FIELDS.contains(&field)
}

/// Fields whose name ends in `_id` but which gtfs-validator 8.0.1 does not
/// annotate `@FieldType(ID)`, so `non_ascii_or_non_printable_char` never
/// fires on them there (Gtfs*Schema.java). `direction_id` is an enum.
const NON_ID_TYPED_ID_FIELDS: &[(&str, &str)] = &[
    ("booking_rules.txt", "prior_notice_service_id"),
    ("frequencies.txt", "trip_id"),
    ("location_group_stops.txt", "location_group_id"),
    ("location_group_stops.txt", "stop_id"),
    ("timeframes.txt", "service_id"),
    ("translations.txt", "record_id"),
    ("translations.txt", "record_sub_id"),
    ("trips.txt", "direction_id"),
];

fn is_id_field(file_name: &str, field: &str) -> bool {
    (field.ends_with("_id") || field == "parent_station")
        && !NON_ID_TYPED_ID_FIELDS.contains(&(file_name, field))
}

fn has_only_printable_ascii(value: &str) -> bool {
    value.chars().all(|ch| (32..127).contains(&(ch as u32)))
}

pub fn is_value_validated_field(field: &str) -> bool {
    let normalized = field.trim().to_ascii_lowercase();
    let field = normalized.as_str();
    enum_kind(field).is_some()
        || is_integer_field(field)
        || is_float_field(field)
        || is_date_field(field)
        || is_time_field(field)
        || is_color_field(field)
}

/// `MixedCaseValidatorGenerator` in gtfs-validator, step for step. Tokens are
/// runs of `\p{L}`; `String.split` keeps a leading empty token and drops
/// trailing ones; lengths are UTF-16 units; case tests are `\p{Ll}`/`\p{Lu}`.
/// The generated `\d` tests can never match a letters-only token, so they are
/// omitted.
fn is_mixed_case_violation(value: &str) -> bool {
    let tokens = java_split_on_non_letters(value);

    if tokens.len() == 1 {
        let token = tokens[0];
        return utf16_len(token) > 1 && token.chars().all(is_java_lowercase);
    }

    let mut has_mixed_case_token = false;
    let mut no_number_tokens = 0;
    for token in tokens {
        if utf16_len(token) == 1 {
            continue;
        }
        no_number_tokens += 1;
        if token.chars().any(is_java_uppercase) && token.chars().any(is_java_lowercase) {
            has_mixed_case_token = true;
        }
    }
    no_number_tokens >= 2 && !has_mixed_case_token
}

fn utf16_len(token: &str) -> usize {
    token.chars().map(char::len_utf16).sum()
}

/// Java `\p{L}`: general category Lu, Ll, Lt, Lm or Lo. Not `char::is_alphabetic`,
/// whose Alphabetic property also covers combining vowel signs (Thai, Lao,
/// Devanagari...), letter numbers and circled letters, which Java splits on.
fn is_java_letter(ch: char) -> bool {
    use unicode_general_category::{get_general_category, GeneralCategory};
    matches!(
        get_general_category(ch),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

fn is_java_lowercase(ch: char) -> bool {
    unicode_general_category::get_general_category(ch)
        == unicode_general_category::GeneralCategory::LowercaseLetter
}

fn is_java_uppercase(ch: char) -> bool {
    unicode_general_category::get_general_category(ch)
        == unicode_general_category::GeneralCategory::UppercaseLetter
}

/// `value.split("[^\\p{L}]+")` with Java semantics: no match returns the
/// whole string (so `""` gives `[""]`), a leading separator yields a leading
/// empty token, and trailing empty tokens are removed, so a value with no
/// letters at all splits into nothing.
fn java_split_on_non_letters(value: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut run_start = None;
    for (idx, ch) in value.char_indices() {
        if is_java_letter(ch) {
            if run_start.is_none() {
                run_start = Some(idx);
            }
        } else if let Some(start) = run_start.take() {
            tokens.push(&value[start..idx]);
        }
    }
    if let Some(start) = run_start {
        tokens.push(&value[start..]);
    }

    if tokens.is_empty() {
        return if value.is_empty() {
            vec![""]
        } else {
            Vec::new()
        };
    }
    if value.chars().next().is_some_and(|ch| !is_java_letter(ch)) {
        tokens.insert(0, "");
    }
    tokens
}

fn trim_java_whitespace(value: &str) -> &str {
    value.trim_matches(|ch| ch <= ' ')
}

fn is_valid_url(value: &str) -> bool {
    Url::parse(value).is_ok()
}

pub(crate) fn is_valid_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    domain.contains('.')
}

fn is_valid_phone_number(value: &str) -> bool {
    let mut digits = 0;
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        match ch {
            '+' | '-' | '(' | ')' | '.' | ' ' => {}
            _ => return false,
        }
    }
    digits >= 2
}

fn is_valid_language_code(value: &str) -> bool {
    let mut parts = value.split('-');
    let primary = match parts.next() {
        Some(part) => part,
        None => return false,
    };
    if !(2..=3).contains(&primary.len()) {
        return false;
    }
    if !primary.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    for part in parts {
        if !(2..=8).contains(&part.len()) {
            return false;
        }
        if !part.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

fn is_valid_timezone(value: &str) -> bool {
    let zones = valid_timezones();
    if zones.is_empty() {
        return true;
    }
    zones.contains(value)
}

/// Embedded IANA timezone list for environments without filesystem access (WASM)
const IANA_TIMEZONES: &[&str] = &[
    "Africa/Abidjan",
    "Africa/Accra",
    "Africa/Addis_Ababa",
    "Africa/Algiers",
    "Africa/Asmara",
    "Africa/Bamako",
    "Africa/Bangui",
    "Africa/Banjul",
    "Africa/Bissau",
    "Africa/Blantyre",
    "Africa/Brazzaville",
    "Africa/Bujumbura",
    "Africa/Cairo",
    "Africa/Casablanca",
    "Africa/Ceuta",
    "Africa/Conakry",
    "Africa/Dakar",
    "Africa/Dar_es_Salaam",
    "Africa/Djibouti",
    "Africa/Douala",
    "Africa/El_Aaiun",
    "Africa/Freetown",
    "Africa/Gaborone",
    "Africa/Harare",
    "Africa/Johannesburg",
    "Africa/Juba",
    "Africa/Kampala",
    "Africa/Khartoum",
    "Africa/Kigali",
    "Africa/Kinshasa",
    "Africa/Lagos",
    "Africa/Libreville",
    "Africa/Lome",
    "Africa/Luanda",
    "Africa/Lubumbashi",
    "Africa/Lusaka",
    "Africa/Malabo",
    "Africa/Maputo",
    "Africa/Maseru",
    "Africa/Mbabane",
    "Africa/Mogadishu",
    "Africa/Monrovia",
    "Africa/Nairobi",
    "Africa/Ndjamena",
    "Africa/Niamey",
    "Africa/Nouakchott",
    "Africa/Ouagadougou",
    "Africa/Porto-Novo",
    "Africa/Sao_Tome",
    "Africa/Tripoli",
    "Africa/Tunis",
    "Africa/Windhoek",
    "America/Adak",
    "America/Anchorage",
    "America/Anguilla",
    "America/Antigua",
    "America/Araguaina",
    "America/Argentina/Buenos_Aires",
    "America/Argentina/Catamarca",
    "America/Argentina/Cordoba",
    "America/Argentina/Jujuy",
    "America/Argentina/La_Rioja",
    "America/Argentina/Mendoza",
    "America/Argentina/Rio_Gallegos",
    "America/Argentina/Salta",
    "America/Argentina/San_Juan",
    "America/Argentina/San_Luis",
    "America/Argentina/Tucuman",
    "America/Argentina/Ushuaia",
    "America/Aruba",
    "America/Asuncion",
    "America/Atikokan",
    "America/Bahia",
    "America/Bahia_Banderas",
    "America/Barbados",
    "America/Belem",
    "America/Belize",
    "America/Blanc-Sablon",
    "America/Boa_Vista",
    "America/Bogota",
    "America/Boise",
    "America/Cambridge_Bay",
    "America/Campo_Grande",
    "America/Cancun",
    "America/Caracas",
    "America/Cayenne",
    "America/Cayman",
    "America/Chicago",
    "America/Chihuahua",
    "America/Ciudad_Juarez",
    "America/Costa_Rica",
    "America/Creston",
    "America/Cuiaba",
    "America/Curacao",
    "America/Danmarkshavn",
    "America/Dawson",
    "America/Dawson_Creek",
    "America/Denver",
    "America/Detroit",
    "America/Dominica",
    "America/Edmonton",
    "America/Eirunepe",
    "America/El_Salvador",
    "America/Fort_Nelson",
    "America/Fortaleza",
    "America/Glace_Bay",
    "America/Goose_Bay",
    "America/Grand_Turk",
    "America/Grenada",
    "America/Guadeloupe",
    "America/Guatemala",
    "America/Guayaquil",
    "America/Guyana",
    "America/Halifax",
    "America/Havana",
    "America/Hermosillo",
    "America/Indiana/Indianapolis",
    "America/Indiana/Knox",
    "America/Indiana/Marengo",
    "America/Indiana/Petersburg",
    "America/Indiana/Tell_City",
    "America/Indiana/Vevay",
    "America/Indiana/Vincennes",
    "America/Indiana/Winamac",
    "America/Inuvik",
    "America/Iqaluit",
    "America/Jamaica",
    "America/Juneau",
    "America/Kentucky/Louisville",
    "America/Kentucky/Monticello",
    "America/Kralendijk",
    "America/La_Paz",
    "America/Lima",
    "America/Los_Angeles",
    "America/Lower_Princes",
    "America/Maceio",
    "America/Managua",
    "America/Manaus",
    "America/Marigot",
    "America/Martinique",
    "America/Matamoros",
    "America/Mazatlan",
    "America/Menominee",
    "America/Merida",
    "America/Metlakatla",
    "America/Mexico_City",
    "America/Miquelon",
    "America/Moncton",
    "America/Monterrey",
    "America/Montevideo",
    "America/Montserrat",
    "America/Nassau",
    "America/New_York",
    "America/Nipigon",
    "America/Nome",
    "America/Noronha",
    "America/North_Dakota/Beulah",
    "America/North_Dakota/Center",
    "America/North_Dakota/New_Salem",
    "America/Nuuk",
    "America/Ojinaga",
    "America/Panama",
    "America/Paramaribo",
    "America/Phoenix",
    "America/Port-au-Prince",
    "America/Port_of_Spain",
    "America/Porto_Velho",
    "America/Puerto_Rico",
    "America/Punta_Arenas",
    "America/Rankin_Inlet",
    "America/Recife",
    "America/Regina",
    "America/Resolute",
    "America/Rio_Branco",
    "America/Santarem",
    "America/Santiago",
    "America/Santo_Domingo",
    "America/Sao_Paulo",
    "America/Scoresbysund",
    "America/Sitka",
    "America/St_Barthelemy",
    "America/St_Johns",
    "America/St_Kitts",
    "America/St_Lucia",
    "America/St_Thomas",
    "America/St_Vincent",
    "America/Swift_Current",
    "America/Tegucigalpa",
    "America/Thule",
    "America/Tijuana",
    "America/Toronto",
    "America/Tortola",
    "America/Vancouver",
    "America/Whitehorse",
    "America/Winnipeg",
    "America/Yakutat",
    "America/Yellowknife",
    "Antarctica/Casey",
    "Antarctica/Davis",
    "Antarctica/DumontDUrville",
    "Antarctica/Macquarie",
    "Antarctica/Mawson",
    "Antarctica/McMurdo",
    "Antarctica/Palmer",
    "Antarctica/Rothera",
    "Antarctica/Syowa",
    "Antarctica/Troll",
    "Antarctica/Vostok",
    "Arctic/Longyearbyen",
    "Asia/Aden",
    "Asia/Almaty",
    "Asia/Amman",
    "Asia/Anadyr",
    "Asia/Aqtau",
    "Asia/Aqtobe",
    "Asia/Ashgabat",
    "Asia/Atyrau",
    "Asia/Baghdad",
    "Asia/Bahrain",
    "Asia/Baku",
    "Asia/Bangkok",
    "Asia/Barnaul",
    "Asia/Beirut",
    "Asia/Bishkek",
    "Asia/Brunei",
    "Asia/Chita",
    "Asia/Choibalsan",
    "Asia/Colombo",
    "Asia/Damascus",
    "Asia/Dhaka",
    "Asia/Dili",
    "Asia/Dubai",
    "Asia/Dushanbe",
    "Asia/Famagusta",
    "Asia/Gaza",
    "Asia/Hebron",
    "Asia/Ho_Chi_Minh",
    "Asia/Hong_Kong",
    "Asia/Hovd",
    "Asia/Irkutsk",
    "Asia/Jakarta",
    "Asia/Jayapura",
    "Asia/Jerusalem",
    "Asia/Kabul",
    "Asia/Kamchatka",
    "Asia/Karachi",
    "Asia/Kathmandu",
    "Asia/Khandyga",
    "Asia/Kolkata",
    "Asia/Krasnoyarsk",
    "Asia/Kuala_Lumpur",
    "Asia/Kuching",
    "Asia/Kuwait",
    "Asia/Macau",
    "Asia/Magadan",
    "Asia/Makassar",
    "Asia/Manila",
    "Asia/Muscat",
    "Asia/Nicosia",
    "Asia/Novokuznetsk",
    "Asia/Novosibirsk",
    "Asia/Omsk",
    "Asia/Oral",
    "Asia/Phnom_Penh",
    "Asia/Pontianak",
    "Asia/Pyongyang",
    "Asia/Qatar",
    "Asia/Qostanay",
    "Asia/Qyzylorda",
    "Asia/Riyadh",
    "Asia/Sakhalin",
    "Asia/Samarkand",
    "Asia/Seoul",
    "Asia/Shanghai",
    "Asia/Singapore",
    "Asia/Srednekolymsk",
    "Asia/Taipei",
    "Asia/Tashkent",
    "Asia/Tbilisi",
    "Asia/Tehran",
    "Asia/Thimphu",
    "Asia/Tokyo",
    "Asia/Tomsk",
    "Asia/Ulaanbaatar",
    "Asia/Urumqi",
    "Asia/Ust-Nera",
    "Asia/Vientiane",
    "Asia/Vladivostok",
    "Asia/Yakutsk",
    "Asia/Yangon",
    "Asia/Yekaterinburg",
    "Asia/Yerevan",
    "Atlantic/Azores",
    "Atlantic/Bermuda",
    "Atlantic/Canary",
    "Atlantic/Cape_Verde",
    "Atlantic/Faroe",
    "Atlantic/Madeira",
    "Atlantic/Reykjavik",
    "Atlantic/South_Georgia",
    "Atlantic/St_Helena",
    "Atlantic/Stanley",
    "Australia/Adelaide",
    "Australia/Brisbane",
    "Australia/Broken_Hill",
    "Australia/Darwin",
    "Australia/Eucla",
    "Australia/Hobart",
    "Australia/Lindeman",
    "Australia/Lord_Howe",
    "Australia/Melbourne",
    "Australia/Perth",
    "Australia/Sydney",
    "Europe/Amsterdam",
    "Europe/Andorra",
    "Europe/Astrakhan",
    "Europe/Athens",
    "Europe/Belgrade",
    "Europe/Berlin",
    "Europe/Bratislava",
    "Europe/Brussels",
    "Europe/Bucharest",
    "Europe/Budapest",
    "Europe/Busingen",
    "Europe/Chisinau",
    "Europe/Copenhagen",
    "Europe/Dublin",
    "Europe/Gibraltar",
    "Europe/Guernsey",
    "Europe/Helsinki",
    "Europe/Isle_of_Man",
    "Europe/Istanbul",
    "Europe/Jersey",
    "Europe/Kaliningrad",
    "Europe/Kirov",
    "Europe/Kyiv",
    "Europe/Lisbon",
    "Europe/Ljubljana",
    "Europe/London",
    "Europe/Luxembourg",
    "Europe/Madrid",
    "Europe/Malta",
    "Europe/Mariehamn",
    "Europe/Minsk",
    "Europe/Monaco",
    "Europe/Moscow",
    "Europe/Oslo",
    "Europe/Paris",
    "Europe/Podgorica",
    "Europe/Prague",
    "Europe/Riga",
    "Europe/Rome",
    "Europe/Samara",
    "Europe/San_Marino",
    "Europe/Sarajevo",
    "Europe/Saratov",
    "Europe/Simferopol",
    "Europe/Skopje",
    "Europe/Sofia",
    "Europe/Stockholm",
    "Europe/Tallinn",
    "Europe/Tirane",
    "Europe/Ulyanovsk",
    "Europe/Vaduz",
    "Europe/Vatican",
    "Europe/Vienna",
    "Europe/Vilnius",
    "Europe/Volgograd",
    "Europe/Warsaw",
    "Europe/Zagreb",
    "Europe/Zurich",
    "Indian/Antananarivo",
    "Indian/Chagos",
    "Indian/Christmas",
    "Indian/Cocos",
    "Indian/Comoro",
    "Indian/Kerguelen",
    "Indian/Mahe",
    "Indian/Maldives",
    "Indian/Mauritius",
    "Indian/Mayotte",
    "Indian/Reunion",
    "Pacific/Apia",
    "Pacific/Auckland",
    "Pacific/Bougainville",
    "Pacific/Chatham",
    "Pacific/Chuuk",
    "Pacific/Easter",
    "Pacific/Efate",
    "Pacific/Fakaofo",
    "Pacific/Fiji",
    "Pacific/Funafuti",
    "Pacific/Galapagos",
    "Pacific/Gambier",
    "Pacific/Guadalcanal",
    "Pacific/Guam",
    "Pacific/Honolulu",
    "Pacific/Kanton",
    "Pacific/Kiritimati",
    "Pacific/Kosrae",
    "Pacific/Kwajalein",
    "Pacific/Majuro",
    "Pacific/Marquesas",
    "Pacific/Midway",
    "Pacific/Nauru",
    "Pacific/Niue",
    "Pacific/Norfolk",
    "Pacific/Noumea",
    "Pacific/Pago_Pago",
    "Pacific/Palau",
    "Pacific/Pitcairn",
    "Pacific/Pohnpei",
    "Pacific/Port_Moresby",
    "Pacific/Rarotonga",
    "Pacific/Saipan",
    "Pacific/Tahiti",
    "Pacific/Tarawa",
    "Pacific/Tongatapu",
    "Pacific/Wake",
    "Pacific/Wallis",
    "UTC",
    "Etc/GMT",
    "Etc/GMT+0",
    "Etc/GMT-0",
    "Etc/GMT0",
    "Etc/UTC",
    "Etc/Universal",
    "Etc/Zulu",
];

/// tzdb backward-compatibility link names (e.g. Europe/Nicosia, US/Eastern)
/// accepted by Java's `ZoneId`, and therefore by the canonical validator. Keep
/// these embedded so validation is deterministic even on systems without a
/// complete zoneinfo installation.
const IANA_TIMEZONE_LINKS: &[&str] = &[
    "Africa/Asmera",
    "Africa/Timbuktu",
    "America/Argentina/ComodRivadavia",
    "America/Atka",
    "America/Buenos_Aires",
    "America/Catamarca",
    "America/Coral_Harbour",
    "America/Cordoba",
    "America/Coyhaique",
    "America/Ensenada",
    "America/Fort_Wayne",
    "America/Godthab",
    "America/Indianapolis",
    "America/Jujuy",
    "America/Knox_IN",
    "America/Louisville",
    "America/Mendoza",
    "America/Montreal",
    "America/Pangnirtung",
    "America/Porto_Acre",
    "America/Rainy_River",
    "America/Rosario",
    "America/Santa_Isabel",
    "America/Shiprock",
    "America/Thunder_Bay",
    "America/Virgin",
    "Antarctica/South_Pole",
    "Asia/Ashkhabad",
    "Asia/Calcutta",
    "Asia/Chongqing",
    "Asia/Chungking",
    "Asia/Dacca",
    "Asia/Harbin",
    "Asia/Istanbul",
    "Asia/Kashgar",
    "Asia/Katmandu",
    "Asia/Macao",
    "Asia/Rangoon",
    "Asia/Saigon",
    "Asia/Tel_Aviv",
    "Asia/Thimbu",
    "Asia/Ujung_Pandang",
    "Asia/Ulan_Bator",
    "Atlantic/Faeroe",
    "Atlantic/Jan_Mayen",
    "Australia/ACT",
    "Australia/Canberra",
    "Australia/Currie",
    "Australia/LHI",
    "Australia/NSW",
    "Australia/North",
    "Australia/Queensland",
    "Australia/South",
    "Australia/Tasmania",
    "Australia/Victoria",
    "Australia/West",
    "Australia/Yancowinna",
    "Brazil/Acre",
    "Brazil/DeNoronha",
    "Brazil/East",
    "Brazil/West",
    "CET",
    "CST6CDT",
    "Canada/Atlantic",
    "Canada/Central",
    "Canada/Eastern",
    "Canada/Mountain",
    "Canada/Newfoundland",
    "Canada/Pacific",
    "Canada/Saskatchewan",
    "Canada/Yukon",
    "Chile/Continental",
    "Chile/EasterIsland",
    "Cuba",
    "EET",
    "EST",
    "EST5EDT",
    "Egypt",
    "Eire",
    "Etc/GMT+1",
    "Etc/GMT+10",
    "Etc/GMT+11",
    "Etc/GMT+12",
    "Etc/GMT+2",
    "Etc/GMT+3",
    "Etc/GMT+4",
    "Etc/GMT+5",
    "Etc/GMT+6",
    "Etc/GMT+7",
    "Etc/GMT+8",
    "Etc/GMT+9",
    "Etc/GMT-1",
    "Etc/GMT-10",
    "Etc/GMT-11",
    "Etc/GMT-12",
    "Etc/GMT-13",
    "Etc/GMT-14",
    "Etc/GMT-2",
    "Etc/GMT-3",
    "Etc/GMT-4",
    "Etc/GMT-5",
    "Etc/GMT-6",
    "Etc/GMT-7",
    "Etc/GMT-8",
    "Etc/GMT-9",
    "Etc/Greenwich",
    "Etc/UCT",
    "Europe/Belfast",
    "Europe/Kiev",
    "Europe/Nicosia",
    "Europe/Tiraspol",
    "Europe/Uzhgorod",
    "Europe/Zaporozhye",
    "GB",
    "GB-Eire",
    "GMT",
    "GMT+0",
    "GMT-0",
    "GMT0",
    "Greenwich",
    "HST",
    "Hongkong",
    "Iceland",
    "Iran",
    "Israel",
    "Jamaica",
    "Japan",
    "Kwajalein",
    "Libya",
    "MET",
    "MST",
    "MST7MDT",
    "Mexico/BajaNorte",
    "Mexico/BajaSur",
    "Mexico/General",
    "NZ",
    "NZ-CHAT",
    "Navajo",
    "PRC",
    "PST8PDT",
    "Pacific/Enderbury",
    "Pacific/Johnston",
    "Pacific/Ponape",
    "Pacific/Samoa",
    "Pacific/Truk",
    "Pacific/Yap",
    "Poland",
    "Portugal",
    "ROC",
    "ROK",
    "Singapore",
    "Turkey",
    "UCT",
    "US/Alaska",
    "US/Aleutian",
    "US/Arizona",
    "US/Central",
    "US/East-Indiana",
    "US/Eastern",
    "US/Hawaii",
    "US/Indiana-Starke",
    "US/Michigan",
    "US/Mountain",
    "US/Pacific",
    "US/Samoa",
    "Universal",
    "W-SU",
    "WET",
    "Zulu",
];

fn valid_timezones() -> &'static HashSet<String> {
    static TIMEZONES: OnceLock<HashSet<String>> = OnceLock::new();
    TIMEZONES.get_or_init(|| {
        let mut zones: HashSet<String> = IANA_TIMEZONES
            .iter()
            .chain(IANA_TIMEZONE_LINKS)
            .map(|s| s.to_string())
            .collect();
        // Also try to read from filesystem for any additional timezones
        for path in [
            "/usr/share/zoneinfo/zone1970.tab",
            "/usr/share/zoneinfo/zone.tab",
        ] {
            if let Ok(contents) = std::fs::read_to_string(path) {
                for line in contents.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let mut parts = trimmed.split('\t');
                    parts.next();
                    parts.next();
                    if let Some(name) = parts.next() {
                        zones.insert(name.trim().to_string());
                    }
                }
            }
        }
        zones
    })
}

fn is_valid_currency_code(value: &str) -> bool {
    currency_codes().contains(value)
}

fn currency_fraction_digits(value: &str) -> Option<u8> {
    if !is_valid_currency_code(value) {
        return None;
    }
    if CURRENCY_ZERO_DECIMALS.contains(&value) {
        return Some(0);
    }
    if CURRENCY_THREE_DECIMALS.contains(&value) {
        return Some(3);
    }
    if CURRENCY_FOUR_DECIMALS.contains(&value) {
        return Some(4);
    }
    Some(2)
}

fn currency_codes() -> &'static HashSet<&'static str> {
    static CODES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CODES.get_or_init(|| CURRENCY_CODES.iter().copied().collect())
}

fn decimal_scale(value: &str) -> Option<u8> {
    let value = value.trim();
    let value = value.strip_prefix('+').unwrap_or(value);
    let value = value.strip_prefix('-').unwrap_or(value);
    let mut parts = value.split('.');
    let int_part = parts.next()?;
    let frac_part = parts.next();
    if parts.next().is_some() || int_part.is_empty() {
        return None;
    }
    if !int_part.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    match frac_part {
        None => Some(0),
        Some(part) => {
            if part.is_empty() {
                return None;
            }
            if !part.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            u8::try_from(part.len()).ok()
        }
    }
}

#[allow(dead_code)]
fn missing_required_field_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_required_field",
        NoticeSeverity::Error,
        "required field is missing",
    );
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

fn missing_recommended_field_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_recommended_field",
        NoticeSeverity::Warning,
        "recommended field is missing",
    );
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
    notice
}

#[cfg(test)]
mod tests_timezones {
    use super::*;

    #[test]
    fn accepts_tzdb_link_names() {
        for zone in [
            "Europe/Nicosia",
            "US/Eastern",
            "Europe/Kiev",
            "Asia/Calcutta",
            "America/Buenos_Aires",
        ] {
            assert!(is_valid_timezone(zone), "{zone} should be valid");
        }
    }

    #[test]
    fn accepts_canonical_names() {
        assert!(is_valid_timezone("Asia/Nicosia"));
        assert!(is_valid_timezone("Europe/Berlin"));
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(!is_valid_timezone("Europe/Atlantis"));
        assert!(!is_valid_timezone(""));
    }
}

#[cfg(test)]
mod tests_whitespaces {
    use super::*;

    fn codes(file_name: &str, data: &[u8], code: &str) -> Vec<(String, String)> {
        let mut notices = NoticeContainer::new();
        validate_csv_data(file_name, data, &mut notices);
        notices
            .iter()
            .filter(|n| n.code == code)
            .map(|n| {
                (
                    n.context["fieldName"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    n.context["fieldValue"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect()
    }

    /// GTF-35: whitespace inside quotes is what Java reports, in default mode.
    #[test]
    fn quoted_whitespace_is_reported_without_thorough() {
        let data = b"stop_id,stop_name\nS1,\" Central Station \"\n";
        assert_eq!(
            codes("stops.txt", data, "leading_or_trailing_whitespaces"),
            vec![("stop_name".to_string(), " Central Station ".to_string())]
        );
    }

    /// Whitespace around a bare field is stripped by univocity before Java
    /// validates, so it must not be reported.
    #[test]
    fn bare_whitespace_is_not_reported() {
        let data = b"stop_id,stop_name\n S1 , Central Station \n";
        assert!(codes("stops.txt", data, "leading_or_trailing_whitespaces").is_empty());
    }

    /// Latvia (mdb-992): a quote after a space still opens the quote, so the
    /// value is `DUS`, not `"DUS"`, and mixed_case does not fire on it.
    #[test]
    fn quote_after_space_opens_the_field() {
        let data = b"stop_id,stop_name\n817, \"DUS\"\n";
        assert!(codes("stops.txt", data, "mixed_case_recommended_field").is_empty());
        assert!(codes("stops.txt", data, "leading_or_trailing_whitespaces").is_empty());
    }

    #[test]
    fn test_whitespace_checks_schema_aware() {
        let mut notices = NoticeContainer::new();
        let data = b"agency_name,extra_col,agency_url,agency_timezone\n agency 1 , val ,url,tz";
        validate_csv_data("agency.txt", data, &mut notices);

        let whitespace_notices: Vec<_> = notices
            .iter()
            .filter(|n| n.code == "leading_or_trailing_whitespaces")
            .collect();

        assert!(
            whitespace_notices.is_empty(),
            "Did not expect whitespace notices, found: {:?}",
            whitespace_notices
        );
    }
}

#[cfg(test)]
mod tests_non_ascii {
    use super::*;

    fn non_ascii_fields(file_name: &str, data: &[u8]) -> Vec<String> {
        let mut notices = NoticeContainer::new();
        validate_csv_data(file_name, data, &mut notices);
        notices
            .iter()
            .filter(|n| n.code == "non_ascii_or_non_printable_char")
            .map(|n| {
                n.context["columnName"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn id_typed_fields_are_checked() {
        let data = "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nvia\u{e7}\u{e3}o,08:00:00,08:00:00,S1,1".as_bytes();
        assert_eq!(non_ascii_fields("stop_times.txt", data), vec!["trip_id"]);
    }

    /// São Paulo (mdb-8): frequencies.trip_id is not `@FieldType(ID)` in
    /// gtfs-validator, so Java never reports it. Same for the other seven.
    #[test]
    fn fields_without_id_type_in_java_are_skipped() {
        let data =
            "trip_id,start_time,end_time,headway_secs\nvia\u{e7}\u{e3}o,08:00:00,09:00:00,600"
                .as_bytes();
        assert!(non_ascii_fields("frequencies.txt", data).is_empty());

        let data = "table_name,field_name,language,translation,record_id,record_sub_id\nstops,stop_name,fr,Gare,arr\u{ea}t,\u{e9}"
            .as_bytes();
        assert!(non_ascii_fields("translations.txt", data).is_empty());

        let data = "timeframe_group_id,service_id\n\u{e9}t\u{e9},\u{e9}t\u{e9}".as_bytes();
        assert_eq!(
            non_ascii_fields("timeframes.txt", data),
            vec!["timeframe_group_id"]
        );

        let data = "location_group_id,stop_id\n\u{e9},\u{e8}".as_bytes();
        assert!(non_ascii_fields("location_group_stops.txt", data).is_empty());

        let data =
            "booking_rule_id,booking_type,prior_notice_service_id\nr\u{e8}gle,2,\u{e9}".as_bytes();
        assert_eq!(
            non_ascii_fields("booking_rules.txt", data),
            vec!["booking_rule_id"]
        );
    }
}
