use std::fmt;
use std::io::{BufRead, BufReader, Read};

use csv::{ReaderBuilder, StringRecord, Trim};
use serde::de::DeserializeOwned;

use crate::ValidationNotice;

#[derive(Debug)]
pub struct CsvParseError {
    pub file: String,
    pub row: Option<u64>,
    pub field: Option<String>,
    pub message: String,
    pub char_index: Option<u64>,
    pub column_index: Option<u64>,
    pub line_index: Option<u64>,
    pub parsed_content: Option<String>,
}

impl fmt::Display for CsvParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "csv error in {}", self.file)?;
        if let Some(row) = self.row {
            write!(f, " at row {}", row)?;
        }
        if let Some(field) = &self.field {
            write!(f, " field {}", field)?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for CsvParseError {}

#[derive(Debug, Clone)]
pub struct CsvTable<T> {
    pub headers: Vec<String>,
    pub rows: Vec<T>,
    pub row_numbers: Vec<u64>,
}

impl<T> Default for CsvTable<T> {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}

impl<T> CsvTable<T> {
    pub fn row_number(&self, index: usize) -> u64 {
        self.row_numbers
            .get(index)
            .copied()
            .unwrap_or(index as u64 + 2)
    }
}

pub fn read_csv_from_reader<T, R>(
    reader: R,
    file_name: impl Into<String>,
) -> Result<CsvTable<T>, CsvParseError>
where
    T: DeserializeOwned,
    R: Read,
{
    let (table, errors) = read_csv_from_reader_with_errors(reader, file_name)?;
    if let Some(error) = errors.into_iter().next() {
        return Err(error);
    }
    Ok(table)
}

pub fn read_csv_from_reader_with_errors<T, R>(
    reader: R,
    file_name: impl Into<String>,
) -> Result<(CsvTable<T>, Vec<CsvParseError>), CsvParseError>
where
    T: DeserializeOwned,
    R: Read,
{
    let file = file_name.into();
    let mut reader = BufReader::new(reader);
    if let Err(err) = skip_utf8_bom(&mut reader) {
        return Err(CsvParseError {
            file,
            row: None,
            field: None,
            message: err.to_string(),
            char_index: None,
            column_index: None,
            line_index: None,
            parsed_content: None,
        });
    }

    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(Trim::All)
        .from_reader(reader);

    let headers_record = csv_reader
        .headers()
        .map_err(|err| map_csv_error(&file, None, err))?
        .clone();
    let headers = headers_record
        .iter()
        .map(|value| value.trim().to_string())
        .collect();

    let mut rows = Vec::new();
    let mut row_numbers = Vec::new();
    let mut errors = Vec::new();
    let mut iter = csv_reader.deserialize();
    while let Some(result) = iter.next() {
        match result {
            Ok(record) => {
                // `Position::record()` counts records (header = 0) and is
                // terminator-independent; `line()` lags by one on CRLF feeds.
                let row_number = iter.reader().position().record();
                rows.push(record);
                row_numbers.push(row_number);
            }
            Err(err) => errors.push(map_csv_error(&file, Some(&headers_record), err)),
        }
    }

    Ok((
        CsvTable {
            headers,
            rows,
            row_numbers,
        },
        errors,
    ))
}

/// Sequentially validate and deserialize CSV records in a single scan.
///
/// The row validator sees the original, untrimmed record while serde receives
/// a trimmed copy, matching the parallel reader's behavior. This is used by
/// WASM and non-parallel builds to avoid scanning every CSV once for validation
/// and a second time for deserialization.
pub fn read_csv_from_reader_with_validation<T, R, V>(
    reader: R,
    file_name: impl Into<String>,
    validator: V,
) -> Result<(CsvTable<T>, Vec<CsvParseError>, Vec<ValidationNotice>), CsvParseError>
where
    T: DeserializeOwned,
    R: Read,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice>,
{
    let file = file_name.into();
    let mut buf_reader = BufReader::new(reader);
    if let Err(err) = skip_utf8_bom(&mut buf_reader) {
        return Err(map_io_error(&file, err));
    }

    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(Trim::Headers)
        .from_reader(buf_reader);

    let headers_record = csv_reader
        .headers()
        .map_err(|err| map_csv_error(&file, None, err))?
        .clone();
    let headers = headers_record
        .iter()
        .map(|value| value.trim().to_string())
        .collect();

    let mut rows = Vec::new();
    let mut row_numbers = Vec::new();
    let mut errors = Vec::new();
    let mut notices = Vec::new();
    let mut trimmed_record = csv::StringRecord::new();
    let mut validation_stopped = false;

    for (index, result) in csv_reader.byte_records().enumerate() {
        let record = match result {
            Ok(record) => record,
            Err(err) => {
                errors.push(map_csv_error(&file, Some(&headers_record), err));
                continue;
            }
        };
        let line_number = record
            .position()
            .map(|position| position.record() + 1)
            .unwrap_or((index + 2) as u64);

        let (_, parsed, row_notices) = deserialize_validate_one::<T, _>(
            record,
            line_number,
            &headers_record,
            &file,
            &|record, line| {
                if validation_stopped {
                    Vec::new()
                } else {
                    validator(record, line)
                }
            },
            &mut trimmed_record,
        );
        if row_notices
            .iter()
            .any(|notice| notice.code == "too_many_rows")
        {
            validation_stopped = true;
        }
        notices.extend(row_notices);

        match parsed {
            Ok(row) => {
                rows.push(row);
                row_numbers.push(line_number);
            }
            Err(err) => errors.push(err),
        }
    }

    Ok((
        CsvTable {
            headers,
            rows,
            row_numbers,
        },
        errors,
        notices,
    ))
}

pub(crate) fn map_csv_error(
    file: &str,
    headers: Option<&StringRecord>,
    err: csv::Error,
) -> CsvParseError {
    let position = err.position();
    let row = position.map(|pos| pos.line());
    let field_index = match err.kind() {
        csv::ErrorKind::Deserialize { err, .. } => err.field(),
        csv::ErrorKind::Utf8 { err, .. } => Some(err.field() as u64),
        _ => None,
    };
    let column_index = field_index.map(|index| index as u64);
    let field = field_index.and_then(|index| {
        headers.and_then(|record| {
            let idx = index as usize;
            record.get(idx).map(|value| value.trim().to_string())
        })
    });

    CsvParseError {
        file: file.to_string(),
        row,
        field,
        message: err.to_string(),
        char_index: position.map(|pos| pos.byte()),
        column_index,
        line_index: position.map(|pos| pos.line()),
        parsed_content: position.map(|pos| pos.record().to_string()),
    }
}

pub(crate) fn map_io_error(file: &str, err: std::io::Error) -> CsvParseError {
    CsvParseError {
        file: file.to_string(),
        row: None,
        field: None,
        message: err.to_string(),
        char_index: None,
        column_index: None,
        line_index: None,
        parsed_content: None,
    }
}

pub(crate) fn skip_utf8_bom<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    let buf = reader.fill_buf()?;
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        reader.consume(3);
    }
    Ok(())
}

