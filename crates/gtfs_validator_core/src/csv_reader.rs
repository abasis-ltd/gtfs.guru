use std::fmt;
use std::io::Read;

use csv::{ByteRecord, ReaderBuilder, StringRecord, Terminator, Trim};
use serde::de::DeserializeOwned;

use crate::csv_univocity::{FieldTooLong, NormalizingReader, DEFAULT_MAX_CHARS_PER_COLUMN};
use crate::{NoticeContainer, NoticeSeverity, ValidationNotice};

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

/// Java's `String.trim()`: strips every char `<= U+0020` from both ends, and
/// nothing else (no-break and other Unicode spaces are kept).
pub use gtfs_guru_model::java_trim;

/// The column limit the canonical validator applies to `file_name`: univocity's
/// default everywhere but `areas.txt`, whose descriptor lifts it.
pub fn max_chars_per_column(file_name: &str) -> Option<usize> {
    if file_name.eq_ignore_ascii_case(crate::feed::AREAS_FILE) {
        None
    } else {
        Some(DEFAULT_MAX_CHARS_PER_COLUMN)
    }
}

/// The csv crate configured to read what [`crate::csv_univocity`] produces:
/// `\n` is the only record terminator (univocity's line separator), so a bare
/// `\r` stays inside its value, and every record carries its own `\n`.
pub(crate) fn csv_reader_builder() -> ReaderBuilder {
    let mut builder = ReaderBuilder::new();
    builder
        .has_headers(true)
        .flexible(true)
        .trim(Trim::None)
        .terminator(Terminator::Any(b'\n'))
        .buffer_capacity(64 * 1024);
    builder
}

/// Why a scan stopped early.
#[derive(Debug)]
pub(crate) enum ScanError {
    /// A value over the column limit: univocity's `TextParsingException`.
    TooLong(FieldTooLong),
    /// The underlying reader failed.
    Io(std::io::Error),
}

impl ScanError {
    fn from_csv(err: csv::Error) -> Self {
        if let Some(too_long) = FieldTooLong::from_csv_error(&err) {
            return ScanError::TooLong(too_long.clone());
        }
        match err.into_kind() {
            csv::ErrorKind::Io(io_err) => ScanError::Io(io_err),
            other => ScanError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{:?}", other),
            )),
        }
    }
}

/// Reads records the way the canonical validator's `CsvFile` does, and numbers
/// them the same way: a row is the physical line on which its record ends,
/// with the header on line 1, so blank lines, comments and multi-line values
/// all count.
pub(crate) struct RecordScanner<R: Read> {
    /// The reader until the header is read; then the record iterator, which
    /// hands out tightly sized records (a record built up from empty would
    /// carry the slack of every buffer doubling, for millions of rows).
    reader: Option<csv::Reader<NormalizingReader<R>>>,
    records: Option<csv::ByteRecordsIntoIter<NormalizingReader<R>>>,
}

impl<R: Read> RecordScanner<R> {
    /// A scanner honouring the calling thread's `--thorough` setting.
    pub(crate) fn new(reader: R, max_chars: Option<usize>) -> Self {
        Self::with_options(
            reader,
            max_chars,
            crate::validation_context::thorough_mode_enabled(),
        )
    }

    /// `thorough` keeps whitespace-only lines as rows, for `empty_row`.
    pub(crate) fn with_options(reader: R, max_chars: Option<usize>, thorough: bool) -> Self {
        let normalizer = crate::csv_univocity::Normalizer::with_max_chars(max_chars)
            .keep_whitespace_rows(thorough);
        Self {
            reader: Some(
                csv_reader_builder()
                    .from_reader(NormalizingReader::with_normalizer(reader, normalizer)),
            ),
            records: None,
        }
    }

