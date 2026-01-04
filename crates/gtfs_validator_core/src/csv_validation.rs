use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use csv::{ReaderBuilder, StringRecord, Trim};
use url::Url;

use crate::csv_schema::schema_for_file;
use crate::feed::FARE_PRODUCTS_FILE;
use crate::validation_context::validation_country_code;
use crate::{NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_model::{GtfsColor, GtfsDate, GtfsTime};

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

pub fn validate_csv_data(file_name: &str, data: &[u8], notices: &mut NoticeContainer) {
    let data = strip_utf8_bom(data);
    let validate_phone_numbers = validation_country_code().is_some();
    let schema = schema_for_file(file_name);
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(Trim::None)
        .from_reader(data);

    let headers_record = match reader.headers() {
        Ok(headers) => headers.clone(),
        Err(_) => return,
    };
    let headers: Vec<String> = headers_record
        .iter()
        .map(|value| value.to_string())
        .collect();
    let normalized_headers: Vec<String> = headers
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();
    let header_index: HashMap<&str, usize> = normalized_headers
        .iter()
        .enumerate()
        .map(|(index, value)| (value.as_str(), index))
        .collect();
    validate_headers(file_name, &headers, notices);

    let header_len = headers.len();
    let line_count = data.split(|&b| b == b'\n').count() as u64;
    let mut last_row_number = 1;
    for (index, result) in reader.records().enumerate() {
        let record = match result {
            Ok(record) => record,
            Err(_) => continue,
        };
        let row_number = record
            .position()
            .map(|pos| pos.line())
            .unwrap_or(index as u64 + 2);

        if row_number > last_row_number + 1 {
            for r in (last_row_number + 1)..row_number {
                notices.push(empty_row_notice(file_name, r));
            }
        }
        last_row_number = row_number;

        if row_number > MAX_ROW_NUMBER {
            notices.push(too_many_rows_notice(file_name, row_number));
            break;
        }

        if record.iter().all(|value| value.trim().is_empty()) {
            notices.push(empty_row_notice(file_name, row_number));
        }

        if record.len() != header_len {
            notices.push(invalid_row_length_notice(
                file_name,
                row_number,
                header_len,
                record.len(),
            ));
        }

        if file_name.eq_ignore_ascii_case(FARE_PRODUCTS_FILE) {
            validate_currency_amount(file_name, &record, &header_index, row_number, notices);
        }

        for (col_index, value) in record.iter().enumerate() {
            let header_name = headers
                .get(col_index)
                .map(|value| value.trim())
                .unwrap_or("");
            let normalized_header = normalized_headers
                .get(col_index)
                .map(String::as_str)
                .unwrap_or("");
            if value.contains('\n') || value.contains('\r') {
                notices.push(new_line_notice(file_name, header_name, row_number, value));
            }
            if value != value.trim() {
                notices.push(leading_trailing_whitespace_notice(
                    file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }
            if value.chars().any(|ch| ch == '\u{FFFD}') {
                notices.push(invalid_character_notice(
                    file_name,
                    header_name,
                    row_number,
                    value,
                ));
            }
            if value
                .chars()
                .any(|ch| !ch.is_ascii() || ch.is_ascii_control())
            {
                notices.push(non_ascii_notice(file_name, header_name, row_number, value));
            }

            let trimmed = value.trim();
            if trimmed.is_empty() {
                if let Some(schema) = schema {
                    if schema.required_fields.contains(&normalized_header) {
                        notices.push(missing_required_field_notice(
                            file_name,
                            header_name,
                            row_number,
                        ));
                    } else if schema.recommended_fields.contains(&normalized_header) {
                        notices.push(missing_recommended_field_notice(
                            file_name,
                            header_name,
                            row_number,
                        ));
                    }
                }
                continue;
            }

            if is_mixed_case_field(normalized_header) && is_mixed_case_violation(trimmed) {
                notices.push(mixed_case_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }
            if is_language_field(normalized_header) && trimmed.chars().any(|ch| ch.is_uppercase()) {
                notices.push(mixed_case_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }

            if let Some(kind) = enum_kind(normalized_header) {
                validate_enum_value(file_name, header_name, row_number, trimmed, kind, notices);
                continue;
            }

            if is_integer_field(normalized_header) {
                if trimmed.parse::<i64>().is_err() {
                    notices.push(invalid_integer_notice(
                        file_name,
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
                        file_name,
                        header_name,
                        row_number,
                        trimmed,
                    ));
                }
                continue;
            }

            if is_date_field(normalized_header) && GtfsDate::parse(trimmed).is_err() {
                notices.push(invalid_date_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_time_field(normalized_header) && GtfsTime::parse(trimmed).is_err() {
                notices.push(invalid_time_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_color_field(normalized_header) && GtfsColor::parse(trimmed).is_err() {
                notices.push(invalid_color_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_timezone_field(normalized_header) && !is_valid_timezone(trimmed) {
                notices.push(invalid_timezone_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_language_field(normalized_header) && !is_valid_language_code(trimmed) {
                notices.push(invalid_language_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_currency_field(normalized_header) && !is_valid_currency_code(trimmed) {
                notices.push(invalid_currency_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_url_field(normalized_header) && !is_valid_url(trimmed) {
                notices.push(invalid_url_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if is_email_field(normalized_header) && !is_valid_email(trimmed) {
                notices.push(invalid_email_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
                continue;
            }

            if validate_phone_numbers
                && is_phone_field(normalized_header)
                && !is_valid_phone_number(trimmed)
            {
                notices.push(invalid_phone_notice(
                    file_name,
                    header_name,
                    row_number,
                    trimmed,
                ));
            }
        }
    }

    if last_row_number < line_count {
        for r in (last_row_number + 1)..=line_count {
            notices.push(empty_row_notice(file_name, r));
        }
    }
}

fn strip_utf8_bom(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    }
}

fn validate_headers(file_name: &str, headers: &[String], notices: &mut NoticeContainer) {
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
        let header_set: HashSet<String> = headers
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .collect();
        for required in schema.required_fields {
            if !header_set.contains(&required.to_ascii_lowercase()) {
                notices.push(missing_required_column_notice(file_name, required));
            }
        }
        for recommended in schema.recommended_fields {
            if !header_set.contains(&recommended.to_ascii_lowercase()) {
                notices.push(missing_recommended_column_notice(file_name, recommended));
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
    notice.field_order = vec!["filename".to_string(), "index".to_string()];
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
        "fieldName".to_string(),
        "filename".to_string(),
        "firstIndex".to_string(),
        "secondIndex".to_string(),
    ];
    notice
}

fn unknown_column_notice(file: &str, field_name: &str, index: usize) -> ValidationNotice {
    let mut notice =
        ValidationNotice::new("unknown_column", NoticeSeverity::Info, "unknown column");
    notice.insert_context_field("fieldName", field_name);
    notice.insert_context_field("filename", file);
    notice.insert_context_field("index", index);
    notice.field_order = vec![
        "fieldName".to_string(),
        "filename".to_string(),
        "index".to_string(),
    ];
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
    notice.field_order = vec!["fieldName".to_string(), "filename".to_string()];
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
    notice.field_order = vec!["fieldName".to_string(), "filename".to_string()];
    notice
}

fn empty_row_notice(file: &str, row_number: u64) -> ValidationNotice {
    let mut notice = ValidationNotice::new("empty_row", NoticeSeverity::Warning, "row is empty");
    notice.insert_context_field("csvRowNumber", row_number);
    notice.insert_context_field("filename", file);
    notice.field_order = vec!["csvRowNumber".to_string(), "filename".to_string()];
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
        "csvRowNumber".to_string(),
        "filename".to_string(),
        "headerCount".to_string(),
        "rowLength".to_string(),
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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
    ];
    notice
}

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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
        "columnName".to_string(),
        "csvRowNumber".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
    notice.field_order = vec!["filename".to_string(), "rowNumber".to_string()];
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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
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
        "csvRowNumber".to_string(),
        "currencyCode".to_string(),
        "fieldName".to_string(),
        "fieldValue".to_string(),
        "filename".to_string(),
    ];
    notice
}

fn validate_currency_amount(
    file: &str,
    record: &StringRecord,
    header_index: &HashMap<&str, usize>,
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
        if token.len() <= 1 {
            return false;
        }
        if token.chars().any(|ch| ch.is_ascii_digit()) {
            return false;
        }
        return token.chars().all(|ch| ch.is_lowercase())
            || token.chars().all(|ch| ch.is_uppercase());
    }

    let mut has_mixed_case = false;
    let mut no_number_tokens = 0;
    for token in tokens {
        if token.len() == 1 || token.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        no_number_tokens += 1;
        let mut has_upper = false;
        let mut has_lower = false;
        for ch in token.chars() {
            if ch.is_uppercase() {
                has_upper = true;
            }
            if ch.is_lowercase() {
                has_lower = true;
            }
        }
        if has_upper && has_lower {
            has_mixed_case = true;
        }
    }

    no_number_tokens >= 2 && !has_mixed_case
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

fn valid_timezones() -> &'static HashSet<String> {
    static TIMEZONES: OnceLock<HashSet<String>> = OnceLock::new();
    TIMEZONES.get_or_init(|| {
        let mut zones = HashSet::new();
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
                if !zones.is_empty() {
                    break;
                }
            }
        }
        zones.insert("UTC".to_string());
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
    notice.file = Some(file.to_string());
    notice.row = Some(row_number);
    notice.field = Some(field_name.to_string());
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "filename".to_string(),
    ];
    notice
}

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
    notice.field_order = vec![
        "csvRowNumber".to_string(),
        "fieldName".to_string(),
        "filename".to_string(),
    ];
    notice
}