/// Parallel version of CSV parsing using rayon.
///
/// Record boundary detection runs sequentially (the csv crate must scan the
/// buffer in order to honor quoted fields), but the expensive work — UTF-8
/// validation, field trimming, serde deserialization and per-row validation —
/// is parallelized across the rayon thread pool in row chunks.
///
/// `pool` is the shared string interner. Each worker thread installs a
/// thread-local interner hook (read by `StringId`'s `Deserialize` impl) and
/// re-applies the captured validation context (read by the row validator)
/// before processing its chunk.
#[cfg(feature = "parallel")]
pub fn read_csv_from_reader_parallel<T, R, V>(
    reader: R,
    file_name: impl Into<String>,
    validator: V,
    pool: &crate::StringPool,
) -> Result<(CsvTable<T>, Vec<CsvParseError>, Vec<ValidationNotice>), CsvParseError>
where
    T: DeserializeOwned + Send,
    R: Read,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice> + Sync,
{
    let file = file_name.into();
    let mut buf_reader = BufReader::new(reader);
    if let Err(err) = skip_utf8_bom(&mut buf_reader) {
        return Err(map_io_error(&file, err));
    }

    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(Trim::Headers)
        .from_reader(buf_reader);

    let headers_record = csv_reader
        .headers()
        .map_err(|err| map_csv_error(&file, None, err))?
        .clone();
    let headers: Vec<String> = headers_record
        .iter()
        .map(|value| value.trim().to_string())
        .collect();

    // Collect byte records with their line numbers sequentially.
    // The csv crate's iterator handles record boundary detection (quote-aware).
    let mut raw_records: Vec<(u64, csv::ByteRecord)> = Vec::new();
    let mut scan_errors: Vec<CsvParseError> = Vec::new();

    for (index, result) in csv_reader.byte_records().enumerate() {
        match result {
            Ok(record) => {
                let line_number = record
                    .position()
                    .map(|p| p.record() + 1)
                    .unwrap_or((index + 2) as u64);
                raw_records.push((line_number, record));
            }
            Err(err) => {
                scan_errors.push(map_csv_error(&file, Some(&headers_record), err));
            }
        }
    }

    // Capture the validation context so each worker can re-apply it.
    let ctx = crate::validation_context::ValidationContextState::capture();

    let processed = deserialize_validate_records::<T, _>(
        raw_records,
        &headers_record,
        &file,
        &validator,
        pool,
        &ctx,
    );

    let mut rows = Vec::with_capacity(processed.len());
    let mut row_numbers = Vec::with_capacity(processed.len());
    let mut errors = scan_errors;
    let mut all_notices = Vec::new();

    for (line_number, result, row_notices) in processed {
        all_notices.extend(row_notices);
        match result {
            Ok(record) => {
                rows.push(record);
                row_numbers.push(line_number);
            }
            Err(err) => {
                errors.push(err);
            }
        }
    }

    Ok((
        CsvTable {
            headers,
            rows,
            row_numbers,
        },
        errors,
        all_notices,
    ))
}

