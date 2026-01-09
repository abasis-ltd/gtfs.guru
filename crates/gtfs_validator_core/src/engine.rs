use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{
    input::{collect_input_notices, GtfsBytesReader},
    GtfsFeed, GtfsInput, GtfsInputError, NoticeContainer, NoticeSeverity, ValidationNotice,
    ValidatorRunner,
};

pub struct ValidationOutcome {
    pub feed: Option<GtfsFeed>,
    pub notices: NoticeContainer,
}

pub fn validate_input(input: &GtfsInput, runner: &ValidatorRunner) -> ValidationOutcome {
    validate_input_and_progress(input, runner, None)
}

pub fn validate_input_and_progress(
    input: &GtfsInput,
    runner: &ValidatorRunner,
    progress: Option<&dyn crate::ProgressHandler>,
) -> ValidationOutcome {
    let mut notices = NoticeContainer::new();

    if let Ok(input_notices) = collect_input_notices(input) {
        for notice in input_notices {
            notices.push(notice);
        }
    }

    let load_result = catch_unwind(AssertUnwindSafe(|| {
        GtfsFeed::from_input_with_notices_and_progress(input, &mut notices, progress)
    }));

    match load_result {
        Ok(Ok(feed)) => {
            runner.run_with_progress(&feed, &mut notices, progress);
            ValidationOutcome {
                feed: Some(feed),
                notices,
            }
        }
        Ok(Err(err)) => {
            push_input_error_notice(&mut notices, err);
            ValidationOutcome {
                feed: None,
                notices,
            }
        }
        Err(panic) => {
            notices.push(runtime_exception_in_loader_error_notice(
                input.path().display().to_string(),
                panic_payload_message(&*panic),
            ));
            ValidationOutcome {
                feed: None,
                notices,
            }
        }
    }
}

/// Validate GTFS feed from in-memory ZIP bytes (for WASM compatibility)
pub fn validate_bytes(zip_bytes: &[u8], runner: &ValidatorRunner) -> ValidationOutcome {
    let reader = GtfsBytesReader::from_slice(zip_bytes);
    validate_bytes_reader(&reader, runner)
}

/// Validate GTFS feed from a bytes reader
pub fn validate_bytes_reader(
    reader: &GtfsBytesReader,
    runner: &ValidatorRunner,
) -> ValidationOutcome {
    validate_bytes_reader_and_progress(reader, runner, None)
}

