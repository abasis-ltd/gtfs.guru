use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use zip::ZipArchive;

#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
use crate::csv_reader::read_csv_from_reader_parallel;
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
use crate::csv_reader::read_csv_from_reader_with_validation;
use crate::csv_reader::{read_csv_from_reader, CsvParseError, CsvTable};
use crate::csv_validation::is_value_validated_field;
use crate::csv_validation::{validate_headers, RowValidator};

use crate::feed::GTFS_FILE_NAMES;
use crate::{NoticeContainer, NoticeSeverity, ValidationNotice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GtfsInputSource {
    Zip,
    Directory,
}

#[derive(Debug, thiserror::Error)]
pub enum GtfsInputError {
    #[error("input path does not exist: {0}")]
    MissingPath(PathBuf),
    #[error("input path is neither a file nor a directory: {0}")]
    InvalidPath(PathBuf),
    #[error("zip input is not a .zip file: {0}")]
    InvalidZip(PathBuf),
    #[error("missing file in input: {0}")]
    MissingFile(String),
    #[error("expected file but found directory: {0}")]
    NotAFile(PathBuf),
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("zip archive error for {path}: {source}")]
    ZipArchive {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("zip error for {file}: {source}")]
    ZipFile {
        file: String,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("io error while reading {file} from {path}: {source}")]
    ZipFileIo {
        path: PathBuf,
        file: String,
        #[source]
        source: std::io::Error,
    },
    #[error("csv parse error: {0}")]
    Csv(#[from] CsvParseError),
    #[error("json parse error for {file}: {source}")]
    Json {
        file: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone)]
pub struct GtfsInput {
    path: PathBuf,
    source: GtfsInputSource,
}

impl GtfsInput {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, GtfsInputError> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(GtfsInputError::MissingPath(path));
        }

        if path.is_dir() {
            return Ok(Self {
                path,
                source: GtfsInputSource::Directory,
            });
        }

        if path.is_file() {
            let is_zip = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("gtfs"))
                .unwrap_or(false);

            if !is_zip {
                return Err(GtfsInputError::InvalidZip(path));
            }

            return Ok(Self {
                path,
                source: GtfsInputSource::Zip,
            });
        }

        Err(GtfsInputError::InvalidPath(path))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source(&self) -> GtfsInputSource {
        self.source
    }

    pub fn reader(&self) -> GtfsInputReader {
        GtfsInputReader {
            path: self.path.clone(),
            source: self.source,
            // Shared across every capped read done through this reader, so the
            // total decompressed volume of one archive is bounded, not just each
            // member individually.
            remaining_bytes: Arc::new(AtomicU64::new(max_total_bytes())),
        }
    }
}

pub fn collect_input_notices(input: &GtfsInput) -> Result<Vec<ValidationNotice>, GtfsInputError> {
    let reader = input.reader();
    let files = reader.list_files()?;
    let known: HashSet<String> = GTFS_FILE_NAMES
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let mut notices = Vec::new();

    for path in files {
        let normalized = path.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
        if file_name.eq_ignore_ascii_case(".ds_store") {
            continue;
        }
        let is_known = known.contains(&file_name.to_ascii_lowercase());
        if !is_known {
            notices.push(unknown_file_notice(file_name));
        }
    }

    if matches!(input.source(), GtfsInputSource::Zip) && reader.has_nested_gtfs_files()? {
        notices.push(invalid_input_files_notice());
    }

    Ok(notices)
}

fn decode_utf8_lossy(data: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(data) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(String::from_utf8_lossy(data).into_owned()),
    }
}

#[derive(Debug, Clone)]
pub struct GtfsInputReader {
    path: PathBuf,
    source: GtfsInputSource,
    /// Remaining decompression budget for the whole archive. Cloned readers
    /// share the same counter (it is an `Arc`), so concurrent member reads all
    /// draw from one archive-wide limit.
    remaining_bytes: Arc<AtomicU64>,
}

