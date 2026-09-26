//! The same feed must yield the same notices whichever way it is read: a zip
//! on disk, a directory, or zip bytes in memory (the WASM and MCP path). Each
//! fixture is also pinned to what the canonical validator (v8.0.1) reports.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtfs_guru_core::{engine, rules::default_runner, GtfsInput, ValidationNotice};
use zip::write::FileOptions;
use zip::ZipWriter;

const AGENCY: &str = "agency_id,agency_name,agency_url,agency_timezone\n\
                      a1,Agency,https://example.com,America/New_York\n";
const ROUTES: &str = "route_id,agency_id,route_short_name,route_type\nr1,a1,R1,3\n";
const TRIPS: &str = "route_id,service_id,trip_id\nr1,s1,t1\n";
const STOPS: &str = "stop_id,stop_name,stop_lat,stop_lon\n\
                     st1,One,40.7128,-74.0060\nst2,Two,40.7138,-74.0050\n";
const STOP_TIMES: &str = "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
                          t1,08:00:00,08:00:00,st1,1\nt1,08:10:00,08:10:00,st2,2\n";
const CALENDAR: &str =
    "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
                        s1,1,1,1,1,1,0,0,20250101,20251231\n";

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
}

/// The base feed with `overrides` replacing (or adding) files.
fn feed_files(overrides: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    let mut files: BTreeMap<String, Vec<u8>> = [
        ("agency.txt", AGENCY),
        ("routes.txt", ROUTES),
        ("trips.txt", TRIPS),
        ("stops.txt", STOPS),
        ("stop_times.txt", STOP_TIMES),
        ("calendar.txt", CALENDAR),
    ]
    .into_iter()
    .map(|(name, data)| (name.to_string(), data.as_bytes().to_vec()))
    .collect();
    for (name, data) in overrides {
        files.insert(name.to_string(), data.to_vec());
    }
    files.into_iter().collect()
}

fn zip_bytes(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        for (name, data) in files {
            zip.start_file(name, FileOptions::default())
                .expect("zip entry");
            zip.write_all(data).expect("zip data");
        }
        zip.finish().expect("finish zip");
    }
    buffer.into_inner()
}

/// A comparable rendering of a notice: code, severity and context.
fn key(notice: &ValidationNotice) -> String {
    format!(
        "{} {:?} {} {:?} {:?} {:?}",
        notice.code,
        notice.severity,
        serde_json::to_string(&notice.context).expect("context"),
        notice.file,
        notice.row,
        notice.field
    )
}

/// Validate `files` three ways, check the notices agree, and return them.
fn validate_everywhere(files: &[(String, Vec<u8>)]) -> Vec<ValidationNotice> {
    let _date = gtfs_guru_core::set_validation_date(Some(
        chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
    ));
    let runner = default_runner();
    let bytes = zip_bytes(files);

    let dir = temp_dir("gtfs_paths");
    fs::create_dir_all(&dir).expect("dir");
    let zip_path = dir.join("feed.zip");
    fs::write(&zip_path, &bytes).expect("write zip");
    let feed_dir = dir.join("feed");
    fs::create_dir_all(&feed_dir).expect("feed dir");
    for (name, data) in files {
        fs::write(feed_dir.join(name), data).expect("write file");
    }

    let render = |notices: &gtfs_guru_core::NoticeContainer| {
        let mut keys: Vec<String> = notices.iter().map(key).collect();
        keys.sort();
        keys
    };
    let from_zip = engine::validate_input(&GtfsInput::from_path(&zip_path).unwrap(), &runner);
    let from_dir = engine::validate_input(&GtfsInput::from_path(&feed_dir).unwrap(), &runner);
    let from_bytes = engine::validate_bytes(&bytes, &runner);
    fs::remove_dir_all(&dir).ok();

    let zip_keys = render(&from_zip.notices);
    assert_eq!(zip_keys, render(&from_dir.notices), "zip vs directory");
    assert_eq!(
        zip_keys,
        render(&from_bytes.notices),
        "zip vs in-memory bytes"
    );
    from_zip.notices.iter().cloned().collect()
}

fn find<'a>(notices: &'a [ValidationNotice], code: &str) -> Vec<&'a ValidationNotice> {
    notices
        .iter()
        .filter(|notice| notice.code == code)
        .collect()
}

fn row_of(notice: &ValidationNotice) -> u64 {
    notice
        .row
        .or_else(|| notice.context.get("csvRowNumber").and_then(|v| v.as_u64()))
        .expect("row number")
}

#[test]
fn rows_are_numbered_by_physical_line_on_every_path() {
    // Java: a blank line, a comment and a CRLF multi-line value all count;
    // the bad latitude sits on physical line 7.
    let stops = b"stop_id,stop_name,stop_lat,stop_lon\r\n\r\nst1,\"One\r\nStop\",40.7128,-74.0060\r\n#note\r\nst2,Two,40.7138,-74.0050\r\nbad,Bad,999,-74\r\n";
    let notices = validate_everywhere(&feed_files(&[("stops.txt", stops)]));
    let new_line = find(&notices, "new_line_in_value");
    assert_eq!(new_line.len(), 1);
    assert_eq!(row_of(new_line[0]), 4);
    let out_of_range = find(&notices, "number_out_of_range");
    assert_eq!(out_of_range.len(), 1);
    assert_eq!(row_of(out_of_range[0]), 7);
}

