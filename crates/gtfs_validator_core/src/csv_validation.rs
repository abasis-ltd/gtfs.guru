use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use csv::{ReaderBuilder, StringRecord, Trim};
use url::Url;

use crate::csv_schema::schema_for_file;
use crate::feed::FARE_PRODUCTS_FILE;
use crate::validation_context::{thorough_mode_enabled, validation_country_code};
use crate::{NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_guru_model::{GtfsColor, GtfsDate, GtfsTime};

const MAX_ROW_NUMBER: u64 = 1_000_000_000;

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
    "stop_lat",
    "stop_lon",
];

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

use crate::csv_schema::CsvSchema;

pub struct RowValidator {
    pub file_name: String,
    pub headers: Vec<String>,
    pub normalized_headers: Vec<String>,
    pub header_index: HashMap<String, usize>,
    pub schema: Option<&'static CsvSchema>,
    pub validate_phone_numbers: bool,
}

impl RowValidator {
    pub fn new(file_name: &str, headers: Vec<String>) -> Self {
        let normalized_headers: Vec<String> = headers
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .collect();
        let header_index: HashMap<String, usize> = normalized_headers
            .iter()
            .enumerate()
            .map(|(index, value)| (value.clone(), index))
            .collect();
        let schema = schema_for_file(file_name);
        let validate_phone_numbers = validation_country_code().is_some();

        Self {
            file_name: file_name.to_string(),
            headers,
            normalized_headers,
            header_index,
            schema,
            validate_phone_numbers,
        }
    }