impl GtfsInputReader {
    pub fn get_files_with_sizes(&self) -> Result<HashMap<String, u64>, GtfsInputError> {
        match self.source {
            GtfsInputSource::Directory => {
                let mut files = HashMap::new();
                for entry in std::fs::read_dir(&self.path).map_err(|err| GtfsInputError::Io {
                    path: self.path.clone(),
                    source: err,
                })? {
                    let entry = entry.map_err(|err| GtfsInputError::Io {
                        path: self.path.clone(),
                        source: err,
                    })?;
                    let path = entry.path();
                    let file_type = entry.file_type().map_err(|err| GtfsInputError::Io {
                        path: path.clone(),
                        source: err,
                    })?;
                    if file_type.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let size = path
                            .metadata()
                            .map_err(|err| GtfsInputError::Io {
                                path: path.clone(),
                                source: err,
                            })?
                            .len();
                        files.insert(name, size);
                    }
                }
                Ok(files)
            }
            GtfsInputSource::Zip => {
                let file = File::open(&self.path).map_err(|err| GtfsInputError::Io {
                    path: self.path.clone(),
                    source: err,
                })?;
                let mut archive =
                    ZipArchive::new(file).map_err(|err| GtfsInputError::ZipArchive {
                        path: self.path.clone(),
                        source: err,
                    })?;

                let mut files = HashMap::new();
                for index in 0..archive.len() {
                    let file = archive
                        .by_index(index)
                        .map_err(|err| GtfsInputError::ZipFile {
                            file: self.path.to_string_lossy().to_string(),
                            source: err,
                        })?;
                    if !file.is_dir() {
                        // Only include root-level files (mirroring current logic)
                        let name = file.name().to_string();
                        if !(name.contains('/') || name.contains('\\')) {
                            files.insert(name, file.size());
                        }
                    }
                }
                Ok(files)
            }
        }
    }

    pub fn read_file(&self, file_name: &str) -> Result<Vec<u8>, GtfsInputError> {
        match self.source {
            GtfsInputSource::Directory => self.read_from_directory(file_name),
            GtfsInputSource::Zip => self.read_from_zip(file_name),
        }
    }

    pub fn read_csv<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        read_csv_from_reader(data_str.as_bytes(), file_name).map_err(GtfsInputError::Csv)
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    pub fn read_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        pool: &crate::StringPool,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        if strip_utf8_bom(&data).is_empty() {
            notices.push_empty_table(file_name);
            return Ok(CsvTable::default());
        }
        let data_str = decode_utf8_lossy(&data);
        let data_bytes = data_str.as_bytes();
        // Peek headers for validator setup
        let mut peek_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .trim(csv::Trim::None)
            .from_reader(data_bytes);

        let headers_record = match peek_reader.headers() {
            Ok(h) => h.clone(),
            Err(_) => {
                let (table, _, _) =
                    read_csv_from_reader_parallel(data_bytes, file_name, |_, _| Vec::new(), pool)
                        .map_err(GtfsInputError::Csv)?;
                return Ok(table);
            }
        };

        let headers: Vec<String> = headers_record.iter().map(|s| s.to_string()).collect();
        validate_headers(file_name, &headers, notices);
        let validator = RowValidator::new(file_name, headers);

        let (table, errors, row_notices) = read_csv_from_reader_parallel(
            data_bytes,
            file_name,
            |record, line| validator.validate_row(record, line),
            pool,
        )
        .map_err(GtfsInputError::Csv)?;

        for notice in row_notices {
            notices.push(notice);
        }
        for error in errors {
            if skip_csv_parse_error(&table, &error) {
                continue;
            }
            notices.push_csv_error(&error);
        }

        Ok(table)
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    pub fn read_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        _pool: &crate::StringPool,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        read_csv_bytes_with_notices(data_str.as_bytes(), file_name, notices)
    }

    pub fn read_optional_csv<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                read_csv_from_reader(data_str.as_bytes(), file_name)
                    .map(Some)
                    .map_err(GtfsInputError::Csv)
            }
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        pool: &crate::StringPool,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                if strip_utf8_bom(&data).is_empty() {
                    notices.push_empty_table(file_name);
                    return Ok(Some(CsvTable::default()));
                }
                let data_str = decode_utf8_lossy(&data);
                let data_bytes = data_str.as_bytes();
                // Peek headers for validator setup
                let mut peek_reader = csv::ReaderBuilder::new()
                    .has_headers(true)
                    .flexible(true)
                    .trim(csv::Trim::None)
                    .from_reader(data_bytes);

                let headers_record = match peek_reader.headers() {
                    Ok(h) => h.clone(),
                    Err(_) => {
                        let (table, _, _) = read_csv_from_reader_parallel(
                            data_bytes,
                            file_name,
                            |_, _| Vec::new(),
                            pool,
                        )
                        .map_err(GtfsInputError::Csv)?;
                        return Ok(Some(table));
                    }
                };

                let headers: Vec<String> = headers_record.iter().map(|s| s.to_string()).collect();
                let mut header_notices = NoticeContainer::new();
                validate_headers(file_name, &headers, &mut header_notices);
                let has_header_errors = header_notices
                    .iter()
                    .any(|notice| notice.severity == NoticeSeverity::Error);
                notices.merge(header_notices);
                let validator = RowValidator::new(file_name, headers);

                let (table, errors, row_notices) = read_csv_from_reader_parallel(
                    data_bytes,
                    file_name,
                    |record, line| {
                        if has_header_errors {
                            Vec::new()
                        } else {
                            validator.validate_row(record, line)
                        }
                    },
                    pool,
                )
                .map_err(GtfsInputError::Csv)?;

                if !has_header_errors {
                    for notice in row_notices {
                        notices.push(notice);
                    }
                }
                for error in errors {
                    if skip_csv_parse_error(&table, &error) {
                        continue;
                    }
                    notices.push_csv_error(&error);
                }

                Ok(Some(table))
            }
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        _pool: &crate::StringPool,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                read_csv_bytes_with_notices(data_str.as_bytes(), file_name, notices).map(Some)
            }
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn read_json<T: DeserializeOwned>(&self, file_name: &str) -> Result<T, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data = strip_utf8_bom(&data);
        serde_json::from_slice(data).map_err(|err| GtfsInputError::Json {
            file: file_name.to_string(),
            source: err,
        })
    }

    pub fn read_optional_json<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<Option<T>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => serde_json::from_slice(strip_utf8_bom(&data))
                .map(Some)
                .map_err(|err| GtfsInputError::Json {
                    file: file_name.to_string(),
                    source: err,
                }),
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Streaming parallel CSV reader for very large zip members (e.g.
    /// `stop_times.txt` on big feeds).
    ///
    /// A producer thread decompresses the zip entry and scans CSV record
    /// boundaries, handing batches of byte records to the rayon pool for
    /// deserialization + validation while it keeps decompressing. This overlaps
    /// the otherwise-serial unzip + boundary scan with the parallel parse and
    /// bounds peak memory (only a few batches are in flight at once).
    ///
    /// Falls back to the in-memory parallel reader for non-zip sources. The
    /// caller (the feed loader) only routes large files here, and the dominant
    /// one runs on the main thread, so no rayon worker ever blocks on the
    /// producer channel.
    #[cfg(feature = "parallel")]
    pub(crate) fn read_optional_csv_streaming<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        pool: &crate::StringPool,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        use crate::csv_reader::{
            deserialize_validate_records, map_csv_error, map_io_error, skip_utf8_bom,
        };
        use std::sync::mpsc::sync_channel;

        if self.source != GtfsInputSource::Zip {
            return self.read_optional_csv_with_notices(file_name, notices, pool);
        }

        // Rows per batch handed to the consumer, and how many batches may be
        // buffered ahead of it (this bounds peak memory).
        const BATCH_ROWS: usize = 65_536;
        const CHANNEL_CAPACITY: usize = 3;

        enum Msg {
            Headers(csv::ByteRecord),
            Batch(Vec<(u64, csv::ByteRecord)>),
            ScanError(CsvParseError),
        }

        let (tx, rx) = sync_channel::<Msg>(CHANNEL_CAPACITY);
        let path = &self.path;
        let remaining_bytes = &self.remaining_bytes;

        std::thread::scope(|scope| -> Result<Option<CsvTable<T>>, GtfsInputError> {
            // ---- Producer: stream-decompress + scan record boundaries. ----
            let producer = scope.spawn(move || -> Result<bool, GtfsInputError> {
                let file = File::open(path).map_err(|err| GtfsInputError::Io {
                    path: path.clone(),
                    source: err,
                })?;
                let mut archive =
                    ZipArchive::new(file).map_err(|err| GtfsInputError::ZipArchive {
                        path: path.clone(),
                        source: err,
                    })?;
                let Some(index) = locate_zip_member(&mut archive, file_name)? else {
                    return Ok(false); // missing file
                };
                let zipped = archive
                    .by_index(index)
                    .map_err(|err| GtfsInputError::ZipFile {
                        file: file_name.to_string(),
                        source: err,
                    })?;

                // Enforce the same per-member and archive-wide decompression
                // caps as the buffered path, so streaming a large member cannot
                // bypass the zip-bomb guard.
                let cap = max_member_bytes();
                if zipped.size() > cap {
                    return Err(member_too_large(path, file_name, zipped.size(), cap));
                }
                if zipped.size() > remaining_bytes.load(Ordering::Relaxed) {
                    return Err(archive_budget_exceeded(path, file_name, max_total_bytes()));
                }
                let capped = CappedReader::new(zipped, path, file_name, cap, remaining_bytes);
                let mut buf_reader = std::io::BufReader::with_capacity(1 << 20, capped);
                skip_utf8_bom(&mut buf_reader).map_err(|err| {
                    if limit_kind(&err).is_some() {
                        map_capped_read_error(path, file_name, cap, err)
                    } else {
                        GtfsInputError::Csv(map_io_error(file_name, err))
                    }
                })?;

                let mut csv_reader = csv::ReaderBuilder::new()
                    .has_headers(true)
                    .flexible(true)
                    .trim(csv::Trim::None)
                    .from_reader(buf_reader);

                let headers = csv_reader
                    .byte_headers()
                    .map_err(|err| {
                        if let Some(io_err) = csv_limit_io_error(&err) {
                            map_capped_read_error(path, file_name, cap, io_err)
                        } else {
                            GtfsInputError::Csv(map_csv_error(file_name, None, err))
                        }
                    })?
                    .clone();
                let mut headers_for_errors = csv::StringRecord::new();
                for field in headers.iter() {
                    headers_for_errors.push_field(&String::from_utf8_lossy(field));
                }
                if tx.send(Msg::Headers(headers)).is_err() {
                    return Ok(true); // consumer dropped
                }

                let mut batch: Vec<(u64, csv::ByteRecord)> = Vec::with_capacity(BATCH_ROWS);
                let mut record_index = 0usize;
                let records = csv_reader.into_byte_records();
                for result in records {
                    match result {
                        Ok(record) => {
                            let line_number = record
                                .position()
                                .map(|p| p.record() + 1)
                                .unwrap_or((record_index + 2) as u64);
                            batch.push((line_number, record));
                            record_index += 1;
                            if batch.len() >= BATCH_ROWS {
                                let full =
                                    std::mem::replace(&mut batch, Vec::with_capacity(BATCH_ROWS));
                                if tx.send(Msg::Batch(full)).is_err() {
                                    return Ok(true);
                                }
                            }
                        }
                        Err(err) => {
                            if let Some(io_err) = csv_limit_io_error(&err) {
                                return Err(map_capped_read_error(path, file_name, cap, io_err));
                            }
                            let parse_err =
                                map_csv_error(file_name, Some(&headers_for_errors), err);
                            if tx.send(Msg::ScanError(parse_err)).is_err() {
                                return Ok(true);
                            }
                        }
                    }
                }
                if !batch.is_empty() {
                    let _ = tx.send(Msg::Batch(batch));
                }
                Ok(true)
            });

            // ---- Consumer: build validator from headers, deserialize batches. ----
            let ctx = crate::validation_context::ValidationContextState::capture();
            let mut headers_vec: Option<Vec<String>> = None;
            let mut headers_trimmed: Option<csv::StringRecord> = None;
            let mut validator: Option<RowValidator> = None;
            let mut has_header_errors = false;

            let mut rows: Vec<T> = Vec::new();
            let mut row_numbers: Vec<u64> = Vec::new();
            let mut parse_errors: Vec<CsvParseError> = Vec::new();
            let mut collected_notices: Vec<ValidationNotice> = Vec::new();

            for msg in rx {
                match msg {
                    Msg::Headers(byte_headers) => {
                        // Untrimmed header names feed header validation and the row
                        // validator (matching the in-memory path); a trimmed copy
                        // is used as the deserialization header map.
                        let untrimmed: Vec<String> = byte_headers
                            .iter()
                            .map(|field| String::from_utf8_lossy(field).into_owned())
                            .collect();
                        let mut header_notices = NoticeContainer::new();
                        validate_headers(file_name, &untrimmed, &mut header_notices);
                        has_header_errors = header_notices
                            .iter()
                            .any(|notice| notice.severity == NoticeSeverity::Error);
                        notices.merge(header_notices);
                        validator = Some(RowValidator::new(file_name, untrimmed.clone()));

                        let mut trimmed = csv::StringRecord::new();
                        for field in untrimmed.iter() {
                            trimmed.push_field(field.trim());
                        }
                        headers_vec = Some(
                            untrimmed
                                .iter()
                                .map(|field| field.trim().to_string())
                                .collect(),
                        );
                        headers_trimmed = Some(trimmed);
                    }
                    Msg::Batch(batch) => {
                        let (Some(validator), Some(headers_trimmed)) =
                            (validator.as_ref(), headers_trimmed.as_ref())
                        else {
                            continue;
                        };
                        let processed = deserialize_validate_records::<T, _>(
                            batch,
                            headers_trimmed,
                            file_name,
                            &|record: &csv::StringRecord, line: u64| {
                                if has_header_errors {
                                    Vec::new()
                                } else {
                                    validator.validate_row(record, line)
                                }
                            },
                            pool,
                            &ctx,
                        );
                        for (line_number, result, row_notices) in processed {
                            if !has_header_errors {
                                collected_notices.extend(row_notices);
                            }
                            match result {
                                Ok(record) => {
                                    rows.push(record);
                                    row_numbers.push(line_number);
                                }
                                Err(err) => parse_errors.push(err),
                            }
                        }
                    }
                    Msg::ScanError(err) => parse_errors.push(err),
                }
            }

            let found = producer.join().expect("csv streaming producer panicked")?;
            if !found {
                return Ok(None);
            }

            let table = CsvTable {
                headers: headers_vec.unwrap_or_default(),
                rows,
                row_numbers,
            };
            for notice in collected_notices {
                notices.push(notice);
            }
            for error in parse_errors {
                if skip_csv_parse_error(&table, &error) {
                    continue;
                }
                notices.push_csv_error(&error);
            }
            Ok(Some(table))
        })
    }

    fn read_from_directory(&self, file_name: &str) -> Result<Vec<u8>, GtfsInputError> {
        let path = self.path.join(file_name);
        if path.exists() {
            if !is_regular_file(&path) {
                return Err(GtfsInputError::NotAFile(path));
            }
            return read_filesystem_file_capped(path, file_name, &self.remaining_bytes);
        }

        let Some(found_path) = find_case_insensitive_file(&self.path, file_name)? else {
            return Err(GtfsInputError::MissingFile(file_name.to_string()));
        };
        if !is_regular_file(&found_path) {
            return Err(GtfsInputError::NotAFile(found_path));
        }
        read_filesystem_file_capped(found_path, file_name, &self.remaining_bytes)
    }

    fn read_from_zip(&self, file_name: &str) -> Result<Vec<u8>, GtfsInputError> {
        let file = File::open(&self.path).map_err(|err| GtfsInputError::Io {
            path: self.path.clone(),
            source: err,
        })?;
        let mut archive = ZipArchive::new(file).map_err(|err| GtfsInputError::ZipArchive {
            path: self.path.clone(),
            source: err,
        })?;

        match archive.by_name(file_name) {
            Ok(zipped) => {
                return read_zip_member_capped(
                    zipped,
                    &self.path,
                    file_name,
                    &self.remaining_bytes,
                );
            }
            Err(zip::result::ZipError::FileNotFound) => {}
            Err(err) => {
                return Err(GtfsInputError::ZipFile {
                    file: file_name.to_string(),
                    source: err,
                });
            }
        }

        let target = file_name.to_ascii_lowercase();
        let mut matched_index = None;
        let mut matched_depth = None;
        let mut matched_name = None;
        for index in 0..archive.len() {
            let (name, is_dir) = {
                let file = archive
                    .by_index(index)
                    .map_err(|err| GtfsInputError::ZipFile {
                        file: file_name.to_string(),
                        source: err,
                    })?;
                (file.name().to_string(), file.is_dir())
            };
            if is_dir {
                continue;
            }
            if name.contains('/') || name.contains('\\') {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let tail = lower
                .rsplit(|ch| ch == '/' || ch == '\\')
                .next()
                .unwrap_or(lower.as_str());
            if tail != target {
                continue;
            }
            let depth = name.matches(|ch| ch == '/' || ch == '\\').count();
            match matched_depth {
                None => {
                    matched_index = Some(index);
                    matched_depth = Some(depth);
                    matched_name = Some(lower);
                }
                Some(current_depth) if depth < current_depth => {
                    matched_index = Some(index);
                    matched_depth = Some(depth);
                    matched_name = Some(lower);
                }
                Some(current_depth) if depth == current_depth => {
                    let should_replace = matched_name
                        .as_ref()
                        .map(|best| lower < *best)
                        .unwrap_or(true);
                    if should_replace {
                        matched_index = Some(index);
                        matched_name = Some(lower);
                    }
                }
                _ => {}
            }
        }

        let Some(index) = matched_index else {
            return Err(GtfsInputError::MissingFile(file_name.to_string()));
        };
        let zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        read_zip_member_capped(zipped, &self.path, file_name, &self.remaining_bytes)
    }

    pub fn list_files(&self) -> Result<Vec<String>, GtfsInputError> {
        match self.source {
            GtfsInputSource::Directory => list_files_in_directory(&self.path),
            GtfsInputSource::Zip => list_files_in_zip(&self.path),
        }
    }

    pub fn has_nested_gtfs_files(&self) -> Result<bool, GtfsInputError> {
        match self.source {
            GtfsInputSource::Directory => has_nested_gtfs_file_in_directory(&self.path),
            GtfsInputSource::Zip => has_nested_gtfs_file_in_zip(&self.path),
        }
    }
}

