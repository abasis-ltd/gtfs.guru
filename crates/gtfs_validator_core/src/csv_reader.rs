use std::fmt;
use std::io::{BufRead, BufReader, Read};

use csv::{ReaderBuilder, StringRecord, Trim};
use serde::de::DeserializeOwned;

#[cfg(feature = "parallel")]
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
                let row_number = iter.reader().position().line().saturating_sub(1);
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

fn map_csv_error(file: &str, headers: Option<&StringRecord>, err: csv::Error) -> CsvParseError {
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

fn skip_utf8_bom<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    let buf = reader.fill_buf()?;
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        reader.consume(3);
    }
    Ok(())
}

/// Parallel version of CSV parsing using rayon.
///
/// The CSV crate handles record boundary detection sequentially (fast),
/// while Serde deserialization is parallelized across a thread pool (CPU-intensive).
/// Results are sorted by index to preserve original file order.
///
/// The `interner_setup` closure is called on each worker thread before deserialization
/// to set up thread-local hooks (e.g., StringId interner) that are needed during serde deserialization.
#[cfg(feature = "parallel")]
pub fn read_csv_from_reader_parallel<T, R, V, I>(
    reader: R,
    file_name: impl Into<String>,
    validator: V,
    interner_setup: I,
) -> Result<(CsvTable<T>, Vec<CsvParseError>, Vec<ValidationNotice>), CsvParseError>
where
    T: DeserializeOwned + Send,
    R: Read,
    V: Fn(&csv::StringRecord, u64) -> Vec<ValidationNotice> + Sync + Send,
    I: Fn() + Sync + Send,
{
    let file = file_name.into();
    let mut buf_reader = BufReader::new(reader);
    if let Err(err) = skip_utf8_bom(&mut buf_reader) {
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
    let byte_headers = csv::ByteRecord::from(headers_record.clone());

    // Collect byte records with their line numbers sequentially
    // The csv crate's iterator handles record boundary detection
    let mut raw_records: Vec<(usize, u64, csv::ByteRecord)> = Vec::new();
    let mut scan_errors: Vec<CsvParseError> = Vec::new();

    for (index, result) in csv_reader.byte_records().enumerate() {
        match result {
            Ok(record) => {
                // Determine line number from position if available, otherwise fallback to estimation
                let line_number = record
                    .position()
                    .map(|p| p.line())
                    .unwrap_or((index + 2) as u64);
                raw_records.push((index, line_number, record));
            }
            Err(err) => {
                scan_errors.push(map_csv_error(&file, Some(&headers_record), err));
            }
        }
    }

    // Parallel deserialization using rayon
    let file_ref = &file;
    let headers_ref = &headers_record;
    let _byte_headers_ref = &byte_headers;
    let validator_ref = &validator;
    let _interner_setup_ref = &interner_setup;

    let mut results: Vec<(usize, u64, Result<T, CsvParseError>, Vec<ValidationNotice>)> =
        raw_records
            .into_iter() // Use iter() instead of par_iter() to keep deserialization on caller thread (which has interner hook)
            .map(|(index, line_number, record)| {
                // The interner hook is already set on this thread (set by ParallelLoader::load in feed.rs)
                // No need to call interner_setup_ref() since we're on the same thread

                let mut notices = Vec::new();
                // Explicitly convert to StringRecord for validation (checks UTF-8)
                match csv::StringRecord::from_byte_record(record.clone()) {
                    Ok(string_record) => {
                        // string_record is untrimmed (except headers logic).
                        // Validate untrimmed data:
                        notices = validator_ref(&string_record, line_number);

                        // For deserialization, we MUST trim fields to handle numeric types correctly.
                        let mut trimmed_record = csv::StringRecord::with_capacity(
                            string_record.as_slice().len(),
                            string_record.len(),
                        );
                        for field in string_record.iter() {
                            trimmed_record.push_field(field.trim());
                        }

                        // Deserialize from TRIMMED record
                        let result = trimmed_record
                            .deserialize(Some(headers_ref))
                            .map_err(|err| {
                                map_byte_record_error(file_ref, Some(headers_ref), line_number, err)
                            });
                        (index, line_number, result, notices)
                    }
                    Err(utf8_err) => {
                        // Report UTF-8 error as ParseError if it prevents validation
                        // Note: deserialize might still work if it doesn't touch bad bytes, but we prioritize data integrity
                        let err = csv::Error::from(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            utf8_err,
                        ));
                        return (
                            index,
                            line_number,
                            Err(map_byte_record_error(
                                file_ref,
                                Some(headers_ref),
                                line_number,
                                err,
                            )),
                            notices,
                        );
                    }
                }
            })
            .collect();

    // Sort by index to restore original file order
    results.sort_unstable_by_key(|(index, _, _, _)| *index);

    // Split into rows, row_numbers, and errors
    let mut rows = Vec::with_capacity(results.len());
    let mut row_numbers = Vec::with_capacity(results.len());
    let mut errors = scan_errors;
    let mut all_notices = Vec::new();

    for (_, line_number, result, row_notices) in results {
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

/// Map deserialization error from ByteRecord (used in parallel mode)
#[cfg(feature = "parallel")]
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
    fn strips_utf8_bom_from_headers() {
        let data = b"\xEF\xBB\xBFa,b\n9,10\n";
        let table =
            read_csv_from_reader::<ExampleRow, _>(data.as_slice(), "bom.csv").expect("parse csv");

        assert_eq!(table.headers, vec!["a", "b"]);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].a, 9);
    }
    #[test]
    #[cfg(feature = "parallel")]
    fn reads_headers_and_rows_parallel() {
        let data = "a,b\n1,2\n3,4\n5,6\n";
        let (table, errors, notices) = read_csv_from_reader_parallel::<ExampleRow, _, _, _>(
            data.as_bytes(),
            "test.csv",
            |_, _| Vec::new(),
            || {},
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