/// Deserialize + validate a batch of byte records in parallel, preserving the
/// original record order.
///
/// Work is split into row chunks; each rayon worker installs the thread-local
/// interner hook (read by `StringId::deserialize`) and re-applies the captured
/// validation context once per chunk. `chunks` on an indexed parallel iterator
/// yields results in order, so the flattened output needs no post-sort.
///
/// Used both by the in-memory [`read_csv_from_reader_parallel`] and by the
/// streaming loader, so the per-record semantics stay identical.
#[cfg(feature = "parallel")]
pub(crate) fn deserialize_validate_records<T, V>(
    records: Vec<(u64, csv::ByteRecord)>,
    headers: &csv::StringRecord,
    file: &str,
    validator: &V,
    pool: &crate::StringPool,
    ctx: &crate::validation_context::ValidationContextState,
) -> Vec<(u64, Result<T, CsvParseError>, Vec<ValidationNotice>)>
where
    T: DeserializeOwned + Send,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice> + Sync,
{
    use rayon::prelude::*;

    // Rows handed to a single worker task. Large enough to amortize the
    // per-chunk thread-local setup, small enough to keep every core busy.
    const PARALLEL_CHUNK_ROWS: usize = 8192;

    let nested: Vec<Vec<(u64, Result<T, CsvParseError>, Vec<ValidationNotice>)>> = records
        .into_par_iter()
        .chunks(PARALLEL_CHUNK_ROWS)
        .map(|chunk| {
            // Install thread-local hooks for this worker thread. Idempotent and
            // cheap; re-applied per chunk. The interner points at the shared pool.
            let chunk_pool = pool.clone();
            let local_intern_cache = std::cell::RefCell::new(rustc_hash::FxHashMap::<
                compact_str::CompactString,
                gtfs_guru_model::StringId,
            >::default());
            let _interner_guard = gtfs_guru_model::set_thread_local_interner_scoped(move |s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return gtfs_guru_model::StringId(0);
                }
                let key = compact_str::CompactString::new(trimmed);
                if let Some(id) = local_intern_cache.borrow().get(&key) {
                    return *id;
                }
                let id = chunk_pool.intern(trimmed);
                local_intern_cache.borrow_mut().insert(key, id);
                id
            });
            let _ctx_guards = ctx.apply();

            // Reused across rows in this chunk to avoid a per-row allocation.
            let mut trimmed_record = csv::StringRecord::new();

            let mut out = Vec::with_capacity(chunk.len());
            for (line_number, record) in chunk {
                out.push(deserialize_validate_one::<T, V>(
                    record,
                    line_number,
                    headers,
                    file,
                    validator,
                    &mut trimmed_record,
                ));
            }
            out
        })
        .collect();

    let mut flat = Vec::with_capacity(nested.iter().map(Vec::len).sum());
    for chunk in nested {
        flat.extend(chunk);
    }
    flat
}

