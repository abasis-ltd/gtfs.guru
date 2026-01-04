use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use clap::Parser;
use reqwest::blocking::Client;
use tracing::info;

use gtfs_validator_core::{
    build_notice_schema_map, collect_input_notices, default_runner, set_validation_country_code,
    set_validation_date, GtfsFeed, GtfsInput, GtfsInputError, NoticeContainer, NoticeSeverity,
    ValidationNotice, ValidatorRunner,
};
use gtfs_validator_report::{
    write_html_report, HtmlReportContext, MemoryUsageRecord, ReportSummary, ReportSummaryContext,
    SarifReport, ValidationReport,
};

#[derive(Debug, Parser)]
#[command(name = "gtfs-validator")]
#[command(about = "GTFS validator (Rust rewrite)")]
struct Args {
    #[arg(short = 'i', long = "input")]
    input: Option<PathBuf>,

    #[arg(short = 'u', long = "url")]
    url: Option<String>,

    #[arg(short = 's', long = "storage_directory", alias = "storage-directory")]
    storage_directory: Option<PathBuf>,

    #[arg(short = 'o', long = "output_base", alias = "output")]
    output: PathBuf,

    #[arg(short = 'c', long = "country_code", alias = "country-code")]
    country_code: Option<String>,

    #[arg(short = 'd', long = "date", alias = "date-for-validation")]
    date_for_validation: Option<String>,

    #[arg(
        short = 'v',
        long = "validation_report_name",
        alias = "validation-report-name"
    )]
    validation_report_name: Option<String>,

    #[arg(short = 'r', long = "html_report_name", alias = "html-report-name")]
    html_report_name: Option<String>,

    #[arg(
        short = 'e',
        long = "system_errors_report_name",
        alias = "system-errors-report-name"
    )]
    system_errors_report_name: Option<String>,

    #[arg(short = 'p', long = "pretty")]
    pretty: bool,

    #[arg(
        short = 'n',
        long = "export_notices_schema",
        alias = "export-notices-schema"
    )]
    export_notices_schema: bool,

    #[arg(long = "skip_validator_update", alias = "skip-validator-update")]
    skip_validator_update: bool,

    #[arg(long = "validated-at")]
    validated_at: Option<String>,

    #[arg(long = "threads", default_value_t = 1)]
    threads: u32,

    #[arg(long = "google_rules", alias = "google-rules")]
    google_rules: bool,

    /// Generate SARIF output for CI/CD integration (GitHub Actions, GitLab CI, etc.)
    #[arg(long = "sarif")]
    sarif: Option<String>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let args = Args::parse();

    if args.export_notices_schema {
        export_notice_schema(&args)?;
        if args.input.is_none() && args.url.is_none() {
            return Ok(());
        }
    }

    let resolved = resolve_input(&args)?;
    let input = resolved.input;
    info!("input {:?} detected", input.source());

    let _validation_date_guard = match args.date_for_validation.as_deref() {
        Some(value) => Some(set_validation_date(Some(parse_validation_date(value)?))),
        None => None,
    };
    let _validation_country_guard = match args.country_code.as_deref() {
        Some(value) if !value.trim().is_empty() && !value.trim().eq_ignore_ascii_case("ZZ") => {
            Some(set_validation_country_code(Some(value.trim().to_string())))
        }
        _ => None,
    };
    let _google_rules_guard = if args.google_rules {
        Some(gtfs_validator_core::set_google_rules_enabled(true))
    } else {
        None
    };

    let runner = default_runner();
    let started_at = Instant::now();
    let mut memory_usage_records = Vec::new();
    let mut last_used_bytes = None;
    record_memory_usage(
        &mut memory_usage_records,
        &mut last_used_bytes,
        "GtfsFeedLoader.loadTables",
    );
    let outcome = validate_with_metrics(
        &input,
        &runner,
        &mut memory_usage_records,
        &mut last_used_bytes,
    );
    record_memory_usage(
        &mut memory_usage_records,
        &mut last_used_bytes,
        "ValidationRunner.run",
    );
    let elapsed = started_at.elapsed();
    let notices = outcome.notices;

    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("create output dir {}", args.output.display()))?;

    let mut summary_context = ReportSummaryContext::new()
        .with_gtfs_input(input.path())
        .with_output_directory(&args.output)
        .with_validation_time_seconds(elapsed.as_secs_f64())
        .with_validator_version(env!("CARGO_PKG_VERSION"))
        .with_memory_usage_records(memory_usage_records)
        .with_threads(args.threads);
    if let Some(gtfs_input_uri) = resolved.gtfs_input_uri.as_deref() {
        summary_context = summary_context.with_gtfs_input_uri(gtfs_input_uri);
    }
    if let Some(country_code) = args.country_code.as_deref() {
        summary_context = summary_context.with_country_code(country_code);
    }
    if let Some(date_for_validation) = args.date_for_validation.as_deref() {
        summary_context = summary_context.with_date_for_validation(date_for_validation);
    }
    if let Some(validation_report_name) = args.validation_report_name.as_deref() {
        summary_context = summary_context.with_validation_report_name(validation_report_name);
    }
    if let Some(html_report_name) = args.html_report_name.as_deref() {
        summary_context = summary_context.with_html_report_name(html_report_name);
    }
    if let Some(system_errors_report_name) = args.system_errors_report_name.as_deref() {
        summary_context = summary_context.with_system_errors_report_name(system_errors_report_name);
    }
    if let Some(validated_at) = args.validated_at.as_deref() {
        summary_context = summary_context.with_validated_at(validated_at);
    }
    if let Some(feed) = outcome.feed.as_ref() {
        summary_context = summary_context.with_feed(feed);
    }
    let summary = ReportSummary::from_context(summary_context);
    let validation_report_name = summary
        .validation_report_name
        .clone()
        .unwrap_or_else(|| "report.json".to_string());
    let html_report_name = summary
        .html_report_name
        .clone()
        .unwrap_or_else(|| "report.html".to_string());
    let system_errors_report_name = summary
        .system_errors_report_name
        .clone()
        .unwrap_or_else(|| "system_errors.json".to_string());
    let html_context = HtmlReportContext::from_summary(&summary, resolved.gtfs_source_label);
    write_html_report(
        args.output.join(&html_report_name),
        &notices,
        &summary,
        html_context,
    )?;
    let report = ValidationReport::from_container_with_summary(&notices, summary);
    report.write_json_with_format(args.output.join(&validation_report_name), args.pretty)?;
    ValidationReport::empty()
        .write_json_with_format(args.output.join(&system_errors_report_name), args.pretty)?;

    // Generate SARIF report if requested
    if let Some(sarif_name) = &args.sarif {
        let sarif_path = args.output.join(sarif_name);
        let sarif_report = SarifReport::from_notices(&notices);
        sarif_report.write(&sarif_path)?;
        info!("SARIF report written to {}", sarif_path.display());
    }

    Ok(())
}