fn skip_csv_parse_error<T>(table: &CsvTable<T>, error: &CsvParseError) -> bool {
    // In default mode, suppress csv_parsing_failed for tolerance (matches Java Univocity)
    if !crate::validation_context::thorough_mode_enabled() {
        return true;
    }

    let field = error.field.as_deref().or_else(|| {
        error
            .column_index
            .and_then(|index| table.headers.get(index as usize))
            .map(String::as_str)
    });
    if field.map(is_value_validated_field).unwrap_or(false) {
        return true;
    }

    let message = error.message.to_ascii_lowercase();
    message.contains("invalid date")
        || message.contains("invalid time")
        || message.contains("invalid color")
        || message.contains("invalid digit")
        || message.contains("invalid float")
}

#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
fn read_csv_bytes_with_notices<T: DeserializeOwned>(
    data: &[u8],
    file_name: &str,
    notices: &mut NoticeContainer,
) -> Result<CsvTable<T>, GtfsInputError> {
    if strip_utf8_bom(data).is_empty() {
        notices.push_empty_table(file_name);
        return Ok(CsvTable::default());
    }

    // Read only the header up front so row validation can be configured. The
    // data rows themselves are scanned exactly once below.
    let mut header_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::Headers)
        .from_reader(data);
    let headers_record = header_reader
        .headers()
        .map_err(|err| GtfsInputError::Csv(crate::csv_reader::map_csv_error(file_name, None, err)))?
        .clone();
    let headers: Vec<String> = headers_record.iter().map(str::to_string).collect();

    let mut header_notices = NoticeContainer::new();
    validate_headers(file_name, &headers, &mut header_notices);
    let has_header_errors = header_notices
        .iter()
        .any(|notice| notice.severity == NoticeSeverity::Error);
    notices.merge(header_notices);
    let validator = RowValidator::new(file_name, headers);

    let (table, errors, row_notices) =
        read_csv_from_reader_with_validation(data, file_name, |record, line| {
            if has_header_errors {
                Vec::new()
            } else {
                validator.validate_row(record, line)
            }
        })
        .map_err(GtfsInputError::Csv)?;

    if !has_header_errors {
        for notice in row_notices {
            notices.push(notice);
        }
    }
    for error in errors {
        if !skip_csv_parse_error(&table, &error) {
            notices.push_csv_error(&error);
        }
    }
    Ok(table)
}

