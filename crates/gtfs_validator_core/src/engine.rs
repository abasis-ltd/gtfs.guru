use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{
    input::collect_input_notices, GtfsFeed, GtfsInput, GtfsInputError, NoticeContainer,
    NoticeSeverity, ValidationNotice, ValidatorRunner,
};

pub struct ValidationOutcome {
    pub feed: Option<GtfsFeed>,
    pub notices: NoticeContainer,
}

pub fn validate_input(input: &GtfsInput, runner: &ValidatorRunner) -> ValidationOutcome {
    let mut notices = NoticeContainer::new();

    if let Ok(input_notices) = collect_input_notices(input) {
        for notice in input_notices {
            notices.push(notice);
        }
    }

    let load_result = catch_unwind(AssertUnwindSafe(|| {
        GtfsFeed::from_input_with_notices(input, &mut notices)
    }));

    match load_result {
        Ok(Ok(feed)) => {
            runner.run_with(&feed, &mut notices);
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
    notice.field_order = vec!["filename".to_string(), "message".to_string()];
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
    notice.field_order = vec!["exception".to_string(), "message".to_string()];
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
    notice.field_order = vec![
        "exception".to_string(),
        "filename".to_string(),
        "message".to_string(),
    ];
    notice
}

fn panic_payload_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic".to_string()
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
    notice.field_order = vec!["exception".to_string(), "message".to_string()];
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
    notice.field_order = vec![
        "exception".to_string(),
        "message".to_string(),
        "validator".to_string(),
    ];
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
    notice.field_order = vec!["exception".to_string(), "message".to_string()];
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

        assert!(outcome.feed.is_none());
        assert_eq!(outcome.notices.len(), 1);
        assert_eq!(
            outcome.notices.iter().next().unwrap().code,
            "missing_required_file"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