fn export_notice_schema(args: &Args) -> anyhow::Result<()> {
    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("create output dir {}", args.output.display()))?;
    let schema = build_notice_schema_map();
    let json = if args.pretty {
        serde_json::to_string_pretty(&schema)
    } else {
        serde_json::to_string(&schema)
    }
    .context("serialize notice schema")?;
    let path = args.output.join("notice_schema.json");
    std::fs::write(&path, format!("{}\n", json))
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

struct ResolvedInput {
    input: GtfsInput,
    gtfs_input_uri: Option<String>,
    gtfs_source_label: String,
}

fn resolve_input(args: &Args) -> anyhow::Result<ResolvedInput> {
    match (&args.input, &args.url) {
        (Some(_), Some(_)) => {
            bail!("--input and --url cannot be provided at the same time");
        }
        (None, None) => {
            bail!("one of --input or --url must be provided");
        }
        (Some(path), None) => {
            if args.storage_directory.is_some() {
                bail!("--storage_directory requires --url");
            }
            let input = GtfsInput::from_path(path)
                .with_context(|| format!("load input {}", path.display()))?;
            Ok(ResolvedInput {
                input,
                gtfs_input_uri: None,
                gtfs_source_label: path.display().to_string(),
            })
        }
        (None, Some(url)) => {
            if url.trim().is_empty() {
                bail!("--url must not be empty");
            }
            if let Some(storage_directory) = args.storage_directory.as_ref() {
                std::fs::create_dir_all(storage_directory).with_context(|| {
                    format!("create storage directory {}", storage_directory.display())
                })?;
            }
            let (download_dir, file_name) = match args.storage_directory.clone() {
                Some(dir) => (dir, download_file_name(url)),
                None => (
                    std::env::temp_dir(),
                    format!(
                        "gtfs_download_{}_{}.zip",
                        std::process::id(),
                        unique_suffix()
                    ),
                ),
            };
            let download_path = download_dir.join(file_name);
            download_url_to_path(url, &download_path)?;
            let input = GtfsInput::from_path(&download_path)
                .with_context(|| format!("load input {}", download_path.display()))?;
            Ok(ResolvedInput {
                input,
                gtfs_input_uri: Some(url.clone()),
                gtfs_source_label: url.clone(),
            })
        }
    }
}