fn strip_utf8_bom(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    }
}

/// Upper bound on the uncompressed size of a single zip member, to defend
/// against zip bombs when reading an untrusted archive into memory. The default
/// is generous so that legitimately large feeds (e.g. a multi-hundred-MB
/// `stop_times.txt`) still load; a deployment handling untrusted uploads should
/// lower it via `GTFS_VALIDATOR_MAX_MEMBER_BYTES`.
fn max_member_bytes() -> u64 {
    const DEFAULT_MAX_MEMBER_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
    std::env::var("GTFS_VALIDATOR_MAX_MEMBER_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_MEMBER_BYTES)
}

/// How far the single-threaded reader will inflate a member looking for the end
/// of its header row. Far above any real GTFS header, and bounded so a member
/// that is one enormous line cannot be decompressed whole by the header pass.
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
const HEADER_SCAN_BYTES: u64 = 1 << 20;

/// Upper bound on the *total* uncompressed size of a single archive, summed
/// across every member read from it. This backstops [`max_member_bytes`]: even
/// if every individual member stays under the per-member cap, an archive full of
/// large members cannot make the process decompress an unbounded volume.
/// Overridable via `GTFS_VALIDATOR_MAX_TOTAL_BYTES`.
fn max_total_bytes() -> u64 {
    const DEFAULT_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024; // 8 GiB
    std::env::var("GTFS_VALIDATOR_MAX_TOTAL_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_TOTAL_BYTES)
}

/// Used for both zip members and files read straight off disk, so the wording
/// stays true either way: "uncompressed zip member" would be wrong for the
/// directory reader, which shares this cap.
fn member_too_large(path: &Path, file_name: &str, observed: u64, limit: u64) -> GtfsInputError {
    GtfsInputError::ZipFileIo {
        path: path.to_path_buf(),
        file: file_name.to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "'{}' is {} bytes, exceeding the {}-byte per-file limit",
                file_name, observed, limit
            ),
        ),
    }
}

fn archive_budget_exceeded(path: &Path, file_name: &str, limit: u64) -> GtfsInputError {
    GtfsInputError::ZipFileIo {
        path: path.to_path_buf(),
        file: file_name.to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "archive exceeds the {}-byte total decompression limit while reading '{}'",
                limit, file_name
            ),
        ),
    }
}

/// Atomically deduct `amount` from the shared per-archive decompression budget.
/// Returns `Err(())` (never over-subtracting) if the running total would exceed
/// the cap, so concurrent member reads cannot collectively slip past it.
fn charge_archive_budget(budget: &AtomicU64, amount: u64) -> Result<(), ()> {
    if amount == 0 {
        return Ok(());
    }
    let mut current = budget.load(Ordering::Relaxed);
    loop {
        if amount > current {
            return Err(());
        }
        match budget.compare_exchange_weak(
            current,
            current - amount,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Ok(()),
            Err(observed) => current = observed,
        }
    }
}

/// Read a whole zip member into memory, refusing to allocate more than
/// [`max_member_bytes`] for it or to push the archive total past
/// [`max_total_bytes`]. Both the declared uncompressed size and the actual
/// number of decompressed bytes are checked, so a lying zip header cannot slip
/// past the cap. The archive budget is charged as bytes stream out, so parallel
/// member reads cannot each allocate a full member before any of them debit.
fn read_zip_member_capped(
    zipped: zip::read::ZipFile<'_>,
    path: &Path,
    file_name: &str,
    budget: &AtomicU64,
) -> Result<Vec<u8>, GtfsInputError> {
    let cap = max_member_bytes();
    let total_cap = max_total_bytes();
    if zipped.size() > cap {
        return Err(member_too_large(path, file_name, zipped.size(), cap));
    }
    if zipped.size() > budget.load(Ordering::Relaxed) {
        return Err(archive_budget_exceeded(path, file_name, total_cap));
    }
    read_capped_to_vec(zipped, path, file_name, cap, budget, None)
}

fn read_filesystem_file_capped(
    path: PathBuf,
    file_name: &str,
    budget: &AtomicU64,
) -> Result<Vec<u8>, GtfsInputError> {
    let cap = max_member_bytes();
    let total_cap = max_total_bytes();
    let declared = std::fs::metadata(&path)
        .map_err(|err| GtfsInputError::Io {
            path: path.clone(),
            source: err,
        })?
        .len();
    if declared > cap {
        return Err(member_too_large(&path, file_name, declared, cap));
    }
    if declared > budget.load(Ordering::Relaxed) {
        return Err(archive_budget_exceeded(&path, file_name, total_cap));
    }
    let file = File::open(&path).map_err(|err| GtfsInputError::Io {
        path: path.clone(),
        source: err,
    })?;
    read_capped_to_vec(file, &path, file_name, cap, budget, Some(declared))
}

/// `capacity_hint` is the size to preallocate, and must come from a source that
/// cannot lie. `File::read_to_end` reserves from the real file size, and
/// wrapping the file in [`CappedReader`] loses that, so the caller passes it
/// back in. A zip member's declared size is written by whoever built the
/// archive, so the zip path passes `None` rather than let a header reserve
/// gigabytes it never fills.
fn read_capped_to_vec<R: Read>(
    reader: R,
    path: &Path,
    file_name: &str,
    cap: u64,
    budget: &AtomicU64,
    capacity_hint: Option<u64>,
) -> Result<Vec<u8>, GtfsInputError> {
    let mut capped = CappedReader::new(reader, path, file_name, cap, budget);
    let mut buffer = match capacity_hint {
        Some(size) => Vec::with_capacity(size.min(cap) as usize),
        None => Vec::new(),
    };
    capped
        .read_to_end(&mut buffer)
        .map_err(|err| map_capped_read_error(path, file_name, cap, err))?;
    Ok(buffer)
}

fn map_capped_read_error(
    path: &Path,
    file_name: &str,
    cap: u64,
    err: std::io::Error,
) -> GtfsInputError {
    match limit_kind(&err) {
        Some(LimitKind::Total) => archive_budget_exceeded(path, file_name, max_total_bytes()),
        Some(LimitKind::Member) => member_too_large(path, file_name, cap.saturating_add(1), cap),
        None => GtfsInputError::ZipFileIo {
            path: path.to_path_buf(),
            file: file_name.to_string(),
            source: err,
        },
    }
}

/// Which cap a [`CappedReader`] hit. Carried as the payload of the `io::Error`
/// the reader yields so the error can be recognised by type rather than by the
/// wording of a `format!`, which nothing would keep in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitKind {
    /// The per-member cap, [`max_member_bytes`].
    Member,
    /// The archive-wide budget, [`max_total_bytes`].
    Total,
}

#[derive(Debug)]
struct LimitExceeded {
    kind: LimitKind,
    file_name: String,
    limit: u64,
}

impl std::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            LimitKind::Member => write!(
                f,
                "zip member '{}' exceeds the {}-byte per-file limit",
                self.file_name, self.limit
            ),
            LimitKind::Total => write!(
                f,
                "archive exceeds the {}-byte total decompression limit while reading '{}'",
                self.limit, self.file_name
            ),
        }
    }
}

impl std::error::Error for LimitExceeded {}

fn limit_io_error(kind: LimitKind, file_name: &str, limit: u64) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        LimitExceeded {
            kind,
            file_name: file_name.to_string(),
            limit,
        },
    )
}

/// The cap an `io::Error` reports hitting, if it is one of ours.
fn limit_kind(err: &std::io::Error) -> Option<LimitKind> {
    err.get_ref()
        .and_then(|inner| inner.downcast_ref::<LimitExceeded>())
        .map(|exceeded| exceeded.kind)
}