    pub fn validate_row(&self, record: &StringRecord, row_number: u64) -> Vec<ValidationNotice> {
        let mut notices = Vec::new();
        let header_len = self.headers.len();

        if row_number > MAX_ROW_NUMBER {
            notices.push(too_many_rows_notice(&self.file_name, row_number));
            return notices;
        }

        // In default mode, skip empty rows silently (matches Java)
        // In thorough mode, emit empty_row notice
        if record.iter().all(|value| value.trim().is_empty()) {
            if thorough_mode_enabled() {
                notices.push(empty_row_notice(&self.file_name, row_number));
            }
            return notices;
        }

        if record.len() != header_len {
            notices.push(invalid_row_length_notice(
                &self.file_name,
                row_number,
                header_len,
                record.len(),
            ));
        }

        let mut temp_container = NoticeContainer::new();
        if self.file_name.eq_ignore_ascii_case(FARE_PRODUCTS_FILE) {
            validate_currency_amount(
                &self.file_name,
                record,
                &self.header_index,
                row_number,
                &mut temp_container,
            );
        }
        notices.extend(temp_container.into_vec());

        for (col_index, value) in record.iter().enumerate() {
            let header_name = self
                .headers
                .get(col_index)
                .map(|value| value.trim())
                .unwrap_or("");
            let normalized_header = self
                .normalized_headers
                .get(col_index)
                .map(String::as_str)
                .unwrap_or("");
            let is_schema_field = self
                .schema
                .map(|schema| schema.fields.contains(&normalized_header))
                .unwrap_or(false);
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
            if is_schema_field && trimmed.len() < value.len() {
                notices.push(leading_trailing_whitespace_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }

            if trimmed.is_empty() {
                continue;
            }

            if is_schema_field
                && is_id_field(normalized_header)
                && !has_only_printable_ascii(trimmed)
            {
                notices.push(non_ascii_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }

            if is_mixed_case_field(normalized_header) && is_mixed_case_violation(trimmed) {
                notices.push(mixed_case_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }
            if is_language_field(normalized_header) && trimmed.chars().any(|ch| ch.is_uppercase()) {
                notices.push(mixed_case_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }

            if let Some(kind) = enum_kind(normalized_header) {
                let mut temp_container = NoticeContainer::new();
                validate_enum_value(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                    kind,
                    &mut temp_container,
                );
                notices.extend(temp_container.into_vec());
                continue;
            }

            if is_integer_field(normalized_header) {
                if trimmed.parse::<i64>().is_err() {
                    notices.push(invalid_integer_notice(
                        &self.file_name,
                        header_name,
                        row_number,
                        trimmed,
                    ));
                }
                continue;
            }

            if is_float_field(normalized_header) {
                if trimmed.parse::<f64>().is_err() {
                    notices.push(invalid_float_notice(
                        &self.file_name,
                        header_name,
                        row_number,
                        trimmed,
                    ));
                }
                continue;
            }

            if is_date_field(normalized_header) && GtfsDate::parse(trimmed).is_err() {
                notices.push(invalid_date_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_time_field(normalized_header) && GtfsTime::parse(trimmed).is_err() {
                notices.push(invalid_time_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_color_field(normalized_header) && GtfsColor::parse(trimmed).is_err() {
                notices.push(invalid_color_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_timezone_field(normalized_header) && !is_valid_timezone(trimmed) {
                notices.push(invalid_timezone_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_language_field(normalized_header) && !is_valid_language_code(trimmed) {
                notices.push(invalid_language_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_currency_field(normalized_header) && !is_valid_currency_code(trimmed) {
                notices.push(invalid_currency_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_url_field(normalized_header) && !is_valid_url(trimmed) {
                notices.push(invalid_url_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_email_field(normalized_header) && !is_valid_email(trimmed) {
                notices.push(invalid_email_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if self.validate_phone_numbers
                && is_phone_field(normalized_header)
                && !is_valid_phone_number(trimmed)
            {
                notices.push(invalid_phone_notice(
                    &self.file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }
        }
        notices
    }
}

#[allow(dead_code)]
pub fn validate_csv_data(file_name: &str, data: &[u8], notices: &mut NoticeContainer) {
    let data = strip_utf8_bom(data);
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(Trim::Headers)
        .from_reader(data);

    let headers_record = match reader.headers() {
        Ok(headers) => headers.clone(),
        Err(_) => return,
    };
    let headers: Vec<String> = headers_record
        .iter()
        .map(|value| value.to_string())
        .collect();

    let mut header_notices = NoticeContainer::new();
    validate_headers(file_name, &headers, &mut header_notices);
    let has_header_errors = header_notices
        .iter()
        .any(|notice| notice.severity == NoticeSeverity::Error);
    notices.merge(header_notices);
    if has_header_errors {
        return;
    }

    let validator = RowValidator::new(file_name, headers.clone());

    for (index, result) in reader.records().enumerate() {
        let record = match result {
            Ok(record) => record,
            Err(_) => continue,
        };
        let row_number = record
            .position()
            .map(|pos| pos.line())
            .unwrap_or(index as u64 + 2);

        let row_notices = validator.validate_row(&record, row_number);
        for notice in row_notices {
            let is_too_many = notice.code == "too_many_rows";
            notices.push(notice);
            if is_too_many {
                return;
            }
        }
    }
}

#[allow(dead_code)]
fn strip_utf8_bom(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    }
}

pub fn validate_headers(file_name: &str, headers: &[String], notices: &mut NoticeContainer) {
    let schema = schema_for_file(file_name);
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (index, header) in headers.iter().enumerate() {
        let trimmed = header.trim();
        if trimmed.is_empty() {
            notices.push(empty_column_name_notice(file_name, index));
            continue;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if let Some(first_index) = seen.get(&normalized) {
            notices.push(duplicated_column_notice(
                file_name,
                trimmed,
                *first_index,
                index,
            ));
        } else {
            seen.insert(normalized, index);
        }
        if let Some(schema) = schema {
            if !schema
                .fields
                .iter()
                .any(|field| field.eq_ignore_ascii_case(trimmed))
            {
                notices.push(unknown_column_notice(file_name, trimmed, index));
            }
        }
    }
    if let Some(schema) = schema {
        let thorough = thorough_mode_enabled();
        let header_set: HashSet<String> = headers
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .collect();
        for required in schema.required_fields {
            if !header_set.contains(&required.to_ascii_lowercase()) {
                notices.push(missing_required_column_notice(file_name, required));
            }
        }
        if thorough {
            for recommended in schema.recommended_fields {
                if !header_set.contains(&recommended.to_ascii_lowercase()) {
                    notices.push(missing_recommended_column_notice(file_name, recommended));
                }
            }
        }
    }
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

fn leading_trailing_whitespace_notice(
    file: &str,
    field_name: &str,
    row_number: u64,
    value: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "leading_or_trailing_whitespaces",
        NoticeSeverity::Warning,
        "value has leading or trailing whitespace",
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
    notices: &mut NoticeContainer,
) {
    let (Some(&amount_index), Some(&currency_index)) =
        (header_index.get("amount"), header_index.get("currency"))
    else {
        return;
    };

    let amount = record.get(amount_index).unwrap_or("").trim();
    let currency = record.get(currency_index).unwrap_or("").trim();
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
    notices: &mut NoticeContainer,
) {
    match value.parse::<i64>() {
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

fn enum_value_allowed(kind: EnumKind, value: i64) -> bool {
    match kind {
        EnumKind::LocationType => matches!(value, 0 | 1 | 2 | 3 | 4),
        EnumKind::WheelchairBoarding => matches!(value, 0 | 1 | 2),
        EnumKind::RouteType => matches!(value, 0..=7 | 11 | 12 | 100..=1702),
        EnumKind::ContinuousPickupDropOff => matches!(value, 0 | 1 | 2 | 3),
        EnumKind::PickupDropOffType => matches!(value, 0 | 1 | 2 | 3),
        EnumKind::BookingType => matches!(value, 0 | 1 | 2),
        EnumKind::DirectionId => matches!(value, 0 | 1),
        EnumKind::WheelchairAccessible => matches!(value, 0 | 1 | 2),
        EnumKind::BikesAllowed => matches!(value, 0 | 1 | 2),
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

fn is_integer_field(field: &str) -> bool {
    INTEGER_FIELDS.contains(&field)
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

fn is_id_field(field: &str) -> bool {
    field.ends_with("_id") || field == "parent_station"
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

fn is_mixed_case_violation(value: &str) -> bool {
    let tokens: Vec<&str> = value
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|token| !token.is_empty())
        .collect();

    if tokens.is_empty() {
        return false;
    }

    if tokens.len() == 1 {
        let token = tokens[0];
        let token_len = token.chars().count();
        // Java logic: if length > 1, no numbers, and ALL LOWERCASE -> Violation.
        if token_len <= 1 {
            return false;
        }
        if token.chars().any(|ch| ch.is_numeric()) {
            return false;
        }
        // Violation if ALL lowercase (and implies has lowercase chars).
        // Also need to have at least one cased character.
        // For Hebrew/Arabic/CJK, is_lowercase() returns false, so no violation.
        let has_cased = token
            .chars()
            .any(|ch| ch.is_uppercase() || ch.is_lowercase());
        if !has_cased {
            return false;
        }
        return token.chars().all(|ch| ch.is_lowercase());
    }

    let mut has_mixed_case_token = false;
    let mut cased_tokens = 0;

    for token in tokens {
        let token_len = token.chars().count();
        if token_len <= 1 || token.chars().any(|ch| ch.is_numeric()) {
            continue;
        }

        let has_upper = token.chars().any(|ch| ch.is_uppercase());
        let has_lower = token.chars().any(|ch| ch.is_lowercase());

        // Skip tokens without any cased characters (e.g., Hebrew, Arabic, CJK)
        if !has_upper && !has_lower {
            continue;
        }

        cased_tokens += 1;

        if has_upper && has_lower {
            has_mixed_case_token = true;
        }
    }

    // Java logic: if >= 2 cased tokens without numbers, and NO token is mixed case -> Violation.
    cased_tokens >= 2 && !has_mixed_case_token
}

fn trim_java_whitespace(value: &str) -> &str {
    value.trim_matches(|ch| ch <= ' ')
}

fn is_valid_url(value: &str) -> bool {
    Url::parse(value).is_ok()
}

fn is_valid_email(value: &str) -> bool {
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

fn valid_timezones() -> &'static HashSet<String> {
    static TIMEZONES: OnceLock<HashSet<String>> = OnceLock::new();
    TIMEZONES.get_or_init(|| {
        let mut zones: HashSet<String> = IANA_TIMEZONES.iter().map(|s| s.to_string()).collect();
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

#[cfg(test)]
mod tests_whitespaces {
    use super::*;

    #[test]
    fn test_whitespace_checks_schema_aware() {
        let mut notices = NoticeContainer::new();
        // Agency file has schema.
        // Headers: agency_name (known), extra_col (unknown)
        // Values contain whitespace.
        // agency_name -> should trigger warning (schema field)
        // extra_col -> should NOT trigger warning (unknown field)
        let data = b"agency_name,extra_col,agency_url,agency_timezone\n agency 1 , val ,url,tz";
        validate_csv_data("agency.txt", data, &mut notices);

        let whitespace_notices: Vec<_> = notices
            .iter()
            .filter(|n| n.code == "leading_or_trailing_whitespaces")
            .collect();

        assert_eq!(
            whitespace_notices.len(),
            1,
            "Expected 1 whitespace notice (for agency_name), found: {:?}",
            whitespace_notices
        );

        let notice = &whitespace_notices[0];
        // verify context fieldName
        let field_name_json = notice
            .context
            .get("fieldName")
            .expect("Should have fieldName in context");
        assert_eq!(field_name_json.as_str(), Some("agency_name"));
    }
}