    /// The header, decoded lossily, or `None` when the input holds no record
    /// at all -- the canonical validator's `empty_file`. Call it first.
    pub(crate) fn headers(&mut self) -> Result<Option<Vec<String>>, ScanError> {
        let Some(reader) = self.reader.as_mut() else {
            return Ok(None);
        };
        let headers = reader.byte_headers().map_err(ScanError::from_csv)?;
        if headers.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            headers
                .iter()
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect(),
        ))
    }

    /// The next data record and its row number.
    pub(crate) fn next_record(&mut self) -> Result<Option<(u64, ByteRecord)>, ScanError> {
        if self.records.is_none() {
            let Some(reader) = self.reader.take() else {
                return Ok(None);
            };
            self.records = Some(reader.into_byte_records());
        }
        let records = self.records.as_mut().expect("iterator set above");
        match records.next() {
            None => Ok(None),
            Some(Err(err)) => Err(ScanError::from_csv(err)),
            // Every normalised record ends with the `\n` just consumed, so the
            // line counter sits one past the record's last line.
            Some(Ok(record)) => Ok(Some((
                records.reader().position().line().saturating_sub(1),
                record,
            ))),
        }
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
    let (table, errors, _) =
        read_csv_from_reader_with_validation(reader, file_name, |_, _| Vec::new())?;
    Ok((table, errors))
}

/// Sequentially validate and deserialize CSV records in a single scan.
///
/// The row validator sees the record as univocity would hand it to the
/// canonical validator (see [`crate::csv_univocity`]); serde receives a copy
/// with Java-trimmed fields. Rows the validator rejects -- an error notice, a
/// wrong length, an empty row -- are left out of the table, as the canonical
/// validator does not build an entity for them.
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
    let mut scanner = RecordScanner::new(reader, max_chars_per_column(&file));
    let headers = scanner
        .headers()
        .map_err(|err| scan_error_to_parse_error(&file, err, None))?
        .unwrap_or_default();
    let header_record = trimmed_header_record(&headers);

    let mut rows = Vec::new();
    let mut row_numbers = Vec::new();
    let mut errors = Vec::new();
    let mut notices = Vec::new();
    let mut trimmed_record = csv::StringRecord::new();

    loop {
        let (line_number, record) = match scanner.next_record() {
            Ok(Some(next)) => next,
            Ok(None) => break,
            Err(err) => {
                errors.push(scan_error_to_parse_error(&file, err, Some(&headers)));
                break;
            }
        };
        let (_, parsed, row_notices) = deserialize_validate_one::<T, _>(
            record,
            line_number,
            &header_record,
            &file,
            &validator,
            &mut trimmed_record,
        );
        let stop = row_notices
            .iter()
            .any(|notice| notice.code == "too_many_rows");
        notices.extend(row_notices);
        match parsed {
            Some(Ok(row)) => {
                rows.push(row);
                row_numbers.push(line_number);
            }
            Some(Err(err)) => errors.push(err),
            None => {}
        }
        if stop {
            break;
        }
    }

    Ok((
        CsvTable {
            headers: header_record.iter().map(str::to_string).collect(),
            rows,
            row_numbers,
        },
        errors,
        notices,
    ))
}

fn trimmed_header_record(headers: &[String]) -> StringRecord {
    let mut record = StringRecord::new();
    for header in headers {
        record.push_field(java_trim(header));
    }
    record
}