/// Re-wrap the limit error a CSV reader swallowed, preserving the sentinel so
/// the caller still recognises it by type.
#[cfg(feature = "parallel")]
fn csv_limit_io_error(err: &csv::Error) -> Option<std::io::Error> {
    let csv::ErrorKind::Io(io_err) = err.kind() else {
        return None;
    };
    let exceeded = io_err.get_ref()?.downcast_ref::<LimitExceeded>()?;
    Some(limit_io_error(
        exceeded.kind,
        &exceeded.file_name,
        exceeded.limit,
    ))
}

/// A `Read` adapter that enforces the per-member and archive-wide decompression
/// caps as bytes stream out of a zip member. The streaming CSV reader never
/// buffers a whole member, so without this a large member would be decompressed
/// past the cap one batch at a time. On overflow it yields an
/// `InvalidData` io error. Reads are sized so a lying header cannot inflate
/// more than one extra byte past the cap.
struct CappedReader<'a, R> {
    inner: R,
    file_name: String,
    member_remaining: u64,
    member_cap: u64,
    total_cap: u64,
    budget: &'a AtomicU64,
    /// Set when a cap is hit. A reader handed to a CSV parser is moved out of
    /// reach, and the parser flattens the `io::Error` into a string, so this is
    /// how the caller recovers the reason by value rather than by substring.
    limit_hit: Arc<AtomicU8>,
}

const LIMIT_HIT_NONE: u8 = 0;
const LIMIT_HIT_MEMBER: u8 = 1;
const LIMIT_HIT_TOTAL: u8 = 2;

impl<'a, R> CappedReader<'a, R> {
    fn new(
        inner: R,
        _path: &Path,
        file_name: &str,
        member_cap: u64,
        budget: &'a AtomicU64,
    ) -> Self {
        Self {
            inner,
            file_name: file_name.to_string(),
            member_remaining: member_cap,
            member_cap,
            total_cap: max_total_bytes(),
            budget,
            limit_hit: Arc::new(AtomicU8::new(LIMIT_HIT_NONE)),
        }
    }

    /// A handle to this reader's limit flag, to keep after the reader is moved
    /// into a parser. Only the single-threaded reader needs it: the parallel
    /// path keeps the `io::Error` and reads the sentinel off it directly.
    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    fn limit_flag(&self) -> Arc<AtomicU8> {
        self.limit_hit.clone()
    }
}

/// The cap a [`CappedReader`] hit, read back through a flag from
/// [`CappedReader::limit_flag`].
#[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
fn limit_kind_from_flag(flag: &AtomicU8) -> Option<LimitKind> {
    match flag.load(Ordering::Relaxed) {
        LIMIT_HIT_MEMBER => Some(LimitKind::Member),
        LIMIT_HIT_TOTAL => Some(LimitKind::Total),
        _ => None,
    }
}

impl<R: Read> Read for CappedReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Read at most remaining+1 so an exact-sized member still reaches EOF,
        // while one extra decompressed byte is enough to reject a lying header.
        let max_read = match self.member_remaining.checked_add(1) {
            Some(plus_one) => plus_one.min(buf.len() as u64) as usize,
            None => buf.len(),
        };
        let n = self.inner.read(&mut buf[..max_read])?;
        if n == 0 {
            return Ok(0);
        }
        let n64 = n as u64;
        if n64 > self.member_remaining {
            self.limit_hit.store(LIMIT_HIT_MEMBER, Ordering::Relaxed);
            return Err(limit_io_error(
                LimitKind::Member,
                &self.file_name,
                self.member_cap,
            ));
        }
        self.member_remaining -= n64;
        if charge_archive_budget(self.budget, n64).is_err() {
            self.limit_hit.store(LIMIT_HIT_TOTAL, Ordering::Relaxed);
            return Err(limit_io_error(
                LimitKind::Total,
                &self.file_name,
                self.total_cap,
            ));
        }
        Ok(n)
    }
}

/// Locate the best-matching root-level zip member for `file_name`, returning its
/// index. Mirrors the matching used by [`GtfsInputReader::read_from_zip`]: an
/// exact name match wins, otherwise a case-insensitive match (preferring the
/// lexicographically smallest name). Nested members are ignored.
fn locate_zip_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    file_name: &str,
) -> Result<Option<usize>, GtfsInputError> {
    let target = file_name.to_ascii_lowercase();
    let mut ci_index: Option<usize> = None;
    let mut ci_name: Option<String> = None;
    for index in 0..archive.len() {
        let (name, is_dir) = {
            let file = archive
                .by_index(index)
                .map_err(|err| GtfsInputError::ZipFile {
                    file: file_name.to_string(),
                    source: err,
                })?;
            (file.name().to_string(), file.is_dir())
        };
        if is_dir {
            continue;
        }
        if name.contains('/') || name.contains('\\') {
            continue; // root-level members only
        }
        if name == file_name {
            return Ok(Some(index)); // exact match wins
        }
        let lower = name.to_ascii_lowercase();
        if lower == target {
            let replace = ci_name.as_ref().map(|best| lower < *best).unwrap_or(true);
            if replace {
                ci_index = Some(index);
                ci_name = Some(lower);
            }
        }
    }
    Ok(ci_index)
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false)
}

