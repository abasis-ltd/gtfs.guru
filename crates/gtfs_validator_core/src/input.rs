use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use zip::ZipArchive;

#[cfg(feature = "parallel")]
use crate::csv_reader::read_csv_from_reader_parallel;
#[cfg(not(feature = "parallel"))]
use crate::csv_reader::read_csv_from_reader_with_errors;
use crate::csv_reader::{read_csv_from_reader, CsvParseError, CsvTable};
use crate::csv_validation::is_value_validated_field;
#[cfg(not(feature = "parallel"))]
use crate::csv_validation::validate_csv_data;
#[cfg(feature = "parallel")]
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

    if reader.has_nested_gtfs_files()? {
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
                    if path.is_file() {
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

    #[cfg(feature = "parallel")]
    pub fn read_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
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
                    read_csv_from_reader_parallel(data_bytes, file_name, |_, _| Vec::new(), || {})
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
            || {},
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

    #[cfg(not(feature = "parallel"))]
    pub fn read_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        let data_bytes = data_str.as_bytes();
        validate_csv_data(file_name, data_bytes, notices);
        let (table, errors) =
            read_csv_from_reader_with_errors(data_bytes, file_name).map_err(GtfsInputError::Csv)?;
        for error in errors {
            if skip_csv_parse_error(&table, &error) {
                continue;
            }
            notices.push_csv_error(&error);
        }
        Ok(table)
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

    #[cfg(feature = "parallel")]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
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
                            || {},
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
                    || {},
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

    #[cfg(not(feature = "parallel"))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                let data_bytes = data_str.as_bytes();
                validate_csv_data(file_name, data_bytes, notices);
                let (table, errors) = read_csv_from_reader_with_errors(data_bytes, file_name)
                    .map_err(GtfsInputError::Csv)?;
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

    fn read_from_directory(&self, file_name: &str) -> Result<Vec<u8>, GtfsInputError> {
        let path = self.path.join(file_name);
        if path.exists() {
            if !path.is_file() {
                return Err(GtfsInputError::NotAFile(path));
            }
            let mut file = File::open(&path).map_err(|err| GtfsInputError::Io {
                path: path.clone(),
                source: err,
            })?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|err| GtfsInputError::Io { path, source: err })?;
            return Ok(buffer);
        }

        let Some(found_path) = find_case_insensitive_file(&self.path, file_name)? else {
            return Err(GtfsInputError::MissingFile(file_name.to_string()));
        };
        if !found_path.is_file() {
            return Err(GtfsInputError::NotAFile(found_path));
        }

        let mut file = File::open(&found_path).map_err(|err| GtfsInputError::Io {
            path: found_path.clone(),
            source: err,
        })?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|err| GtfsInputError::Io {
                path: found_path,
                source: err,
            })?;
        Ok(buffer)
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
            Ok(mut zipped) => {
                let mut buffer = Vec::new();
                zipped
                    .read_to_end(&mut buffer)
                    .map_err(|err| GtfsInputError::ZipFileIo {
                        path: self.path.clone(),
                        file: file_name.to_string(),
                        source: err,
                    })?;
                return Ok(buffer);
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
        let mut zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        let mut buffer = Vec::new();
        zipped
            .read_to_end(&mut buffer)
            .map_err(|err| GtfsInputError::ZipFileIo {
                path: self.path.clone(),
                file: file_name.to_string(),
                source: err,
            })?;
        Ok(buffer)
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

fn strip_utf8_bom(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    }
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
        if entry_path.is_dir() {
            collect_files(root, &entry_path, files)?;
        } else if entry_path.is_file() {
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
}

impl GtfsBytesReader {
    /// Create a new reader from ZIP file bytes
    pub fn from_zip_bytes(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create a new reader from a byte slice (copies the data)
    pub fn from_slice(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
        }
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
            Ok(mut zipped) => {
                let mut buffer = Vec::new();
                zipped
                    .read_to_end(&mut buffer)
                    .map_err(|err| GtfsInputError::ZipFileIo {
                        path: PathBuf::from("<memory>"),
                        file: file_name.to_string(),
                        source: err,
                    })?;
                return Ok(buffer);
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

        let mut zipped = archive
            .by_index(index)
            .map_err(|err| GtfsInputError::ZipFile {
                file: file_name.to_string(),
                source: err,
            })?;
        let mut buffer = Vec::new();
        zipped
            .read_to_end(&mut buffer)
            .map_err(|err| GtfsInputError::ZipFileIo {
                path: PathBuf::from("<memory>"),
                file: file_name.to_string(),
                source: err,
            })?;
        Ok(buffer)
    }

    pub fn read_csv<T: DeserializeOwned>(
        &self,
        file_name: &str,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        read_csv_from_reader(data_str.as_bytes(), file_name).map_err(GtfsInputError::Csv)
    }

    #[cfg(feature = "parallel")]
    pub fn read_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
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
                    read_csv_from_reader_parallel(data_bytes, file_name, |_, _| Vec::new(), || {})
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
            || {},
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

    #[cfg(not(feature = "parallel"))]
    pub fn read_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
    ) -> Result<CsvTable<T>, GtfsInputError> {
        let data = self.read_file(file_name)?;
        let data_str = decode_utf8_lossy(&data);
        let data_bytes = data_str.as_bytes();
        validate_csv_data(file_name, data_bytes, notices);
        let (table, errors) =
            read_csv_from_reader_with_errors(data_bytes, file_name).map_err(GtfsInputError::Csv)?;
        for error in errors {
            if skip_csv_parse_error(&table, &error) {
                continue;
            }
            notices.push_csv_error(&error);
        }
        Ok(table)
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

    #[cfg(feature = "parallel")]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned + Send>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
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
                            || {},
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
                    || {},
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

    #[cfg(not(feature = "parallel"))]
    pub fn read_optional_csv_with_notices<T: DeserializeOwned>(
        &self,
        file_name: &str,
        notices: &mut NoticeContainer,
    ) -> Result<Option<CsvTable<T>>, GtfsInputError> {
        match self.read_file(file_name) {
            Ok(data) => {
                let data_str = decode_utf8_lossy(&data);
                let data_bytes = data_str.as_bytes();
                validate_csv_data(file_name, data_bytes, notices);
                let (table, errors) = read_csv_from_reader_with_errors(data_bytes, file_name)
                    .map_err(GtfsInputError::Csv)?;
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
        let table = reader
            .read_csv_with_notices::<gtfs_guru_model::Route>("routes.txt", &mut notices)
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