fn scan_error_to_parse_error(
    file: &str,
    err: ScanError,
    headers: Option<&[String]>,
) -> CsvParseError {
    match err {
        ScanError::TooLong(too_long) => field_too_long_error(file, &too_long, headers),
        ScanError::Io(err) => map_io_error(file, err),
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

/// Univocity's `TextParsingException` for a value over the column limit, as the
/// canonical validator reports it in `csv_parsing_failed`. `headers` is `None`
/// while the header itself is being read, where univocity has none to print.
pub(crate) fn field_too_long_error(
    file: &str,
    err: &FieldTooLong,
    headers: Option<&[String]>,
) -> CsvParseError {
    let headers_part = headers
        .map(|headers| {
            let names: Vec<&str> = headers
                .iter()
                .map(|header| {
                    let trimmed = java_trim(header);
                    if trimmed.is_empty() {
                        "null"
                    } else {
                        trimmed
                    }
                })
                .collect();
            format!("headers=[{}], ", names.join(", "))
        })
        .unwrap_or_default();
    let message = format!(
        "Length of parsed input ({}) exceeds the maximum number of characters defined in your \
parser settings ({}). \nParser Configuration: CsvParserSettings:\n\tAuto configuration enabled=true\n\
\tAuto-closing enabled=true\n\tAutodetect column delimiter=false\n\tAutodetect quotes=false\n\
\tColumn reordering enabled=true\n\tDelimiters for detection=null\n\tEmpty value=null\n\
\tEscape unquoted values=false\n\tHeader extraction enabled=true\n\tHeaders=null\n\
\tIgnore leading whitespaces=true\n\tIgnore leading whitespaces in quotes=false\n\
\tIgnore trailing whitespaces=true\n\tIgnore trailing whitespaces in quotes=false\n\
\tInput buffer size=1048576\n\tInput reading on separate thread=true\n\
\tKeep escape sequences=false\n\tKeep quotes=false\n\tLength of content displayed on error=-1\n\
\tLine separator detection enabled=false\n\tMaximum number of characters per column={}\n\
\tMaximum number of columns=512\n\tNormalize escaped line separators=true\n\tNull value=null\n\
\tNumber of records to read=all\n\tProcessor=none\n\tRestricting data in exceptions=false\n\
\tRowProcessor error handler=null\n\tSelected fields=none\n\tSkip bits as whitespace=true\n\
\tSkip empty lines=true\n\tUnescaped quote handling=nullFormat configuration:\n\tCsvFormat:\n\
\t\tComment character=#\n\t\tField delimiter=,\n\t\tLine separator (normalized)=\\n\n\
\t\tLine separator sequence=\\n\n\t\tQuote character=\"\n\t\tQuote escape character=\"\n\
\t\tQuote escape escape character=null\nInternal state when error was thrown: line={}, \
column={}, record={}, charIndex={}, {}content parsed={}",
        err.max_chars + 1,
        err.max_chars,
        err.max_chars,
        err.line_index,
        err.column_index,
        err.record_index,
        err.char_index,
        headers_part,
        err.parsed_content,
    );
    CsvParseError {
        file: file.to_string(),
        row: None,
        field: None,
        message,
        char_index: Some(err.char_index),
        column_index: Some(err.column_index),
        line_index: Some(err.line_index),
        parsed_content: Some(err.parsed_content.clone()),
    }
}

/// Notices the canonical validator raises from single-entity validators,
/// which run after a row parsed cleanly: they neither keep a row out of the
/// table nor make the table unparsable.
pub(crate) fn is_entity_level_notice(code: &str) -> bool {
    matches!(
        code,
        "mixed_case_recommended_field" | "invalid_currency_amount"
    )
}

/// Whether the canonical validator would build an entity from a row with these
/// notices: no parse error, a full-length row, and not an empty one.
pub(crate) fn row_is_loadable(notices: &[ValidationNotice]) -> bool {
    !notices.iter().any(|notice| {
        (notice.severity == NoticeSeverity::Error && !is_entity_level_notice(&notice.code))
            || notice.code == "empty_row"
    })
}

/// Whether `notice`, raised while a table was read, makes the canonical
/// validator treat that table as not parsed successfully.
pub(crate) fn notice_fails_table(notice: &ValidationNotice) -> bool {
    notice.severity == NoticeSeverity::Error && !is_entity_level_notice(&notice.code)
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
    let mut scanner = RecordScanner::new(reader, max_chars_per_column(&file));
    let headers = scanner
        .headers()
        .map_err(|err| scan_error_to_parse_error(&file, err, None))?
        .unwrap_or_default();
    let header_record = trimmed_header_record(&headers);

    let mut raw_records: Vec<(u64, ByteRecord)> = Vec::new();
    let mut scan_errors: Vec<CsvParseError> = Vec::new();
    loop {
        match scanner.next_record() {
            Ok(Some(next)) => raw_records.push(next),
            Ok(None) => break,
            Err(err) => {
                scan_errors.push(scan_error_to_parse_error(&file, err, Some(&headers)));
                break;
            }
        }
    }

    // Capture the validation context so each worker can re-apply it.
    let ctx = crate::validation_context::ValidationContextState::capture();
    let processed = deserialize_validate_records::<T, _>(
        raw_records,
        &header_record,
        &file,
        &validator,
        pool,
        &ctx,
    );

    let mut rows = Vec::with_capacity(processed.len());
    let mut row_numbers = Vec::with_capacity(processed.len());
    let mut errors = Vec::new();
    let mut all_notices = Vec::new();
    for (line_number, result, row_notices) in processed {
        let stop = row_notices
            .iter()
            .any(|notice| notice.code == "too_many_rows");
        all_notices.extend(row_notices);
        match result {
            Some(Ok(record)) => {
                rows.push(record);
                row_numbers.push(line_number);
            }
            Some(Err(err)) => errors.push(err),
            None => {}
        }
        if stop {
            break;
        }
    }
    errors.extend(scan_errors);

    Ok((
        CsvTable {
            headers: header_record.iter().map(str::to_string).collect(),
            rows,
            row_numbers,
        },
        errors,
        all_notices,
    ))
}

/// The outcome of one record: its row number, the entity (`None` when the row
/// is not loaded at all), and the row's notices.
pub(crate) type ProcessedRecord<T> = (u64, Option<Result<T, CsvParseError>>, Vec<ValidationNotice>);

/// Deserialize + validate a batch of byte records in parallel, preserving the
/// original record order.
///
/// Work is split into row chunks; each rayon worker installs the thread-local
/// interner hook (read by `StringId::deserialize`) and re-applies the captured
/// validation context once per chunk. `chunks` on an indexed parallel iterator
/// yields results in order, so the flattened output needs no post-sort.
#[cfg(feature = "parallel")]
pub(crate) fn deserialize_validate_records<T, V>(
    records: Vec<(u64, ByteRecord)>,
    headers: &csv::StringRecord,
    file: &str,
    validator: &V,
    pool: &crate::StringPool,
    ctx: &crate::validation_context::ValidationContextState,
) -> Vec<ProcessedRecord<T>>
where
    T: DeserializeOwned + Send,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice> + Sync,
{
    use rayon::prelude::*;

    // Rows handed to a single worker task. Large enough to amortize the
    // per-chunk thread-local setup, small enough to keep every core busy.
    const PARALLEL_CHUNK_ROWS: usize = 8192;

    let nested: Vec<Vec<ProcessedRecord<T>>> = records
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
                let trimmed = java_trim(s);
                if trimmed.is_empty() {
                    return gtfs_guru_model::StringId(0);
                }
                // `CompactString: Borrow<str>`, so the hit path hashes the
                // borrowed field directly. Building an owned key first cost an
                // allocation per field for every id longer than the inline
                // capacity — on the hot path that is once per cell.
                if let Some(id) = local_intern_cache.borrow().get(trimmed) {
                    return *id;
                }
                let id = chunk_pool.intern(trimmed);
                local_intern_cache
                    .borrow_mut()
                    .insert(compact_str::CompactString::new(trimmed), id);
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

/// Deserialize + validate a batch of byte records on the calling thread. The
/// caller has installed the interner hook and validation context.
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
pub(crate) fn deserialize_validate_records_sequential<T, V>(
    records: Vec<(u64, ByteRecord)>,
    headers: &csv::StringRecord,
    file: &str,
    validator: &V,
) -> Vec<ProcessedRecord<T>>
where
    T: DeserializeOwned,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice>,
{
    let mut trimmed_record = csv::StringRecord::new();
    records
        .into_iter()
        .map(|(line_number, record)| {
            deserialize_validate_one::<T, V>(
                record,
                line_number,
                headers,
                file,
                validator,
                &mut trimmed_record,
            )
        })
        .collect()
}

/// Validate (untrimmed) and deserialize (trimmed) a single record.
///
/// The common case — valid UTF-8 — converts the byte record in place. Invalid
/// bytes are replaced with U+FFFD per field so the row validator can flag them,
/// matching the canonical validator's replacing decoder. A row the validator
/// rejects is not deserialized at all.
fn deserialize_validate_one<T, V>(
    record: ByteRecord,
    line_number: u64,
    headers: &csv::StringRecord,
    file: &str,
    validator: &V,
    trimmed_record: &mut csv::StringRecord,
) -> ProcessedRecord<T>
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
    if !row_is_loadable(&notices) {
        return (line_number, None, notices);
    }

    // Most production feeds do not have surrounding field whitespace. Avoid
    // copying every field into a second StringRecord on that hot path, while
    // trimming dirty rows the way Java's `String.trim()` does.
    let needs_trimming = string_record
        .iter()
        .any(|field| field.len() != java_trim(field).len());
    let result = if needs_trimming {
        trimmed_record.clear();
        for field in string_record.iter() {
            trimmed_record.push_field(java_trim(field));
        }
        trimmed_record.deserialize(Some(headers))
    } else {
        string_record.deserialize(Some(headers))
    }
    .map_err(|err| map_byte_record_error(file, Some(headers), line_number, err));

    (line_number, Some(result), notices)
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

/// A row type [`load_table`] can build: `Send` too where rows are
/// deserialized on the rayon pool.
#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
pub(crate) trait LoadRow: DeserializeOwned + Send {}
#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
impl<T: DeserializeOwned + Send> LoadRow for T {}
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
pub(crate) trait LoadRow: DeserializeOwned {}
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
impl<T: DeserializeOwned> LoadRow for T {}

/// Read one GTFS table with notices, the way the canonical `CsvFileLoader`
/// does:
///
/// * no record at all (nothing, blank lines, a lone BOM) is `empty_file`;
/// * header notices are raised, and on a header error no row is read;
/// * each row is validated, and only rows without a parse error become
///   entities;
/// * a value over the column limit is `csv_parsing_failed`, and reading
///   stops there, keeping what earlier rows raised.
///
/// Returns `Err` only when the underlying reader fails.
pub(crate) fn load_table<T, R>(
    reader: R,
    file_name: &str,
    notices: &mut NoticeContainer,
    #[allow(unused_variables)] pool: &crate::StringPool,
) -> Result<CsvTable<T>, std::io::Error>
where
    T: LoadRow,
    R: Read,
{
    let mut scanner = RecordScanner::new(reader, max_chars_per_column(file_name));
    let headers = match scanner.headers() {
        Ok(Some(headers)) => headers,
        Ok(None) => {
            notices.push_empty_table(file_name);
            return Ok(CsvTable::default());
        }
        Err(ScanError::TooLong(err)) => {
            notices.push_csv_error(&field_too_long_error(file_name, &err, None));
            return Ok(CsvTable::default());
        }
        Err(ScanError::Io(err)) => return Err(err),
    };
    let Some(mut builder) = TableBuilder::<T>::begin(file_name, headers, notices) else {
        return Ok(CsvTable::default());
    };

    // Native builds scan every record, then deserialize them in one parallel
    // pass; single-threaded builds bound memory by working in batches.
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    const BATCH_ROWS: usize = usize::MAX;
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    const BATCH_ROWS: usize = 4096;

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    let ctx = crate::validation_context::ValidationContextState::capture();

    let mut failure = None;
    let dbg_scan = std::time::Instant::now();
    loop {
        let mut batch = Vec::new();
        let mut done = false;
        while batch.len() < BATCH_ROWS {
            match scanner.next_record() {
                Ok(Some(next)) => batch.push(next),
                Ok(None) => {
                    done = true;
                    break;
                }
                Err(ScanError::TooLong(err)) => {
                    failure = Some(err);
                    done = true;
                    break;
                }
                Err(ScanError::Io(err)) => return Err(err),
            }
        }
        if std::env::var_os("PROBE").is_some() {
            eprintln!("PROBE {file_name} scan: {:?}", dbg_scan.elapsed());
        }
        let dbg_t = std::time::Instant::now();
        let dbg_n = batch.len();
        #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
        let more = builder.add_batch_parallel(batch, pool, &ctx);
        if std::env::var_os("PROBE").is_some() {
            eprintln!("PROBE {file_name} process {dbg_n}: {:?}", dbg_t.elapsed());
        }
        #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
        let more = builder.add_batch_sequential(batch);
        if done || !more {
            break;
        }
    }
    let dbg_t = std::time::Instant::now();
    let table = builder.finish(failure.as_ref(), notices);
    if std::env::var_os("PROBE").is_some() {
        eprintln!("PROBE {file_name} finish: {:?}", dbg_t.elapsed());
    }
    Ok(table)
}

/// Accumulates one table from batches of scanned records. Shared by
/// [`load_table`] and the streaming zip reader, so both apply the same rules.
pub(crate) struct TableBuilder<T> {
    file_name: String,
    raw_headers: Vec<String>,
    header_record: StringRecord,
    validator: crate::csv_validation::RowValidator,
    rows: Vec<T>,
    row_numbers: Vec<u64>,
    notices: Vec<ValidationNotice>,
    errors: Vec<CsvParseError>,
    stopped: bool,
}

impl<T: DeserializeOwned> TableBuilder<T> {
    /// Validate the header. `None` when the header has an error: the
    /// canonical validator then reads no row, and the table stays empty.
    pub(crate) fn begin(
        file_name: &str,
        headers: Vec<String>,
        notices: &mut NoticeContainer,
    ) -> Option<Self> {
        let mut header_notices = NoticeContainer::new();
        crate::csv_validation::validate_headers(file_name, &headers, &mut header_notices);
        let has_header_errors = header_notices
            .iter()
            .any(|notice| notice.severity == NoticeSeverity::Error);
        notices.merge(header_notices);
        if has_header_errors {
            return None;
        }
        Some(Self {
            file_name: file_name.to_string(),
            header_record: trimmed_header_record(&headers),
            validator: crate::csv_validation::RowValidator::new(file_name, headers.clone()),
            raw_headers: headers,
            rows: Vec::new(),
            row_numbers: Vec::new(),
            notices: Vec::new(),
            errors: Vec::new(),
            stopped: false,
        })
    }

    /// Returns `false` once reading should stop (`too_many_rows`).
    #[cfg(feature = "parallel")]
    pub(crate) fn add_batch_parallel(
        &mut self,
        batch: Vec<(u64, ByteRecord)>,
        pool: &crate::StringPool,
        ctx: &crate::validation_context::ValidationContextState,
    ) -> bool
    where
        T: Send,
    {
        if self.stopped || batch.is_empty() {
            return !self.stopped;
        }
        let validator = &self.validator;
        let processed = deserialize_validate_records::<T, _>(
            batch,
            &self.header_record,
            &self.file_name,
            &|record: &StringRecord, line: u64| validator.validate_row(record, line),
            pool,
            ctx,
        );
        self.absorb(processed)
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    pub(crate) fn add_batch_sequential(&mut self, batch: Vec<(u64, ByteRecord)>) -> bool {
        if self.stopped || batch.is_empty() {
            return !self.stopped;
        }
        let validator = &self.validator;
        let processed = deserialize_validate_records_sequential::<T, _>(
            batch,
            &self.header_record,
            &self.file_name,
            &|record: &StringRecord, line: u64| validator.validate_row(record, line),
        );
        self.absorb(processed)
    }

    fn absorb(&mut self, processed: Vec<ProcessedRecord<T>>) -> bool {
        for (line_number, result, row_notices) in processed {
            let stop = row_notices
                .iter()
                .any(|notice| notice.code == "too_many_rows");
            self.notices.extend(row_notices);
            match result {
                Some(Ok(row)) => {
                    self.rows.push(row);
                    self.row_numbers.push(line_number);
                }
                Some(Err(err)) => self.errors.push(err),
                None => {}
            }
            if stop {
                self.stopped = true;
                return false;
            }
        }
        true
    }

    /// Emit the collected notices and build the table. `failure` is a value
    /// over the column limit that stopped the scan.
    pub(crate) fn finish(
        self,
        failure: Option<&FieldTooLong>,
        notices: &mut NoticeContainer,
    ) -> CsvTable<T> {
        for notice in self.notices {
            notices.push(notice);
        }
        let headers: Vec<String> = self.header_record.iter().map(str::to_string).collect();
        for error in &self.errors {
            if !skip_csv_parse_error(&headers, error) {
                notices.push_csv_error(error);
            }
        }
        if let Some(failure) = failure {
            notices.push_csv_error(&field_too_long_error(
                &self.file_name,
                failure,
                Some(&self.raw_headers),
            ));
        }
        CsvTable {
            headers,
            rows: self.rows,
            row_numbers: self.row_numbers,
        }
    }
}

/// Whether a row's deserialization error stays out of the report. The row
/// validator already reports every value the canonical validator rejects, so
/// in default mode a serde failure is never news; `--thorough` shows the ones
/// the row validator does not cover.
pub(crate) fn skip_csv_parse_error(headers: &[String], error: &CsvParseError) -> bool {
    if !crate::validation_context::thorough_mode_enabled() {
        return true;
    }

    let field = error.field.as_deref().or_else(|| {
        error
            .column_index
            .and_then(|index| headers.get(index as usize))
            .map(String::as_str)
    });
    if field
        .map(crate::csv_validation::is_value_validated_field)
        .unwrap_or(false)
    {
        return true;
    }

    let message = error.message.to_ascii_lowercase();
    message.contains("invalid date")
        || message.contains("invalid time")
        || message.contains("invalid color")
        || message.contains("invalid digit")
        || message.contains("invalid float")
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

    fn row_numbers(data: &[u8]) -> Vec<u64> {
        let (table, errors, _) =
            read_csv_from_reader_with_validation::<ExampleRow, _, _>(data, "rows.csv", |_, _| {
                Vec::new()
            })
            .expect("parse csv");
        assert!(errors.is_empty(), "{errors:?}");
        table.row_numbers
    }

    #[test]
    fn row_numbers_are_the_line_each_record_ends_on() {
        // Every expectation below was read off the canonical validator's
        // `CsvFile` (univocity `currentLine()`).
        assert_eq!(row_numbers(b"a,b\n1,2\n3,4\n"), vec![2, 3]);
        assert_eq!(row_numbers(b"a,b\n1,2\n3,4"), vec![2, 3]);
        assert_eq!(row_numbers(b"a,b\r\n1,2\r\n3,4\r\n"), vec![2, 3]);
        assert_eq!(row_numbers(b"a,b\r\n1,2\r\n3,4"), vec![2, 3]);
        assert_eq!(row_numbers(b"a,b\n\n1,2\n3,4\n"), vec![3, 4]);
        assert_eq!(row_numbers(b"a,b\r\n\r\n1,2\r\n3,4\r\n"), vec![3, 4]);
        assert_eq!(row_numbers(b"a,b\n1,2\n\n\n3,4\n\n"), vec![2, 5]);
        assert_eq!(row_numbers(b"\na,b\n1,2\n"), vec![3]);
        assert_eq!(row_numbers(b"a,b\n   \n3,4\n"), vec![3]);
        assert_eq!(row_numbers(b"a,b\n#c\n1,2\n"), vec![3]);
        assert_eq!(row_numbers(b"a,b\r\n1,2\r\n\r\n\r\n3,4\r\n"), vec![2, 5]);
    }

    #[test]
    fn multi_line_values_number_the_row_by_its_last_line() {
        let data = b"a,b\n\"1\n\",2\n3,4\n";
        let (table, _, _) = read_csv_from_reader_with_validation::<StringRow, _, _>(
            data.as_slice(),
            "rows.csv",
            |_, _| Vec::new(),
        )
        .unwrap();
        assert_eq!(table.row_numbers, vec![3, 4]);
        let data = b"a,b\n1,\"x\n\ny\"\n3,4\n";
        let (table, _, _) = read_csv_from_reader_with_validation::<StringRow, _, _>(
            data.as_slice(),
            "rows.csv",
            |_, _| Vec::new(),
        )
        .unwrap();
        assert_eq!(table.row_numbers, vec![4, 5]);
        assert_eq!(table.rows[0].b, "x\n\ny");
        // At the end of input, without a newline.
        let data = b"a,b\n1,2\n\"x\ny\",z";
        let (table, _, _) = read_csv_from_reader_with_validation::<StringRow, _, _>(
            data.as_slice(),
            "rows.csv",
            |_, _| Vec::new(),
        )
        .unwrap();
        assert_eq!(table.row_numbers, vec![2, 4]);
        // An unterminated quote runs to the end of input.
        let data = b"a,b\n\"x,2\n3,4\n";
        let seen = std::cell::RefCell::new(Vec::new());
        let _ = read_csv_from_reader_with_validation::<StringRow, _, _>(
            data.as_slice(),
            "rows.csv",
            |record, line| {
                seen.borrow_mut()
                    .push((line, record.get(0).unwrap_or("").to_string()));
                Vec::new()
            },
        )
        .unwrap();
        assert_eq!(seen.into_inner(), vec![(4, "x,2\n3,4\n".to_string())]);
    }

    #[derive(Debug, Deserialize)]
    struct StringRow {
        a: String,
        b: String,
    }

    #[test]
    fn bare_carriage_return_stays_inside_the_value() {
        let data = b"a,b\n1 x\ry,2\n3,4\r\n";
        let (table, _, _) = read_csv_from_reader_with_validation::<StringRow, _, _>(
            data.as_slice(),
            "rows.csv",
            |_, _| Vec::new(),
        )
        .unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].a, "1 x\ry");
        assert_eq!(table.rows[1].b, "4");
        assert_eq!(table.row_numbers, vec![2, 3]);
    }

    #[test]
    fn reports_field_on_parse_error() {
        let data = "a,b\n1,boom\n";
        let err = read_csv_from_reader::<ExampleRow, _>(data.as_bytes(), "bad.csv")
            .expect_err("expected parse error");

        assert_eq!(err.file, "bad.csv");
        assert_eq!(err.row, Some(2));
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
        let data = "a,b\n\" 1 \",2\n3,boom\n4,5\n";
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
    fn rows_with_a_row_error_are_not_loaded() {
        let data = "a,b\n1,2\n3,4\n5,6\n";
        let (table, errors, notices) = read_csv_from_reader_with_validation::<ExampleRow, _, _>(
            data.as_bytes(),
            "rows.csv",
            |record, _| match record.get(0) {
                Some("3") => vec![crate::ValidationNotice::new(
                    "invalid_integer",
                    crate::NoticeSeverity::Error,
                    "bad",
                )],
                Some("5") => vec![crate::ValidationNotice::new(
                    "invalid_currency_amount",
                    crate::NoticeSeverity::Error,
                    "entity-level",
                )],
                _ => Vec::new(),
            },
        )
        .expect("parse csv");
        assert!(errors.is_empty());
        assert_eq!(notices.len(), 2);
        assert_eq!(table.row_numbers, vec![2, 4]);
    }

    #[test]
    fn stops_reading_after_too_many_rows_notice() {
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
        assert!(table.rows.is_empty());
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
    fn java_trim_keeps_unicode_spaces() {
        assert_eq!(java_trim(" \t\r\u{1}a b\u{0} "), "a b");
        assert_eq!(java_trim("R1\u{a0}"), "R1\u{a0}");
        assert_eq!(java_trim("\u{2003}x"), "\u{2003}x");
    }

    #[test]
    fn deserialization_trims_only_java_whitespace() {
        let data = "a,b\n\"x\u{a0}\",\" y \"\n";
        let table =
            read_csv_from_reader::<StringRow, _>(data.as_bytes(), "rows.csv").expect("parse csv");
        assert_eq!(table.rows[0].a, "x\u{a0}");
        assert_eq!(table.rows[0].b, "y");
    }

    #[test]
    fn over_long_value_reports_the_canonical_message() {
        let data = format!("a,b\n1,2\n{},3\n4,5\n", "x".repeat(4097));
        let (table, errors) =
            read_csv_from_reader_with_errors::<StringRow, _>(data.as_bytes(), "rows.csv")
                .expect("header parses");
        assert_eq!(table.row_numbers, vec![2]);
        assert_eq!(errors.len(), 1);
        let error = &errors[0];
        assert_eq!(error.char_index, Some(4 + 4 + 4097));
        assert_eq!(error.line_index, Some(2));
        assert_eq!(error.column_index, Some(0));
        assert!(error.message.starts_with(
            "Length of parsed input (4097) exceeds the maximum number of characters defined in your parser settings (4096). \nParser Configuration: CsvParserSettings:\n"
        ));
        assert!(error.message.contains(
            "line=2, column=0, record=1, charIndex=4105, headers=[a, b], content parsed=xxx"
        ));
        assert_eq!(
            error.parsed_content.as_deref(),
            Some("x".repeat(4096).as_str())
        );
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
        let data = "a,b\n1,2\n\n3,4\n5,6";
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
        assert_eq!(table.row_numbers, vec![2, 4, 5]);
    }
}