#[test]
fn empty_files_are_reported_the_same_way() {
    for data in [
        &b"\n"[..],
        b"\xEF\xBB\xBF",
        b"\xEF\xBB\xBF\n",
        b"\n\n",
        b"#only a comment\n",
    ] {
        let notices = validate_everywhere(&feed_files(&[("calendar_dates.txt", data)]));
        let empty = find(&notices, "empty_file");
        assert_eq!(empty.len(), 1, "{data:?}");
        assert!(
            find(&notices, "missing_required_column").is_empty(),
            "{data:?}"
        );
    }
}

#[test]
fn header_errors_suppress_row_notices_on_every_path() {
    // A duplicated column: Java reads no row, so the bad date goes unreported.
    let calendar = b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date,monday\ns1,1,1,1,1,1,0,0,2025x,20251231,1\n";
    let notices = validate_everywhere(&feed_files(&[("calendar.txt", calendar)]));
    assert_eq!(find(&notices, "duplicated_column").len(), 1);
    assert!(find(&notices, "invalid_date").is_empty());
}

#[test]
fn invalid_utf8_in_a_header_is_decoded_lossily() {
    let stops = b"stop_id,stop_name,stop_lat,stop_lon,x\xff\nst1,One,40.7128,-74.0060,a\nst2,Two,40.7138,-74.0050,b\n";
    let notices = validate_everywhere(&feed_files(&[("stops.txt", stops)]));
    let unknown = find(&notices, "unknown_column");
    assert_eq!(unknown.len(), 1);
    assert_eq!(
        unknown[0].context.get("fieldName").and_then(|v| v.as_str()),
        Some("x\u{fffd}")
    );
}

#[test]
fn byte_order_mark_and_surrounding_whitespace_on_every_path() {
    let stops = b"\xEF\xBB\xBF stop_id , stop_name,stop_lat,stop_lon\nst1,One,40.7128,-74.0060\nst2,Two,40.7138,-74.0050\n";
    let notices = validate_everywhere(&feed_files(&[("stops.txt", stops)]));
    assert!(find(&notices, "unknown_column").is_empty());
    assert!(find(&notices, "missing_required_column").is_empty());
}

#[test]
fn an_over_long_value_stops_the_file_on_every_path() {
    let mut stops = b"stop_id,stop_name,stop_lat,stop_lon\nst1,One,40.7128,-74.0060\n".to_vec();
    stops.extend_from_slice(b"st2,");
    stops.extend(std::iter::repeat_n(b'x', 5000));
    stops.extend_from_slice(b",40.7138,-74.0050\nbad,Bad,999,-74\n");
    let notices = validate_everywhere(&feed_files(&[("stops.txt", &stops)]));
    let failed = find(&notices, "csv_parsing_failed");
    assert_eq!(failed.len(), 1);
    let context = &failed[0].context;
    assert_eq!(
        context.get("charIndex").and_then(|v| v.as_u64()),
        Some(65 + 4097)
    );
    assert_eq!(context.get("lineIndex").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(context.get("columnIndex").and_then(|v| v.as_u64()), Some(1));
    // Rows after the failure are never read.
    assert!(find(&notices, "number_out_of_range").is_empty());
}

#[test]
fn row_errors_skip_the_validators_that_need_the_table() {
    // stop_sequence past 32 bits: Java reports invalid_integer, marks
    // stop_times.txt unparsable and skips every validator that reads it.
    let stop_times = b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,08:00:00,08:00:00,st1,1\nt1,08:10:00,08:10:00,st2,3000000000\nt1,07:00:00,07:00:00,st1,2\n";
    let notices = validate_everywhere(&feed_files(&[("stop_times.txt", stop_times)]));
    let invalid = find(&notices, "invalid_integer");
    assert_eq!(invalid.len(), 1);
    assert_eq!(row_of(invalid[0]), 3);
    assert!(find(
        &notices,
        "stop_time_with_arrival_before_previous_departure_time"
    )
    .is_empty());
    assert!(find(&notices, "unsorted_stop_times").is_empty());
}

#[test]
fn a_row_of_empty_values_reports_each_required_field() {
    let routes = b"route_id,agency_id,route_short_name,route_type\nr1,a1,R1,3\n,,,\n";
    let notices = validate_everywhere(&feed_files(&[("routes.txt", routes)]));
    let missing: Vec<(String, u64)> = find(&notices, "missing_required_field")
        .into_iter()
        .map(|notice| (notice.field.clone().unwrap_or_default(), row_of(notice)))
        .collect();
    assert_eq!(
        missing,
        vec![("route_id".to_string(), 3), ("route_type".to_string(), 3)]
    );
}

#[test]
fn header_names_are_case_sensitive() {
    let stops =
        b"stop_id,stop_name,Stop_Lat,stop_lon\nst1,One,999,-74.0060\nst2,Two,40.7138,-74.0050\n";
    let notices = validate_everywhere(&feed_files(&[("stops.txt", stops)]));
    assert_eq!(find(&notices, "unknown_column").len(), 1);
    assert!(find(&notices, "number_out_of_range").is_empty());
}

#[test]
fn only_java_whitespace_is_trimmed_from_ids() {
    // `route_id="r1\u{a0}"` does not reference `r1`.
    let trips = "route_id,service_id,trip_id\n\"r1\u{a0}\",s1,t1\n".as_bytes();
    let notices = validate_everywhere(&feed_files(&[("trips.txt", trips)]));
    let fk: Vec<_> = find(&notices, "foreign_key_violation")
        .into_iter()
        .filter(|notice| {
            notice
                .context
                .get("childFieldName")
                .and_then(|v| v.as_str())
                == Some("route_id")
        })
        .collect();
    assert_eq!(fk.len(), 1);
}