fn download_file_name(url: &str) -> String {
    let trimmed = url.split('?').next().unwrap_or(url);
    let candidate = trimmed
        .rsplit('/')
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("gtfs.zip");
    let lower = candidate.to_ascii_lowercase();
    if lower.ends_with(".zip") || lower.ends_with(".gtfs") {
        candidate.to_string()
    } else {
        format!("{}.zip", candidate)
    }
}

fn download_url_to_path(url: &str, path: &Path) -> anyhow::Result<()> {
    let client = Client::builder()
        .user_agent(format!("gtfs-validator-rust/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build http client")?;
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("download gtfs from {}", url))?
        .error_for_status()
        .with_context(|| format!("download gtfs from {}", url))?;
    let mut file =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    std::io::copy(&mut response, &mut file).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn unique_suffix() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn parse_validation_date(value: &str) -> anyhow::Result<NaiveDate> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("--date-for-validation cannot be empty");
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(trimmed, "%Y%m%d"))
        .with_context(|| format!("invalid --date-for-validation {}", value))
}

fn validate_with_metrics(
    input: &GtfsInput,
    runner: &ValidatorRunner,
    memory_usage_records: &mut Vec<MemoryUsageRecord>,
    last_used_bytes: &mut Option<u64>,
) -> gtfs_validator_core::ValidationOutcome {
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
            record_memory_usage(
                memory_usage_records,
                last_used_bytes,
                "GtfsFeedLoader.executeMultiFileValidators",
            );
            runner.run_with(&feed, &mut notices);
            record_memory_usage(
                memory_usage_records,
                last_used_bytes,
                "org.mobilitydata.gtfsvalidator.table.GtfsFeedLoader.loadAndValidate",
            );
            gtfs_validator_core::ValidationOutcome {
                feed: Some(feed),
                notices,
            }
        }
        Ok(Err(err)) => {
            push_input_error_notice(&mut notices, err);
            gtfs_validator_core::ValidationOutcome {
                feed: None,
                notices,
            }
        }
        Err(panic) => {
            notices.push(runtime_exception_in_loader_error_notice(
                input.path().display().to_string(),
                panic_payload_message(&*panic),
            ));
            gtfs_validator_core::ValidationOutcome {
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
            let mut notice =
                ValidationNotice::new("malformed_json", NoticeSeverity::Error, source.to_string());
            notice.file = Some(file);
            notice.insert_context_field("message", source.to_string());
            notice.field_order = vec!["filename".to_string(), "message".to_string()];
            notices.push(notice);
        }
        other => {
            let mut notice =
                ValidationNotice::new("i_o_error", NoticeSeverity::Error, other.to_string());
            notice.insert_context_field("exception", "GtfsInputError");
            notice.insert_context_field("message", other.to_string());
            notice.field_order = vec!["exception".to_string(), "message".to_string()];
            notices.push(notice);
        }
    }
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

fn record_memory_usage(
    records: &mut Vec<MemoryUsageRecord>,
    last_used_bytes: &mut Option<u64>,
    key: &str,
) {
    let used_bytes = current_rss_bytes().unwrap_or(0);
    let diff = last_used_bytes.map(|prev| used_bytes as i64 - prev as i64);
    *last_used_bytes = Some(used_bytes);

    records.push(MemoryUsageRecord {
        key: key.to_string(),
        total_memory: used_bytes,
        free_memory: used_bytes,
        max_memory: used_bytes,
        diff_memory: diff,
    });
}

fn current_rss_bytes() -> Option<u64> {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        use std::os::raw::{c_int, c_long};

        #[repr(C)]
        struct TimeVal {
            tv_sec: c_long,
            tv_usec: c_long,
        }

        #[repr(C)]
        struct RUsage {
            ru_utime: TimeVal,
            ru_stime: TimeVal,
            ru_maxrss: c_long,
            ru_ixrss: c_long,
            ru_idrss: c_long,
            ru_isrss: c_long,
            ru_minflt: c_long,
            ru_majflt: c_long,
            ru_nswap: c_long,
            ru_inblock: c_long,
            ru_oublock: c_long,
            ru_msgsnd: c_long,
            ru_msgrcv: c_long,
            ru_nsignals: c_long,
            ru_nvcsw: c_long,
            ru_nivcsw: c_long,
        }

        extern "C" {
            fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
        }

        const RUSAGE_SELF: c_int = 0;

        let mut usage = MaybeUninit::<RUsage>::uninit();
        let result = unsafe { getrusage(RUSAGE_SELF, usage.as_mut_ptr()) };
        if result != 0 {
            return None;
        }
        let usage = unsafe { usage.assume_init() };
        let max_rss = usage.ru_maxrss as u64;

        #[cfg(target_os = "macos")]
        {
            Some(max_rss)
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Some(max_rss.saturating_mul(1024));
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}