/// Validate (untrimmed) and deserialize (trimmed) a single record.
///
/// The common case — valid UTF-8 — converts the byte record in place. Invalid
/// bytes are replaced with U+FFFD per field so the row validator can flag them,
/// matching the behavior of decoding the whole buffer up front.
fn deserialize_validate_one<T, V>(
    record: csv::ByteRecord,
    line_number: u64,
    headers: &csv::StringRecord,
    file: &str,
    validator: &V,
    trimmed_record: &mut csv::StringRecord,
) -> (u64, Result<T, CsvParseError>, Vec<ValidationNotice>)
where
    T: DeserializeOwned,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice>,
{
    let string_record = match csv::StringRecord::from_byte_record(record) {
        Ok(string_record) => string_record,
        Err(utf8_err) => {
            let byte_record = utf8_err.into_byte_record();
            let mut lossy = csv::StringRecord::new();
            for field in byte_record.iter() {
                lossy.push_field(&String::from_utf8_lossy(field));
            }
            lossy
        }
    };

    // The untrimmed record is what the row validator inspects (whitespace,
    // embedded newlines, invalid characters, ...).
    let notices = validator(&string_record, line_number);

    // Most production feeds do not have surrounding field whitespace. Avoid
    // copying every field into a second StringRecord on that hot path, while
    // preserving the existing trim-before-deserialize behavior for dirty rows.
    let needs_trimming = string_record
        .iter()
        .any(|field| field.len() != field.trim().len());
    let result = if needs_trimming {
        trimmed_record.clear();
        for field in string_record.iter() {
            trimmed_record.push_field(field.trim());
        }
        trimmed_record.deserialize(Some(headers))
    } else {
        string_record.deserialize(Some(headers))
    }
    .map_err(|err| map_byte_record_error(file, Some(headers), line_number, err));

    (line_number, result, notices)
}