pub fn validate_bytes_reader_and_progress(
    reader: &GtfsBytesReader,
    runner: &ValidatorRunner,
    progress: Option<&dyn crate::ProgressHandler>,
) -> ValidationOutcome {
    let mut notices = NoticeContainer::new();

    // Collect input notices (unknown files, etc.)
    if let Ok(files) = reader.list_files() {
        let known: std::collections::HashSet<String> = crate::feed::GTFS_FILE_NAMES
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        for path in files {
            let normalized = path.replace('\\', "/");
            let file_name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
            if file_name.eq_ignore_ascii_case(".ds_store") {
                continue;
            }
            if !known.contains(&file_name.to_ascii_lowercase()) {
                notices.push(unknown_file_notice(file_name));
            }
        }
    }

    let load_result = catch_unwind(AssertUnwindSafe(|| {
        GtfsFeed::from_bytes_reader_with_notices_and_progress(reader, &mut notices, progress)
    }));

    match load_result {
        Ok(Ok(feed)) => {
            runner.run_with_progress(&feed, &mut notices, progress);
            ValidationOutcome {
                feed: Some(feed),
                notices,
            }
        }
        Ok(Err(err)) => {
            push_input_error_notice(&mut notices, err);
            ValidationOutcome {
                feed: None,
                notices,
            }
        }
        Err(panic) => {
            notices.push(runtime_exception_in_loader_error_notice(
                "<memory>".into(),
                panic_payload_message(&*panic),
            ));
            ValidationOutcome {
                feed: None,
                notices,
            }
        }
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

fn push_input_error_notice(notices: &mut NoticeContainer, err: GtfsInputError) {
    match err {
        GtfsInputError::MissingFile(name) => {
            notices.push_missing_file(name);
        }
        GtfsInputError::Csv(csv_err) => {
            notices.push_csv_error(&csv_err);
        }
        GtfsInputError::Json { file, source } => {
            notices.push(malformed_json_notice(&file, &source));
        }
        other => {
            notices.push(io_error_notice(&other));
        }
    }
}

fn malformed_json_notice(file: &str, source: &serde_json::Error) -> ValidationNotice {
    let mut notice =
        ValidationNotice::new("malformed_json", NoticeSeverity::Error, source.to_string());
    notice.file = Some(file.to_string());
    notice.insert_context_field("message", source.to_string());
    notice.field_order = vec!["filename".into(), "message".into()];
    notice
}

fn io_error_notice(error: &GtfsInputError) -> ValidationNotice {
    let (exception, message) = match error {
        GtfsInputError::MissingPath(_) => ("MissingPath", error.to_string()),
        GtfsInputError::InvalidPath(_) => ("InvalidPath", error.to_string()),
        GtfsInputError::InvalidZip(_) => ("InvalidZip", error.to_string()),
        GtfsInputError::NotAFile(_) => ("NotAFile", error.to_string()),
        GtfsInputError::Io { source, .. } => ("io::Error", source.to_string()),
        GtfsInputError::ZipArchive { source, .. } => ("zip::result::ZipError", source.to_string()),
        GtfsInputError::ZipFile { source, .. } => ("zip::result::ZipError", source.to_string()),
        GtfsInputError::ZipFileIo { source, .. } => ("io::Error", source.to_string()),
        GtfsInputError::Json { source, .. } => ("serde_json::Error", source.to_string()),
        GtfsInputError::MissingFile(_) | GtfsInputError::Csv(_) => {
            ("GtfsInputError", error.to_string())
        }
    };
    let mut notice = ValidationNotice::new("i_o_error", NoticeSeverity::Error, message.clone());
    notice.insert_context_field("exception", exception);
    notice.insert_context_field("message", message);
    notice.field_order = vec!["exception".into(), "message".into()];
    notice
}

fn runtime_exception_in_loader_error_notice(file: String, message: String) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "runtime_exception_in_loader_error",
        NoticeSeverity::Error,
        "runtime exception while loading gtfs",
    );
    notice.insert_context_field("exception", "panic");
    notice.insert_context_field("filename", file);
    notice.insert_context_field("message", message);
    notice.field_order = vec!["exception".into(), "filename".into(), "message".into()];
    notice
}

fn panic_payload_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic".into()
    }
}

#[allow(dead_code)]
fn uri_syntax_error_notice(exception: &str, message: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "u_r_i_syntax_error",
        NoticeSeverity::Error,
        "uri syntax error",
    );
    notice.insert_context_field("exception", exception);
    notice.insert_context_field("message", message);
    notice.field_order = vec!["exception".into(), "message".into()];
    notice
}

#[allow(dead_code)]
fn runtime_exception_in_validator_error_notice(
    exception: &str,
    message: &str,
    validator: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "runtime_exception_in_validator_error",
        NoticeSeverity::Error,
        "runtime exception while validating gtfs",
    );
    notice.insert_context_field("exception", exception);
    notice.insert_context_field("message", message);
    notice.insert_context_field("validator", validator);
    notice.field_order = vec!["exception".into(), "message".into(), "validator".into()];
    notice
}

#[allow(dead_code)]
fn thread_execution_error_notice(exception: &str, message: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "thread_execution_error",
        NoticeSeverity::Error,
        "thread execution error",
    );
    notice.insert_context_field("exception", exception);
    notice.insert_context_field("message", message);
    notice.field_order = vec!["exception".into(), "message".into()];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
    }

    #[test]
    fn returns_notice_on_missing_required_file() {
        let dir = temp_dir("gtfs_missing_file");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            dir.join("agency.txt"),
            "agency_name,agency_url,agency_timezone\nTest,https://example.com,UTC\n",
        )
        .expect("write file");

        let input = GtfsInput::from_path(&dir).expect("input");
        let runner = ValidatorRunner::new();
        let outcome = validate_input(&input, &runner);

        // Feed loading now creates partial feeds with empty tables for missing files
        // The missing_required_file notice is still emitted
        assert!(outcome.feed.is_some());
        let missing_file_notices: Vec<_> = outcome
            .notices
            .iter()
            .filter(|n| n.code == "missing_required_file")
            .collect();
        assert!(
            !missing_file_notices.is_empty(),
            "Expected at least one missing_required_file notice"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