fn list_files_in_directory(path: &Path) -> Result<Vec<String>, GtfsInputError> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|err| GtfsInputError::Io {
        path: path.to_path_buf(),
        source: err,
    })? {
        let entry = entry.map_err(|err| GtfsInputError::Io {
            path: path.to_path_buf(),
            source: err,
        })?;
        let file_type = entry.file_type().map_err(|err| GtfsInputError::Io {
            path: path.to_path_buf(),
            source: err,
        })?;
        if file_type.is_file() {
            files.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    Ok(files)
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), GtfsInputError> {
    for entry in std::fs::read_dir(current).map_err(|err| GtfsInputError::Io {
        path: current.to_path_buf(),
        source: err,
    })? {
        let entry = entry.map_err(|err| GtfsInputError::Io {
            path: current.to_path_buf(),
            source: err,
        })?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(|err| GtfsInputError::Io {
            path: entry_path.clone(),
            source: err,
        })?;
        if file_type.is_dir() {
            collect_files(root, &entry_path, files)?;
        } else if file_type.is_file() {
            let rel = entry_path
                .strip_prefix(root)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .to_string();
            files.push(rel);
        }
    }
    Ok(())
}

fn has_nested_gtfs_file_in_directory(path: &Path) -> Result<bool, GtfsInputError> {
    let mut files = Vec::new();
    collect_files(path, path, &mut files)?;
    for rel in files {
        let normalized = rel.replace('\\', "/");
        if !normalized.contains('/') {
            continue;
        }
        let file_name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
        if GTFS_FILE_NAMES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(file_name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn list_files_in_zip(path: &Path) -> Result<Vec<String>, GtfsInputError> {
    let file = File::open(path).map_err(|err| GtfsInputError::Io {
        path: path.to_path_buf(),
        source: err,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| GtfsInputError::ZipArchive {
        path: path.to_path_buf(),
        source: err,
    })?;

    let mut files = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: path.to_string_lossy().to_string(),
                source: err,
            })?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        if name.contains('/') || name.contains('\\') {
            continue;
        }
        files.push(name);
    }
    Ok(files)
}

fn has_nested_gtfs_file_in_zip(path: &Path) -> Result<bool, GtfsInputError> {
    let file = File::open(path).map_err(|err| GtfsInputError::Io {
        path: path.to_path_buf(),
        source: err,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| GtfsInputError::ZipArchive {
        path: path.to_path_buf(),
        source: err,
    })?;

    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: path.to_string_lossy().to_string(),
                source: err,
            })?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        if !(name.contains('/') || name.contains('\\')) {
            continue;
        }
        let file_name = name
            .rsplit(|ch| ch == '/' || ch == '\\')
            .next()
            .unwrap_or(name.as_str());
        if GTFS_FILE_NAMES
            .iter()
            .any(|gtfs| gtfs.eq_ignore_ascii_case(file_name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reader for GTFS data from in-memory bytes (for WASM compatibility)
#[derive(Clone)]
pub struct GtfsBytesReader {
    data: Vec<u8>,
    remaining_bytes: Arc<AtomicU64>,
}

impl GtfsBytesReader {
    /// Create a new reader from ZIP file bytes
    pub fn from_zip_bytes(data: Vec<u8>) -> Self {
        Self {
            data,
            remaining_bytes: Arc::new(AtomicU64::new(max_total_bytes())),
        }
    }

    /// Create a new reader from a byte slice (copies the data)
    pub fn from_slice(data: &[u8]) -> Self {
        Self::from_zip_bytes(data.to_vec())
    }

    pub fn get_files_with_sizes(&self) -> Result<HashMap<String, u64>, GtfsInputError> {
        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;

        let mut files = HashMap::new();
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|err| GtfsInputError::ZipFile {
                    file: "<memory>".into(),
                    source: err,
                })?;
            if !file.is_dir() {
                let name = file.name().to_string();
                if !(name.contains('/') || name.contains('\\')) {
                    files.insert(name, file.size());
                }
            }
        }
        Ok(files)
    }

    pub fn read_file(&self, file_name: &str) -> Result<Vec<u8>, GtfsInputError> {
        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;

        // Try exact match first
        match archive.by_name(file_name) {
            Ok(zipped) => {
                return read_zip_member_capped(
                    zipped,
                    Path::new("<memory>"),
                    file_name,
                    &self.remaining_bytes,
                );
            }
            Err(zip::result::ZipError::FileNotFound) => {}
            Err(err) => {
                return Err(GtfsInputError::ZipFile {
                    file: file_name.to_string(),
                    source: err,
                });
            }
        }

        // Case-insensitive search with preference for root-level files
        let target = file_name.to_ascii_lowercase();
        let mut matched_index = None;
        let mut matched_depth = None;
        let mut matched_name = None;

        for index in 0..archive.len() {
            let (name, is_dir) = {
                let file = archive
                    .by_index(index)
                    .map_err(|err| GtfsInputError::ZipFile {
                        file: file_name.to_string(),
                        source: err,
                    })?;
                (file.name().to_string(), file.is_dir())
            };
            if is_dir {
                continue;
            }
            if name.contains('/') || name.contains('\\') {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let tail = lower
                .rsplit(|ch| ch == '/' || ch == '\\')
                .next()
                .unwrap_or(lower.as_str());
            if tail != target {
                continue;
            }
            let depth = name.matches(|ch| ch == '/' || ch == '\\').count();
            match matched_depth {
                None => {
                    matched_index = Some(index);
                    matched_depth = Some(depth);
                    matched_name = Some(lower);
                }
                Some(current_depth) if depth < current_depth => {
                    matched_index = Some(index);
                    matched_depth = Some(depth);
                    matched_name = Some(lower);
                }
                Some(current_depth) if depth == current_depth => {
                    let should_replace = matched_name
                        .as_ref()
                        .map(|best| lower < *best)
                        .unwrap_or(true);
                    if should_replace {
                        matched_index = Some(index);
                        matched_name = Some(lower);
                    }
                }
                _ => {}
            }
        }

        let Some(index) = matched_index else {
            return Err(GtfsInputError::MissingFile(file_name.to_string()));
        };

        let zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        read_zip_member_capped(
            zipped,
            Path::new("<memory>"),
            file_name,
            &self.remaining_bytes,
        )
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    fn read_optional_csv_streaming_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        // Read just the header first to configure validation. Reopening the ZIP
        // member below costs one tiny inflate but avoids materializing the full
        // uncompressed CSV in WASM memory.
        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;
        let Some(index) = locate_zip_member(&mut archive, file_name)? else {
            return Ok(None);
        };
        let zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        let cap = max_member_bytes();
        if zipped.size() > cap {
            return Err(member_too_large(
                Path::new("<memory>"),
                file_name,
                zipped.size(),
                cap,
            ));
        }
        if zipped.size() == 0 {
            notices.push_empty_table(file_name);
            return Ok(Some(CsvTable::default()));
        }
        let mut header_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .trim(csv::Trim::Headers)
            .from_reader(zipped.take(HEADER_SCAN_BYTES + 1));
        let headers_record = header_reader
            .headers()
            .map_err(|err| {
                GtfsInputError::Csv(crate::csv_reader::map_csv_error(file_name, None, err))
            })?
            .clone();
        // The header pass is bounded so a member with no line break cannot be
        // inflated whole just to find the columns. Hitting that bound means the
        // record was cut mid-field, and the second pass would then read a longer
        // header than the `RowValidator` was built from -- so refuse instead of
        // validating against a header we know is wrong.
        if header_reader.into_inner().limit() == 0 {
            return Err(GtfsInputError::ZipFileIo {
                path: PathBuf::from("<memory>"),
                file: file_name.to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "'{}' has no complete header row in its first {} bytes",
                        file_name, HEADER_SCAN_BYTES
                    ),
                ),
            });
        }
        let headers: Vec<String> = headers_record.iter().map(str::to_string).collect();

        let mut header_notices = NoticeContainer::new();
        validate_headers(file_name, &headers, &mut header_notices);
        let has_header_errors = header_notices
            .iter()
            .any(|notice| notice.severity == NoticeSeverity::Error);
        notices.merge(header_notices);
        let validator = RowValidator::new(file_name, headers);
        drop(archive);

        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;
        let zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        let capped = CappedReader::new(
            zipped,
            Path::new("<memory>"),
            file_name,
            cap,
            &self.remaining_bytes,
        );
        // The parser flattens our `io::Error` into a message string, so the
        // reason is read back off the reader's own flag instead.
        let limit_flag = capped.limit_flag();
        let (table, errors, row_notices) =
            read_csv_from_reader_with_validation(capped, file_name, |record, line| {
                if has_header_errors {
                    Vec::new()
                } else {
                    validator.validate_row(record, line)
                }
            })
            .map_err(|err| match limit_kind_from_flag(&limit_flag) {
                Some(kind) => {
                    let limit = match kind {
                        LimitKind::Member => cap,
                        LimitKind::Total => max_total_bytes(),
                    };
                    GtfsInputError::ZipFileIo {
                        path: PathBuf::from("<memory>"),
                        file: file_name.to_string(),
                        source: limit_io_error(kind, file_name, limit),
                    }
                }
                None => GtfsInputError::Csv(err),
            })?;

        if !has_header_errors {
            for notice in row_notices {
                notices.push(notice);
            }
        }
        for error in errors {
            if !skip_csv_parse_error(&table, &error) {
                notices.push_csv_error(&error);
            }
        }
        Ok(Some(table))
    }

    pub fn read_csv<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        read_csv_from_reader(data_str.as_bytes(), file_name).map_err(GtfsInputError::Csv)
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    pub fn read_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        pool: &crate::StringPool,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        let data_bytes = data_str.as_bytes();
        // Peek headers for validator setup
        let mut peek_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .trim(csv::Trim::None)
            .from_reader(data_bytes);

        let headers_record = match peek_reader.headers() {
            Ok(h) => h.clone(),
            Err(_) => {
                let (table, _, _) =
                    read_csv_from_reader_parallel(data_bytes, file_name, |_, _| Vec::new(), pool)
                        .map_err(GtfsInputError::Csv)?;
                return Ok(table);
            }
        };

        let headers: Vec<String> = headers_record.iter().map(|s| s.to_string()).collect();
        validate_headers(file_name, &headers, notices);
        let validator = RowValidator::new(file_name, headers);

        let (table, errors, row_notices) = read_csv_from_reader_parallel(
            data_bytes,
            file_name,
            |record, line| validator.validate_row(record, line),
            pool,
        )
        .map_err(GtfsInputError::Csv)?;

        for notice in row_notices {
            notices.push(notice);
        }
        for error in errors {
            if skip_csv_parse_error(&table, &error) {
                continue;
            }
            notices.push_csv_error(&error);
        }

        Ok(table)
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    pub fn read_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        _pool: &crate::StringPool,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        self.read_optional_csv_streaming_with_notices(file_name, notices)?
            .ok_or_else(|| GtfsInputError::MissingFile(file_name.to_string()))
    }

    pub fn read_optional_csv<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                read_csv_from_reader(data_str.as_bytes(), file_name)
                    .map(Some)
                    .map_err(GtfsInputError::Csv)
            }
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        pool: &crate::StringPool,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                let data_bytes = data_str.as_bytes();
                // Peek headers for validator setup
                let mut peek_reader = csv::ReaderBuilder::new()
                    .has_headers(true)
                    .flexible(true)
                    .trim(csv::Trim::None)
                    .from_reader(data_bytes);

                let headers_record = match peek_reader.headers() {
                    Ok(h) => h.clone(),
                    Err(_) => {
                        let (table, _, _) = read_csv_from_reader_parallel(
                            data_bytes,
                            file_name,
                            |_, _| Vec::new(),
                            pool,
                        )
                        .map_err(GtfsInputError::Csv)?;
                        return Ok(Some(table));
                    }
                };

                let headers: Vec<String> = headers_record.iter().map(|s| s.to_string()).collect();
                validate_headers(file_name, &headers, notices);
                let validator = RowValidator::new(file_name, headers);

                let (table, errors, row_notices) = read_csv_from_reader_parallel(
                    data_bytes,
                    file_name,
                    |record, line| validator.validate_row(record, line),
                    pool,
                )
                .map_err(GtfsInputError::Csv)?;

                for notice in row_notices {
                    notices.push(notice);
                }
                for error in errors {
                    if skip_csv_parse_error(&table, &error) {
                        continue;
                    }
                    notices.push_csv_error(&error);
                }

                Ok(Some(table))
            }
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
        _pool: &crate::StringPool,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        self.read_optional_csv_streaming_with_notices(file_name, notices)
    }

    pub fn read_json<T: DeserializeOwned>(&self, file_name: &str) -> Result<T, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data = strip_utf8_bom(&data);
        serde_json::from_slice(data).map_err(|err| GtfsInputError::Json {
            file: file_name.to_string(),
            source: err,
        })
    }

    pub fn read_optional_json<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<Option<T>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => serde_json::from_slice(strip_utf8_bom(&data))
                .map(Some)
                .map_err(|err| GtfsInputError::Json {
                    file: file_name.to_string(),
                    source: err,
                }),
            Err(GtfsInputError::MissingFile(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn list_files(&self) -> Result<Vec<String>, GtfsInputError> {
        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;

        let mut files = Vec::new();
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|err| GtfsInputError::ZipFile {
                    file: "<memory>".into(),
                    source: err,
                })?;
            if file.is_dir() {
                continue;
            }
            let name = file.name().to_string();
            if name.contains('/') || name.contains('\\') {
                continue;
            }
            files.push(name);
        }
        Ok(files)
    }

    pub fn has_nested_gtfs_files(&self) -> Result<bool, GtfsInputError> {
        let cursor = Cursor::new(&self.data);
        let mut archive = ZipArchive::new(cursor).map_err(|err| GtfsInputError::ZipArchive {
            path: PathBuf::from("<memory>"),
            source: err,
        })?;

        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|err| GtfsInputError::ZipFile {
                    file: "<memory>".into(),
                    source: err,
                })?;
            if file.is_dir() {
                continue;
            }
            let name = file.name().to_string();
            if !(name.contains('/') || name.contains('\\')) {
                continue;
            }
            let file_name = name
                .rsplit(|ch| ch == '/' || ch == '\\')
                .next()
                .unwrap_or(name.as_str());
            if GTFS_FILE_NAMES
                .iter()
                .any(|gtfs| gtfs.eq_ignore_ascii_case(file_name))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn unknown_file_notice(file_name: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unknown_file",
        NoticeSeverity::Info,
        "unknown file in input",
    );
    notice.insert_context_field("filename", file_name);
    notice.field_order = vec!["filename".into()];
    notice
}