/// Map deserialization error from ByteRecord (used in parallel mode)
fn map_byte_record_error(
    file: &str,
    headers: Option<&StringRecord>,
    line_number: u64,
    err: csv::Error,
) -> CsvParseError {
    let field_index = match err.kind() {
        csv::ErrorKind::Deserialize { err, .. } => err.field(),
        csv::ErrorKind::Utf8 { err, .. } => Some(err.field() as u64),
        _ => None,
    };
    let column_index = field_index.map(|index| index as u64);
    let field = field_index.and_then(|index| {
        headers.and_then(|record| {
            let idx = index as usize;
            record.get(idx).map(|value| value.trim().to_string())
        })
    });

    CsvParseError {
        file: file.to_string(),
        row: Some(line_number),
        field,
        message: err.to_string(),
        char_index: None,
        column_index,
        line_index: Some(line_number),
        parsed_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::cell::Cell;

    #[derive(Debug, Deserialize)]
    struct ExampleRow {
        a: i32,
        b: i32,
    }

    #[test]
    fn reads_headers_and_rows() {
        let data = "a,b\n1,2\n3,4\n";
        let table =
            read_csv_from_reader::<ExampleRow, _>(data.as_bytes(), "test.csv").expect("parse csv");

        assert_eq!(table.headers, vec!["a", "b"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].a, 1);
        assert_eq!(table.rows[1].b, 4);
        assert_eq!(table.row_numbers, vec![2, 3]);
    }

    #[test]
    fn row_numbers_are_line_numbers_with_crlf() {
        // Feeds exported on Windows use CRLF. The row number must still be the
        // physical line, matching the canonical validator.
        let data = "a,b\r\n1,2\r\n3,4\r\n";
        let table =
            read_csv_from_reader::<ExampleRow, _>(data.as_bytes(), "crlf.csv").expect("parse csv");

        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.row_numbers, vec![2, 3]);
    }

    #[test]
    fn reports_field_on_parse_error() {
        let data = "a,b\n1,boom\n";
        let err = read_csv_from_reader::<ExampleRow, _>(data.as_bytes(), "bad.csv")
            .expect_err("expected parse error");

        assert_eq!(err.file, "bad.csv");
        assert!(err.row.is_some());
        assert_eq!(err.field.as_deref(), Some("b"));
    }

    #[test]
    fn collects_row_errors_without_aborting() {
        let data = "a,b\n1,2\n3,boom\n4,5\n";
        let (table, errors) =
            read_csv_from_reader_with_errors::<ExampleRow, _>(data.as_bytes(), "rows.csv")
                .expect("parse csv");

        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].a, 1);
        assert_eq!(table.rows[1].b, 5);
        assert_eq!(table.row_numbers, vec![2, 4]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field.as_deref(), Some("b"));
    }

    #[test]
    fn validates_and_deserializes_in_one_scan() {
        let data = "a,b\n 1 ,2\n3,boom\n4,5\n";
        let validated_rows = Cell::new(0usize);
        let (table, errors, notices) = read_csv_from_reader_with_validation::<ExampleRow, _, _>(
            data.as_bytes(),
            "rows.csv",
            |record, _| {
                validated_rows.set(validated_rows.get() + 1);
                if record.get(0) == Some(" 1 ") {
                    vec![crate::ValidationNotice::new(
                        "saw_untrimmed_value",
                        crate::NoticeSeverity::Info,
                        "row validator receives the original field",
                    )]
                } else {
                    Vec::new()
                }
            },
        )
        .expect("parse csv");

        assert_eq!(validated_rows.get(), 3);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].a, 1);
        assert_eq!(table.rows[1].b, 5);
        assert_eq!(table.row_numbers, vec![2, 4]);
        assert_eq!(errors.len(), 1);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].code, "saw_untrimmed_value");
    }

    #[test]
    fn stops_row_validation_after_too_many_rows_notice() {
        let data = "a,b\n1,2\n3,4\n";
        let validated_rows = Cell::new(0usize);
        let (table, errors, notices) = read_csv_from_reader_with_validation::<ExampleRow, _, _>(
            data.as_bytes(),
            "rows.csv",
            |_, _| {
                validated_rows.set(validated_rows.get() + 1);
                vec![crate::ValidationNotice::new(
                    "too_many_rows",
                    crate::NoticeSeverity::Error,
                    "too many rows",
                )]
            },
        )
        .expect("parse csv");

        assert_eq!(validated_rows.get(), 1);
        assert_eq!(table.rows.len(), 2);
        assert!(errors.is_empty());
        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn strips_utf8_bom_from_headers() {
        let data = b"\xEF\xBB\xBFa,b\n9,10\n";
        let table =
            read_csv_from_reader::<ExampleRow, _>(data.as_slice(), "bom.csv").expect("parse csv");

        assert_eq!(table.headers, vec!["a", "b"]);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].a, 9);
    }

    #[test]
    fn deserializes_v8_fields() {
        let agencies = read_csv_from_reader::<gtfs_guru_model::Agency, _>(
            b"agency_name,agency_url,agency_timezone,cemv_support\nA,https://example.com,UTC,1\n"
                .as_slice(),
            "agency.txt",
        )
        .unwrap();
        assert_eq!(
            agencies.rows[0].cemv_support,
            Some(gtfs_guru_model::ContactlessEmvSupport::Supported)
        );

        let trips = read_csv_from_reader::<gtfs_guru_model::Trip, _>(
            b"route_id,service_id,trip_id,cars_allowed,safe_duration_factor,safe_duration_offset\nR,S,T,2,1.5,30\n"
                .as_slice(),
            "trips.txt",
        )
        .unwrap();
        assert_eq!(
            trips.rows[0].cars_allowed,
            Some(gtfs_guru_model::CarsAllowed::NotAllowed)
        );
        assert_eq!(trips.rows[0].safe_duration_factor, Some(1.5));
        assert_eq!(trips.rows[0].safe_duration_offset, Some(30.0));

        let stops = read_csv_from_reader::<gtfs_guru_model::Stop, _>(
            b"stop_id,stop_access\nS,0\n".as_slice(),
            "stops.txt",
        )
        .unwrap();
        assert_eq!(
            stops.rows[0].stop_access,
            Some(gtfs_guru_model::StopAccess::AccessibleViaPathways)
        );

        let pathways = read_csv_from_reader::<gtfs_guru_model::Pathway, _>(
            b"pathway_id,from_stop_id,to_stop_id,pathway_mode,is_bidirectional,stair_count\n\
              P,E,N,2,0,-34\n"
                .as_slice(),
            "pathways.txt",
        )
        .unwrap();
        assert_eq!(pathways.rows.len(), 1);
        assert_eq!(pathways.rows[0].stair_count, Some(-34));
    }
    #[test]
    #[cfg(feature = "parallel")]
    fn reads_headers_and_rows_parallel() {
        let data = "a,b\n1,2\n3,4\n5,6\n";
        let pool = crate::StringPool::new();
        let (table, errors, notices) = read_csv_from_reader_parallel::<ExampleRow, _, _>(
            data.as_bytes(),
            "test.csv",
            |_, _| Vec::new(),
            &pool,
        )
        .expect("parse csv");

        assert!(errors.is_empty());
        assert!(notices.is_empty());
        assert_eq!(table.headers, vec!["a", "b"]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0].a, 1);
        assert_eq!(table.rows[1].a, 3);
        assert_eq!(table.rows[2].a, 5);
        assert_eq!(table.row_numbers, vec![2, 3, 4]);
    }
}