fn invalid_input_files_notice() -> ValidationNotice {
    ValidationNotice::new(
        "invalid_input_files_in_subfolder",
        NoticeSeverity::Error,
        "GTFS file found in subfolder",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoticeContainer;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::Deserialize;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    #[derive(Debug, Deserialize)]
    struct ExampleRow {
        a: i32,
        b: i32,
    }

    fn temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
    }

    #[test]
    fn reads_file_from_directory() {
        let dir = temp_path("gtfs_dir");
        fs::create_dir_all(&dir).expect("create dir");
        let file_path = dir.join("stops.txt");
        fs::write(&file_path, b"a,b\n1,2\n").expect("write file");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let data = reader.read_file("stops.txt").expect("read file");
        assert_eq!(data, b"a,b\n1,2\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn directory_reader_rejects_symlinks() {
        let dir = temp_path("gtfs_dir_symlink");
        fs::create_dir_all(&dir).expect("create dir");
        let target = dir.join("real.txt");
        fs::write(&target, b"a,b\n1,2\n").expect("write target");
        let link = dir.join("stops.txt");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let err = reader
            .read_file("stops.txt")
            .expect_err("symlink must fail");
        assert!(matches!(err, GtfsInputError::NotAFile(_)));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_csv_from_zip() {
        let dir = temp_path("gtfs_zip");
        fs::create_dir_all(&dir).expect("create dir");
        let zip_path = dir.join("feed.zip");

        let zip_file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(zip_file);
        let options = FileOptions::default();
        zip.start_file("stops.txt", options).expect("zip file");
        zip.write_all(b"a,b\n3,4\n").expect("zip data");
        zip.finish().expect("finish zip");

        let input = GtfsInput::from_path(&zip_path).expect("input");
        let reader = input.reader();
        let table = reader
            .read_csv::<ExampleRow>("stops.txt")
            .expect("read csv");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].a, 3);
        assert_eq!(table.rows[0].b, 4);

        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn streaming_csv_member_read_error_is_not_silently_loaded() {
        let dir = temp_path("gtfs_streaming_bad_member");
        fs::create_dir_all(&dir).expect("create dir");
        let zip_path = dir.join("feed.zip");

        let zip_file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(zip_file);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("stops.txt", options).expect("zip file");
        zip.write_all(b"a,b\n1,2\n").expect("zip data");
        zip.finish().expect("finish zip");

        let mut zip_bytes = fs::read(&zip_path).expect("read zip");
        assert_eq!(&zip_bytes[0..4], b"PK\x03\x04");
        let name_len = u16::from_le_bytes([zip_bytes[26], zip_bytes[27]]) as usize;
        let extra_len = u16::from_le_bytes([zip_bytes[28], zip_bytes[29]]) as usize;
        let data_start = 30 + name_len + extra_len;
        zip_bytes[data_start] ^= 0xff;
        fs::write(&zip_path, zip_bytes).expect("corrupt zip member data");

        let input = GtfsInput::from_path(&zip_path).expect("input");
        let reader = input.reader();
        let mut notices = NoticeContainer::new();
        let pool = crate::StringPool::new();

        let err = reader
            .read_optional_csv_streaming::<ExampleRow>("stops.txt", &mut notices, &pool)
            .expect_err("member read error must be a CSV error");

        match err {
            GtfsInputError::Csv(err) => {
                assert_eq!(err.file, "stops.txt");
                assert!(!err.message.is_empty());
            }
            other => panic!("expected CSV error, got {other:?}"),
        }
        assert!(notices.is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn charge_archive_budget_enforces_total_without_wrapping() {
        let budget = AtomicU64::new(10);
        assert!(charge_archive_budget(&budget, 4).is_ok());
        assert!(charge_archive_budget(&budget, 6).is_ok());
        assert_eq!(budget.load(Ordering::Relaxed), 0);
        // Exhausted budget: a further non-zero charge fails and must not wrap the
        // counter back up to a huge value.
        assert!(charge_archive_budget(&budget, 1).is_err());
        assert_eq!(budget.load(Ordering::Relaxed), 0);
        // Zero-length reads are always free.
        assert!(charge_archive_budget(&budget, 0).is_ok());
    }

    #[test]
    fn capped_reader_rejects_member_over_per_file_cap() {
        let data = vec![b'x'; 4096];
        let budget = AtomicU64::new(u64::MAX);
        let mut reader = CappedReader::new(
            &data[..],
            Path::new("<test>"),
            "stop_times.txt",
            16,
            &budget,
        );
        let mut out = Vec::new();
        let err = reader
            .read_to_end(&mut out)
            .expect_err("reading past the per-member cap must error");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("per-file limit"));
    }

    #[test]
    fn capped_reader_rejects_archive_over_total_budget() {
        let data = vec![b'x'; 4096];
        // Generous per-member cap, but the archive budget is nearly gone.
        let budget = AtomicU64::new(16);
        let mut reader = CappedReader::new(
            &data[..],
            Path::new("<test>"),
            "stop_times.txt",
            u64::MAX,
            &budget,
        );
        let mut out = Vec::new();
        let err = reader
            .read_to_end(&mut out)
            .expect_err("reading past the archive budget must error");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("total decompression limit"));
    }

    #[test]
    fn limit_errors_carry_their_kind_through_a_csv_error() {
        let member = limit_io_error(LimitKind::Member, "stops.txt", 16);
        assert_eq!(limit_kind(&member), Some(LimitKind::Member));
        assert!(member.to_string().contains("per-file limit"));

        let total = limit_io_error(LimitKind::Total, "stops.txt", 16);
        assert_eq!(limit_kind(&total), Some(LimitKind::Total));
        assert!(total.to_string().contains("total decompression limit"));

        // A plain io error must not be mistaken for one of ours, however it
        // happens to be worded.
        let impostor = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "cell text mentioning a per-file limit and a total decompression limit",
        );
        assert_eq!(limit_kind(&impostor), None);
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn csv_limit_io_error_preserves_the_sentinel() {
        let wrapped = csv::Error::from(limit_io_error(LimitKind::Total, "stops.txt", 32));
        let recovered =
            csv_limit_io_error(&wrapped).expect("limit error must survive the csv wrap");
        assert_eq!(limit_kind(&recovered), Some(LimitKind::Total));

        let unrelated = csv::Error::from(std::io::Error::other("disk gone"));
        assert!(csv_limit_io_error(&unrelated).is_none());
    }

    #[cfg(any(not(feature = "parallel"), target_arch = "wasm32"))]
    #[test]
    fn header_pass_refuses_a_member_with_no_header_row_in_range() {
        use std::io::Write as _;

        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            zip.start_file("stops.txt", FileOptions::default())
                .expect("zip file");
            // One unterminated line, longer than the header scan window.
            zip.write_all(&vec![b'a'; (HEADER_SCAN_BYTES + 2048) as usize])
                .expect("zip data");
            zip.finish().expect("finish zip");
        }

        let reader = GtfsBytesReader::from_zip_bytes(buffer);
        let mut notices = NoticeContainer::new();
        let err = reader
            .read_optional_csv_with_notices::<ExampleRow>(
                "stops.txt",
                &mut notices,
                &crate::StringPool::new(),
            )
            .expect_err("a header row that never ends must be refused");
        assert!(
            err.to_string().contains("no complete header row"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn capped_reader_charges_budget_as_bytes_are_read() {
        let data = vec![b'x'; 32];
        let budget = AtomicU64::new(100);
        let mut reader =
            CappedReader::new(&data[..], Path::new("<test>"), "stops.txt", 100, &budget);
        let mut buf = [0u8; 8];
        let n = reader.read(&mut buf).expect("read");
        assert_eq!(n, 8);
        assert_eq!(budget.load(Ordering::Relaxed), 92);
    }

    #[test]
    fn reads_file_from_directory_case_insensitive() {
        let dir = temp_path("gtfs_dir_case");
        fs::create_dir_all(&dir).expect("create dir");
        let file_path = dir.join("Stops.TXT");
        fs::write(&file_path, b"a,b\n7,8\n").expect("write file");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let data = reader.read_file("stops.txt").expect("read file");
        assert_eq!(data, b"a,b\n7,8\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_file_from_directory_prefers_root_file() {
        let dir = temp_path("gtfs_dir_prefer_root");
        fs::create_dir_all(&dir).expect("create dir");
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(nested.join("stops.txt"), b"a,b\n1,2\n").expect("write file");
        fs::write(dir.join("Stops.TXT"), b"a,b\n3,4\n").expect("write file");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let table = reader
            .read_csv::<ExampleRow>("stops.txt")
            .expect("read csv");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].a, 3);
        assert_eq!(table.rows[0].b, 4);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_json_with_utf8_bom() {
        let dir = temp_path("gtfs_json_bom");
        fs::create_dir_all(&dir).expect("create dir");
        let file_path = dir.join("data.json");
        fs::write(&file_path, b"\xEF\xBB\xBF{\"value\": 1}").expect("write json");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let value: serde_json::Value = reader.read_json("data.json").expect("read json");
        assert_eq!(value["value"], 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skips_csv_parse_errors_for_validated_fields() {
        let dir = temp_path("gtfs_invalid_enum");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("routes.txt"), b"route_id,route_type\nR1,bad\n").expect("write routes");

        let input = GtfsInput::from_path(&dir).expect("input");
        let reader = input.reader();
        let mut notices = NoticeContainer::new();
        let pool = crate::StringPool::new();
        let table = reader
            .read_csv_with_notices::<gtfs_guru_model::Route>("routes.txt", &mut notices, &pool)
            .expect("read csv");

        assert!(table.rows.is_empty());
        assert!(notices
            .iter()
            .any(|notice| notice.code == "invalid_integer"));
        assert!(!notices
            .iter()
            .any(|notice| notice.code == "csv_parsing_failed"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_csv_from_nested_zip_file_case_insensitive() {
        let dir = temp_path("gtfs_zip_nested");
        fs::create_dir_all(&dir).expect("create dir");
        let zip_path = dir.join("feed.zip");

        let zip_file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(zip_file);
        let options = FileOptions::default();
        zip.start_file("Feed/Stops.TXT", options).expect("zip file");
        zip.write_all(b"a,b\n5,6\n").expect("zip data");
        zip.finish().expect("finish zip");

        let input = GtfsInput::from_path(&zip_path).expect("input");
        let reader = input.reader();
        let err = reader.read_csv::<ExampleRow>("stops.txt").unwrap_err();
        assert!(matches!(err, GtfsInputError::MissingFile(_)));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_csv_from_zip_prefers_root_file() {
        let dir = temp_path("gtfs_zip_root_prefer");
        fs::create_dir_all(&dir).expect("create dir");
        let zip_path = dir.join("feed.zip");

        let zip_file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(zip_file);
        let options = FileOptions::default();
        zip.start_file("nested/stops.txt", options)
            .expect("zip file");
        zip.write_all(b"a,b\n1,2\n").expect("zip data");
        zip.start_file("Stops.TXT", options).expect("zip file");
        zip.write_all(b"a,b\n9,10\n").expect("zip data");
        zip.finish().expect("finish zip");

        let input = GtfsInput::from_path(&zip_path).expect("input");
        let reader = input.reader();
        let table = reader
            .read_csv::<ExampleRow>("stops.txt")
            .expect("read csv");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].a, 9);
        assert_eq!(table.rows[0].b, 10);

        fs::remove_dir_all(&dir).ok();
    }
}

fn find_case_insensitive_file(dir: &Path, target: &str) -> Result<Option<PathBuf>, GtfsInputError> {
    let target_lower = target.to_ascii_lowercase();
    let entries = std::fs::read_dir(dir).map_err(|err| GtfsInputError::Io {
        path: dir.to_path_buf(),
        source: err,
    })?;
    let mut entries = entries
        .map(|entry| {
            entry.map_err(|err| GtfsInputError::Io {
                path: dir.to_path_buf(),
                source: err,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|a, b| {
        let a_name = a.file_name().to_string_lossy().into_owned();
        let b_name = b.file_name().to_string_lossy().into_owned();
        let a_lower = a_name.to_ascii_lowercase();
        let b_lower = b_name.to_ascii_lowercase();
        match a_lower.cmp(&b_lower) {
            std::cmp::Ordering::Equal => a_name.cmp(&b_name),
            other => other,
        }
    });

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| GtfsInputError::Io {
            path: dir.to_path_buf(),
            source: err,
        })?;

        if file_type.is_dir() {
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.to_ascii_lowercase() == target_lower {
            return Ok(Some(path));
        }
    }

    Ok(None)
}
