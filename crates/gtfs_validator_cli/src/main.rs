use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use reqwest::blocking::Client;
use tracing::info;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use gtfs_guru_core::{
    apply_fixes, build_notice_schema_map, collect_input_notices, default_runner, diff_feeds,
    set_validation_country_code, set_validation_date, FeedDiff, FixPlan, FixSafety, GtfsFeed,
    GtfsInput, GtfsInputError, GtfsInputSource, NoticeContainer, NoticeSeverity, PlannedEdit,
    ValidationNotice, ValidatorRunner,
};
use gtfs_guru_report::{
    write_html_report, Badge, HtmlReportContext, MemoryUsageRecord, ReportSummary,
    ReportSummaryContext, SarifReport, ValidationReport,
};

/// Severity threshold at which a completed validation exits non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOn {
    /// Always exit 0 when validation completes.
    None,
    /// Exit 2 when at least one error is present.
    Error,
    /// Exit 2 when at least one error or warning is present.
    Warning,
}

#[derive(Debug, Parser)]
#[command(name = "gtfs-guru")]
#[command(about = "GTFS Guru validator (Rust rewrite)")]
#[command(version)]
#[command(subcommand_negates_reqs = true, args_conflicts_with_subcommands = true)]
#[command(group = clap::ArgGroup::new("fix_mode").args(["fix", "fix_unsafe"]).multiple(true))]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to a GTFS zip file or unpacked feed directory.
    #[arg(short = 'i', long = "input")]
    input: Option<PathBuf>,

    /// URL of a remote GTFS zip file to download and validate.
    #[arg(short = 'u', long = "url")]
    url: Option<String>,

    /// Directory in which to keep a feed downloaded with --url.
    #[arg(short = 's', long = "storage_directory", alias = "storage-directory")]
    storage_directory: Option<PathBuf>,

    /// Directory for generated reports; required unless --stdout is used.
    #[arg(
        short = 'o',
        long = "output_base",
        alias = "output",
        required_unless_present = "stdout",
        conflicts_with = "stdout"
    )]
    output: Option<PathBuf>,

    /// Output the JSON validation report to stdout instead of writing report files.
    #[arg(
        long = "stdout",
        conflicts_with_all = ["fix_dry_run", "fix", "fix_unsafe", "fix_output"]
    )]
    stdout: bool,

    /// ISO country code used by region-specific rules (for example US or CY).
    #[arg(short = 'c', long = "country_code", alias = "country-code")]
    country_code: Option<String>,

    /// Date against which to validate the feed, in YYYY-MM-DD format.
    #[arg(short = 'd', long = "date", alias = "date-for-validation")]
    date_for_validation: Option<String>,

    /// File name for the JSON validation report (default: report.json).
    #[arg(
        short = 'v',
        long = "validation_report_name",
        alias = "validation-report-name"
    )]
    validation_report_name: Option<String>,

    /// File name for the HTML report (default: report.html).
    #[arg(short = 'r', long = "html_report_name", alias = "html-report-name")]
    html_report_name: Option<String>,

    /// File name for the system errors report (default: system_errors.json).
    #[arg(
        short = 'e',
        long = "system_errors_report_name",
        alias = "system-errors-report-name"
    )]
    system_errors_report_name: Option<String>,

    /// Pretty-print JSON output.
    #[arg(short = 'p', long = "pretty")]
    pretty: bool,

    /// Write notice_schema.json describing every notice this build can emit.
    #[arg(
        short = 'n',
        long = "export_notices_schema",
        alias = "export-notices-schema"
    )]
    export_notices_schema: bool,

    /// Skip the online check for a newer validator release.
    #[arg(long = "skip_validator_update", alias = "skip-validator-update")]
    skip_validator_update: bool,

    /// Override the validated_at timestamp stored in report metadata.
    #[arg(long = "validated-at")]
    validated_at: Option<String>,

    /// Thread count stored in report metadata; use RAYON_NUM_THREADS to control parallelism.
    #[arg(long = "threads", default_value_t = 1)]
    threads: u32,

    /// Enable additional rules used by Google's GTFS ingestion.
    #[arg(long = "google_rules", alias = "google-rules")]
    google_rules: bool,

    /// Generate SARIF output for CI/CD integration (GitHub Actions, GitLab CI, etc.)
    #[arg(long = "sarif")]
    sarif: Option<String>,

    /// Exit 2 when the completed report reaches this severity.
    #[arg(long = "fail-on", value_enum, default_value_t = FailOn::None)]
    fail_on: FailOn,

    /// Write a shields.io endpoint descriptor for a README status badge.
    /// The path is taken as given, independent of --output_base.
    #[arg(long = "badge", value_name = "PATH")]
    badge: Option<PathBuf>,

    /// Write a self-contained SVG status badge (no shields.io request at render time).
    #[arg(long = "badge-svg", alias = "badge_svg", value_name = "PATH")]
    badge_svg: Option<PathBuf>,

    /// Left-hand text on the badge (default: GTFS).
    #[arg(long = "badge-label", alias = "badge_label", value_name = "TEXT")]
    badge_label: Option<String>,

    /// Show what fixes would be applied without modifying files
    #[arg(
        long = "fix-dry-run",
        alias = "fix-preview",
        conflicts_with_all = ["fix", "fix_unsafe", "fix_output"]
    )]
    fix_dry_run: bool,

    /// Write a repaired copy of the feed with the safe fixes applied.
    #[arg(long = "fix")]
    fix: bool,

    /// Like --fix, but also applies fixes that need confirmation or may change
    /// semantics.
    #[arg(long = "fix-unsafe")]
    pub fix_unsafe: bool,

    /// Where to write the repaired feed. Defaults to `<input>.fixed.<ext>` next
    /// to the input. Never overwrites the input or an existing path.
    #[arg(long = "fix-output", alias = "fix_output", requires = "fix_mode")]
    fix_output: Option<PathBuf>,

    /// Enable thorough validation (reports missing recommended fields and columns).
    /// By default, only mandatory GTFS rules are enforced to match Java validator behavior.
    #[arg(long = "thorough")]
    pub thorough: bool,

    /// Output detailed timing breakdown for performance analysis
    #[arg(long = "timing")]
    pub timing: bool,

    /// Output timing report as JSON instead of human-readable format
    #[arg(long = "timing-json")]
    pub timing_json: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compare two GTFS feeds and their validation results.
    Diff(DiffArgs),
}

#[derive(Debug, ClapArgs)]
struct DiffArgs {
    /// Previous GTFS zip file or unpacked feed directory.
    old: PathBuf,

    /// New GTFS zip file or unpacked feed directory.
    new: PathBuf,

    /// Write the machine-readable report to PATH, or use - for stdout.
    #[arg(long, value_name = "PATH")]
    json: Option<PathBuf>,

    /// Write a Markdown report to PATH, or use - for stdout.
    #[arg(long, value_name = "PATH")]
    markdown: Option<PathBuf>,

    /// Exit 2 if the new feed introduces additional validation errors.
    #[arg(long)]
    fail_on_new_errors: bool,

    /// Compare feed contents without running validation rules.
    #[arg(long)]
    no_validation: bool,

    /// ISO country code used by region-specific validation rules.
    #[arg(short = 'c', long = "country-code")]
    country_code: Option<String>,

    /// Date against which both feeds are validated, in YYYY-MM-DD format.
    #[arg(short = 'd', long = "date")]
    date_for_validation: Option<String>,

    /// Enable additional rules used by Google's GTFS ingestion.
    #[arg(long = "google-rules")]
    google_rules: bool,

    /// Enable thorough validation for both feeds.
    #[arg(long)]
    thorough: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if let Some(command) = args.command.as_ref() {
        tracing_subscriber::fmt().with_target(false).init();
        return match command {
            Command::Diff(diff_args) => run_diff(diff_args),
        };
    }
    if args.stdout {
        gtfs_guru_core::set_performance_logs_enabled(false);
    }
    if args.stdout {
        tracing_subscriber::fmt()
            .with_target(false)
            .with_max_level(tracing::Level::ERROR)
            .init();
    } else {
        tracing_subscriber::fmt().with_target(false).init();
    }

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
        Some(gtfs_guru_core::set_google_rules_enabled(true))
    } else {
        None
    };
    let _thorough_guard = if args.thorough {
        Some(gtfs_guru_core::set_thorough_mode_enabled(true))
    } else {
        None
    };

    let runner = default_runner();
    let started_at = Instant::now();
    let mut memory_usage_records = Vec::new();
    let mut last_used_bytes = None;
    let timing_collector = gtfs_guru_core::TimingCollector::new();

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
        if args.timing || args.timing_json {
            Some(&timing_collector)
        } else {
            None
        },
        args.stdout,
    );
    record_memory_usage(
        &mut memory_usage_records,
        &mut last_used_bytes,
        "ValidationRunner.run",
    );
    let elapsed = started_at.elapsed();
    let (validation_notices, system_errors) = if outcome.feed.is_none() {
        (NoticeContainer::new(), outcome.notices)
    } else {
        (outcome.notices, NoticeContainer::new())
    };

    let output = args.output.as_deref();
    if let Some(output) = output {
        std::fs::create_dir_all(output)
            .with_context(|| format!("create output dir {}", output.display()))?;
    }

    let mut summary_context = ReportSummaryContext::new()
        .with_gtfs_input(input.path())
        .with_validation_time_seconds(elapsed.as_secs_f64())
        .with_validator_version(env!("CARGO_PKG_VERSION"))
        .with_memory_usage_records(memory_usage_records)
        .with_threads(args.threads);
    if let Some(output) = output {
        summary_context = summary_context.with_output_directory(output);
    }
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
    let stdout_report_container = if outcome.feed.is_some() {
        &validation_notices
    } else {
        &system_errors
    };
    // Badges describe feed quality, so they follow the same container as
    // --fail-on: validation notices normally, system errors when the feed
    // could not be loaded at all.
    write_badges(&args, stdout_report_container)?;
    if args.stdout {
        let report =
            ValidationReport::from_container_with_summary(stdout_report_container, summary);
        let json = if args.pretty {
            serde_json::to_string_pretty(&report)
        } else {
            serde_json::to_string(&report)
        }
        .context("serialize report")?;
        println!("{json}");
        exit_if_threshold_reached(args.fail_on, stdout_report_container);
        return Ok(());
    }
    let output = output.expect("clap requires --output_base unless --stdout is used");
    let html_context = HtmlReportContext::from_summary(&summary, resolved.gtfs_source_label);
    write_html_report(
        output.join(&html_report_name),
        &validation_notices,
        &summary,
        html_context,
    )?;
    let report = ValidationReport::from_container_with_summary(&validation_notices, summary);
    report.write_json_with_format(output.join(&validation_report_name), args.pretty)?;
    ValidationReport::from_container(&system_errors)
        .write_json_with_format(output.join(&system_errors_report_name), args.pretty)?;

    // Generate SARIF report if requested
    if let Some(sarif_name) = &args.sarif {
        let sarif_path = output.join(sarif_name);
        let sarif_report = SarifReport::from_notices(&validation_notices);
        sarif_report.write(&sarif_path)?;
        info!("SARIF report written to {}", sarif_path.display());
    }

    // Output timing report if requested
    if args.timing || args.timing_json {
        let timing_summary = timing_collector.summary();
        if args.timing_json {
            let json = timing_summary.to_json();
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_default()
            );
        } else {
            eprintln!("{}", timing_summary.format_report());
        }
    }

    // Handle auto-fix options
    if args.fix_dry_run || args.fix || args.fix_unsafe {
        handle_fixes(&validation_notices, &args, &input)?;
    }

    // Reports are already written: status 2 describes feed quality, not a run failure.
    exit_if_threshold_reached(args.fail_on, stdout_report_container);

    Ok(())
}

fn run_diff(args: &DiffArgs) -> anyhow::Result<()> {
    if args.json.as_deref() == Some(Path::new("-"))
        && args.markdown.as_deref() == Some(Path::new("-"))
    {
        bail!("--json - and --markdown - cannot both write to stdout");
    }
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
    let _google_rules_guard = args
        .google_rules
        .then(|| gtfs_guru_core::set_google_rules_enabled(true));
    let _thorough_guard = args
        .thorough
        .then(|| gtfs_guru_core::set_thorough_mode_enabled(true));

    let (old_feed, old_notices) = load_feed_for_diff(&args.old, !args.no_validation)
        .with_context(|| format!("load old feed {}", args.old.display()))?;
    let (new_feed, new_notices) = load_feed_for_diff(&args.new, !args.no_validation)
        .with_context(|| format!("load new feed {}", args.new.display()))?;
    let report = diff_feeds(&old_feed, &new_feed, &old_notices, &new_notices);

    if args.json.is_none() && args.markdown.is_none() {
        print!("{}", render_diff_text(&report));
    }
    if let Some(path) = args.json.as_deref() {
        let json = serde_json::to_string_pretty(&report).context("serialize diff report")?;
        write_diff_output(path, &(json + "\n"))?;
    }
    if let Some(path) = args.markdown.as_deref() {
        write_diff_output(path, &render_diff_markdown(&report))?;
    }

    if args.fail_on_new_errors && report.new_error_count() > 0 {
        std::process::exit(2);
    }
    Ok(())
}

fn load_feed_for_diff(path: &Path, validate: bool) -> anyhow::Result<(GtfsFeed, NoticeContainer)> {
    let input = GtfsInput::from_path(path)
        .with_context(|| format!("open GTFS input {}", path.display()))?;
    if validate {
        let outcome = gtfs_guru_core::validate_input(&input, &default_runner());
        let feed = outcome
            .feed
            .with_context(|| format!("could not parse {}", path.display()))?;
        Ok((feed, outcome.notices))
    } else {
        let mut parse_notices = NoticeContainer::new();
        let feed = GtfsFeed::from_input_with_notices(&input, &mut parse_notices)
            .with_context(|| format!("parse {}", path.display()))?;
        // This mode is intentionally structural-only, including no comparison
        // of loader notices.
        Ok((feed, NoticeContainer::new()))
    }
}

fn write_diff_output(path: &Path, content: &str) -> anyhow::Result<()> {
    if path == Path::new("-") {
        print!("{content}");
        return Ok(());
    }
    std::fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

fn render_diff_text(report: &FeedDiff) -> String {
    use std::fmt::Write;

    if !report.has_changes() {
        return "No changes.\n".to_string();
    }
    let mut output = String::new();
    writeln!(
        output,
        "GTFS diff: {} new error(s), {} resolved error(s)",
        report.notices.new_errors, report.notices.resolved_errors
    )
    .unwrap();
    append_entity_text(&mut output, "Agencies", &report.agencies);
    append_entity_text(&mut output, "Routes", &report.routes);
    if report.stops.has_changes() {
        writeln!(
            output,
            "Stops: +{} -{} renamed {} moved {} changed {}",
            report.stops.added.len(),
            report.stops.removed.len(),
            report.stops.renamed.len(),
            report.stops.moved.len(),
            report.stops.changed.len()
        )
        .unwrap();
        append_ids(&mut output, "  added", &report.stops.added);
        append_ids(&mut output, "  removed", &report.stops.removed);
        append_ids(&mut output, "  renamed", &report.stops.renamed);
        if !report.stops.moved.is_empty() {
            let moved = report
                .stops
                .moved
                .iter()
                .map(|item| format!("{} ({:.1} m)", item.stop_id, item.distance_meters))
                .collect::<Vec<_>>();
            append_ids(&mut output, "  moved", &moved);
        }
    }
    if !report.trips_by_route.is_empty() {
        writeln!(output, "Trips by route:").unwrap();
        for change in &report.trips_by_route {
            writeln!(
                output,
                "  {}: {} -> {}",
                change.route_id, change.old_count, change.new_count
            )
            .unwrap();
        }
    }
    if !report.frequencies_by_route.is_empty() {
        writeln!(
            output,
            "Frequency windows changed: {} route(s)",
            report.frequencies_by_route.len()
        )
        .unwrap();
        for change in &report.frequencies_by_route {
            writeln!(output, "  {}", change.route_id).unwrap();
        }
    }
    if !report.files.added.is_empty() || !report.files.removed.is_empty() {
        writeln!(
            output,
            "Files: +{} -{}",
            report.files.added.len(),
            report.files.removed.len()
        )
        .unwrap();
        append_ids(&mut output, "  added", &report.files.added);
        append_ids(&mut output, "  removed", &report.files.removed);
    }
    if report.feed_info.changed {
        writeln!(
            output,
            "Feed version: {} -> {}",
            report.feed_info.old_version.as_deref().unwrap_or("n/a"),
            report.feed_info.new_version.as_deref().unwrap_or("n/a")
        )
        .unwrap();
    }
    if !report.notices.changes.is_empty() {
        writeln!(output, "Validation notices:").unwrap();
        for change in &report.notices.changes {
            writeln!(
                output,
                "  {:?} {}: {} -> {}",
                change.severity, change.code, change.old_count, change.new_count
            )
            .unwrap();
        }
    }
    output
}

fn append_entity_text(output: &mut String, label: &str, diff: &gtfs_guru_core::diff::EntityDiff) {
    use std::fmt::Write;
    if !diff.has_changes() {
        return;
    }
    writeln!(
        output,
        "{label}: +{} -{} changed {}",
        diff.added.len(),
        diff.removed.len(),
        diff.changed.len()
    )
    .unwrap();
    append_ids(output, "  added", &diff.added);
    append_ids(output, "  removed", &diff.removed);
    append_ids(output, "  changed", &diff.changed);
}

fn append_ids(output: &mut String, label: &str, ids: &[String]) {
    use std::fmt::Write;
    if !ids.is_empty() {
        writeln!(output, "{label}: {}", ids.join(", ")).unwrap();
    }
}

fn render_diff_markdown(report: &FeedDiff) -> String {
    use std::fmt::Write;

    let mut output = String::from("# GTFS feed diff\n\n");
    writeln!(
        output,
        "**New errors:** {} · **Resolved errors:** {}\n",
        report.notices.new_errors, report.notices.resolved_errors
    )
    .unwrap();
    output.push_str("| Area | Added | Removed | Changed |\n");
    output.push_str("|---|---:|---:|---:|\n");
    writeln!(
        output,
        "| Agencies | {} | {} | {} |",
        report.agencies.added.len(),
        report.agencies.removed.len(),
        report.agencies.changed.len()
    )
    .unwrap();
    writeln!(
        output,
        "| Routes | {} | {} | {} |",
        report.routes.added.len(),
        report.routes.removed.len(),
        report.routes.changed.len()
    )
    .unwrap();
    writeln!(
        output,
        "| Stops | {} | {} | {} renamed, {} moved, {} other |",
        report.stops.added.len(),
        report.stops.removed.len(),
        report.stops.renamed.len(),
        report.stops.moved.len(),
        report.stops.changed.len()
    )
    .unwrap();
    writeln!(
        output,
        "| Files | {} | {} | — |",
        report.files.added.len(),
        report.files.removed.len()
    )
    .unwrap();

    if !report.trips_by_route.is_empty() {
        output.push_str("\n## Trips by route\n\n| Route | Old | New |\n|---|---:|---:|\n");
        for change in &report.trips_by_route {
            writeln!(
                output,
                "| `{}` | {} | {} |",
                change.route_id, change.old_count, change.new_count
            )
            .unwrap();
        }
    }
    if !report.frequencies_by_route.is_empty() {
        output.push_str("\n## Frequency windows\n\n| Route | Old | New |\n|---|---|---|\n");
        for change in &report.frequencies_by_route {
            let old_windows = change.old_windows.join("<br>");
            let new_windows = change.new_windows.join("<br>");
            writeln!(
                output,
                "| `{}` | {} | {} |",
                change.route_id, old_windows, new_windows
            )
            .unwrap();
        }
    }
    if !report.notices.changes.is_empty() {
        output.push_str(
            "\n## Validation notices\n\n| Severity | Code | Old | New |\n|---|---|---:|---:|\n",
        );
        for change in &report.notices.changes {
            writeln!(
                output,
                "| {:?} | `{}` | {} | {} |",
                change.severity, change.code, change.old_count, change.new_count
            )
            .unwrap();
        }
    }
    output
}

fn perf_logging_enabled() -> bool {
    std::env::var_os("GTFS_PERF_DEBUG").is_some()
}

fn write_badges(args: &Args, notices: &NoticeContainer) -> anyhow::Result<()> {
    if args.badge.is_none() && args.badge_svg.is_none() {
        return Ok(());
    }

    let mut badge = Badge::from_notices(notices);
    if let Some(label) = args.badge_label.as_deref() {
        badge = badge.with_label(label);
    }

    if let Some(path) = args.badge.as_deref() {
        badge
            .write_endpoint_json(path)
            .with_context(|| format!("write badge endpoint to {}", path.display()))?;
        info!("Badge endpoint written to {}", path.display());
    }
    if let Some(path) = args.badge_svg.as_deref() {
        badge
            .write_svg(path)
            .with_context(|| format!("write badge SVG to {}", path.display()))?;
        info!("Badge SVG written to {}", path.display());
    }
    Ok(())
}

fn should_fail(fail_on: FailOn, errors: usize, warnings: usize) -> bool {
    match fail_on {
        FailOn::None => false,
        FailOn::Error => errors > 0,
        FailOn::Warning => errors > 0 || warnings > 0,
    }
}

fn exit_if_threshold_reached(fail_on: FailOn, notices: &NoticeContainer) {
    // Exact totals, including notices dropped by the per-group storage cap.
    let (errors, warnings, _infos) = notices.severity_counts();
    if !should_fail(fail_on, errors, warnings) {
        return;
    }

    let threshold = match fail_on {
        FailOn::Error => "error",
        FailOn::Warning => "warning",
        FailOn::None => unreachable!(),
    };
    eprintln!("Feed did not pass --fail-on {threshold}: {errors} error(s), {warnings} warning(s).");
    std::process::exit(2);
}

fn handle_fixes(notices: &NoticeContainer, args: &Args, input: &GtfsInput) -> anyhow::Result<()> {
    let max_safety = if args.fix_unsafe {
        FixSafety::Unsafe
    } else {
        FixSafety::Safe
    };
    let plan = FixPlan::from_notices(notices, max_safety);
    let counts = plan.counts();

    if counts.total() == 0 {
        info!("No auto-fixes available");
        if !args.thorough {
            // Most fix-carrying rules are gated behind thorough mode, so an
            // empty plan usually means they never ran.
            eprintln!(
                "No fixable issues found. Several rules that suggest fixes only run under --thorough."
            );
        }
        return Ok(());
    }

    info!(
        "Found {} fixable issues: {} safe, {} need confirmation, {} unsafe",
        counts.total(),
        counts.safe,
        counts.requires_confirmation,
        counts.unsafe_
    );

    if args.fix_dry_run {
        println!("\n=== Fix Dry Run ===\n");
        print_edits(plan.edits());
        println!(
            "{} edit(s) would be applied, {} skipped as above the requested safety level. \
             The input was not modified; re-run with --fix to write a repaired copy.",
            plan.edits().len(),
            plan.skipped().len()
        );
        return Ok(());
    }

    if plan.is_empty() {
        // Every fix sits above the requested safety level. Copying the feed
        // unchanged would only produce a confusing duplicate.
        println!(
            "Nothing to apply at this safety level: all {} fix(es) need --fix-unsafe. \
             No output was written.",
            plan.skipped().len()
        );
        return Ok(());
    }

    let output = match &args.fix_output {
        Some(path) => path.clone(),
        None => default_fix_output(input),
    };

    println!("\n=== Applying Fixes ===\n");
    print_edits(plan.edits());

    let outcome =
        apply_fixes(input, &plan, &output).context("failed to write the repaired feed")?;

    for conflict in &outcome.conflicts {
        eprintln!(
            "skipped {} at {} row {}, field '{}': {}",
            conflict.edit.notice_code,
            conflict.edit.file,
            conflict.edit.row,
            conflict.edit.field,
            conflict.reason
        );
    }

    println!(
        "Applied {} fix(es) across {} file(s) to {}.",
        outcome.applied.len(),
        outcome.rewritten_files.len(),
        outcome.output.display()
    );
    if !plan.skipped().is_empty() {
        println!(
            "{} fix(es) skipped as above the requested safety level; \
             re-run with --fix-unsafe to include them.",
            plan.skipped().len()
        );
    }
    if !outcome.conflicts.is_empty() {
        println!(
            "{} fix(es) did not match the file on disk and were left alone (see above).",
            outcome.conflicts.len()
        );
    }
    println!("Re-run validation on the output to confirm the result.");

    Ok(())
}

fn print_edits(edits: &[PlannedEdit]) {
    for edit in edits {
        println!(
            "[{}] {} row {}, field '{}': {}",
            edit.safety.label(),
            edit.file,
            edit.row,
            edit.field,
            edit.description
        );
        println!("  - {}", edit.original);
        println!("  + {}", edit.replacement);
        println!();
    }
}

/// `feed.zip` -> `feed.fixed.zip`, `feed/` -> `feed.fixed/`. Placing the copy
/// next to the input keeps it obvious without ever aliasing the original.
fn default_fix_output(input: &GtfsInput) -> PathBuf {
    let path = input.path();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("feed"));

    match input.source() {
        GtfsInputSource::Zip => {
            let extension = path
                .extension()
                .map(|ext| ext.to_string_lossy().into_owned())
                .unwrap_or_else(|| String::from("zip"));
            parent.join(format!("{stem}.fixed.{extension}"))
        }
        GtfsInputSource::Directory => parent.join(format!("{stem}.fixed")),
    }
}

fn export_notice_schema(args: &Args) -> anyhow::Result<()> {
    let output = args
        .output
        .as_deref()
        .context("--export_notices_schema requires --output_base")?;
    std::fs::create_dir_all(output)
        .with_context(|| format!("create output dir {}", output.display()))?;
    let schema = build_notice_schema_map();
    let json = if args.pretty {
        serde_json::to_string_pretty(&schema)
    } else {
        serde_json::to_string(&schema)
    }
    .context("serialize notice schema")?;
    let path = output.join("notice_schema.json");
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

use gtfs_guru_core::progress::ProgressHandler;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;

struct IndicatifHandler {
    _multi: MultiProgress,
    loading_pb: ProgressBar,
    validation_pb: ProgressBar,
}

impl IndicatifHandler {
    fn new(hidden: bool) -> Self {
        let multi = MultiProgress::new();

        let loading_pb = if hidden {
            ProgressBar::hidden()
        } else {
            multi.add(ProgressBar::new(0))
        };
        loading_pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {percent}% {msg}",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        loading_pb.set_message("Waiting to load files...");

        let validation_pb = if hidden {
            ProgressBar::hidden()
        } else {
            multi.add(ProgressBar::new(0))
        };
        validation_pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] {bar:40.magenta/magenta} {percent}% {msg}",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        validation_pb.set_message("Waiting to validate...");

        Self {
            _multi: multi,
            loading_pb,
            validation_pb,
        }
    }
}

impl ProgressHandler for IndicatifHandler {
    fn on_start_file_load(&self, file: &str) {
        self.loading_pb.set_message(format!("Loading {}", file));
    }

    fn on_finish_file_load(&self, _file: &str) {
        self.loading_pb.inc(1);
    }

    fn on_start_validation(&self, validator_name: &str) {
        self.validation_pb
            .set_message(format!("Running {}", validator_name));
    }

    fn on_finish_validation(&self, _validator_name: &str) {
        // Increment handled in increment_validator_progress
    }

    fn set_total_files(&self, count: usize) {
        self.loading_pb.set_length(count as u64);
        self.loading_pb.set_message("Starting load...");
    }

    fn set_total_validators(&self, count: usize) {
        self.validation_pb.set_length(count as u64);
        self.validation_pb.set_message("Starting validation...");
    }

    fn increment_validator_progress(&self) {
        self.validation_pb.inc(1);
    }
}

fn validate_with_metrics(
    input: &GtfsInput,
    runner: &ValidatorRunner,
    memory_usage_records: &mut Vec<MemoryUsageRecord>,
    last_used_bytes: &mut Option<u64>,
    timing: Option<&gtfs_guru_core::TimingCollector>,
    quiet: bool,
) -> gtfs_guru_core::ValidationOutcome {
    let mut notices = NoticeContainer::new();

    if let Ok(input_notices) = collect_input_notices(input) {
        for notice in input_notices {
            notices.push(notice);
        }
    }

    let progress_handler = Arc::new(IndicatifHandler::new(quiet));

    let load_start = std::time::Instant::now();
    let handler_clone = progress_handler.clone();
    // The error is boxed here rather than in GtfsInputError itself: the variants
    // carry paths and source errors that make the type large, and every caller
    // would pay for that on the success path too.
    let load_result = catch_unwind(AssertUnwindSafe(|| {
        GtfsFeed::from_input_with_notices_and_progress(
            input,
            &mut notices,
            Some(handler_clone.as_ref()),
        )
        .map_err(Box::new)
    }));

    progress_handler
        .loading_pb
        .finish_with_message("Loading complete");
    let load_elapsed = load_start.elapsed();
    if !quiet && perf_logging_enabled() {
        eprintln!("[PERF] Feed loading took: {:?}", load_elapsed);
    }

    // Record loading time in timing collector
    if let Some(t) = timing {
        t.record(
            "feed_loading",
            load_elapsed,
            gtfs_guru_core::TimingCategory::Loading,
        );
    }

    match load_result {
        Ok(Ok(feed)) => {
            record_memory_usage(
                memory_usage_records,
                last_used_bytes,
                "GtfsFeedLoader.executeMultiFileValidators",
            );
            let validate_start = std::time::Instant::now();
            let handler_clone = progress_handler.clone();
            runner.run_with_progress_and_timing(
                &feed,
                &mut notices,
                Some(handler_clone.as_ref()),
                timing,
            );

            progress_handler
                .validation_pb
                .finish_with_message("Validation complete");
            if !quiet && perf_logging_enabled() {
                eprintln!("[PERF] Validation took: {:?}", validate_start.elapsed());
            }
            record_memory_usage(
                memory_usage_records,
                last_used_bytes,
                "org.mobilitydata.gtfsvalidator.table.GtfsFeedLoader.loadAndValidate",
            );
            gtfs_guru_core::ValidationOutcome {
                feed: Some(feed),
                notices,
            }
        }
        Ok(Err(err)) => {
            push_input_error_notice(&mut notices, *err);
            gtfs_guru_core::ValidationOutcome {
                feed: None,
                notices,
            }
        }
        Err(panic) => {
            notices.push(runtime_exception_in_loader_error_notice(
                input.path().display().to_string(),
                panic_payload_message(&*panic),
            ));
            gtfs_guru_core::ValidationOutcome {
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
        // Linux reports ru_maxrss in kilobytes, macOS in bytes.
        #[cfg(not(target_os = "macos"))]
        {
            Some(max_rss.saturating_mul(1024))
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use clap::CommandFactory;

    #[test]
    fn fail_on_none_never_fails() {
        assert!(!should_fail(FailOn::None, 0, 0));
        assert!(!should_fail(FailOn::None, 42, 7));
    }

    #[test]
    fn fail_on_error_ignores_warnings() {
        assert!(!should_fail(FailOn::Error, 0, 9));
        assert!(should_fail(FailOn::Error, 1, 0));
    }

    #[test]
    fn fail_on_warning_covers_errors_and_warnings() {
        assert!(!should_fail(FailOn::Warning, 0, 0));
        assert!(should_fail(FailOn::Warning, 0, 1));
        assert!(should_fail(FailOn::Warning, 1, 0));
    }

    #[test]
    fn cli_definition_is_valid_and_has_version() {
        let command = Args::command();
        command.clone().debug_assert();
        assert_eq!(command.get_version(), Some(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn diff_subcommand_does_not_require_legacy_output_flags() {
        let args = Args::try_parse_from([
            "gtfs-guru",
            "diff",
            "old.zip",
            "new.zip",
            "--json",
            "-",
            "--fail-on-new-errors",
        ])
        .unwrap();

        let Some(Command::Diff(diff)) = args.command else {
            panic!("expected diff subcommand");
        };
        assert_eq!(diff.old, PathBuf::from("old.zip"));
        assert_eq!(diff.new, PathBuf::from("new.zip"));
        assert_eq!(diff.json, Some(PathBuf::from("-")));
        assert!(diff.fail_on_new_errors);
    }

    #[test]
    fn badge_flags_are_written_to_the_paths_given() {
        let dir = std::env::temp_dir().join(format!("gtfs-guru-badge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("badge.json");
        let svg_path = dir.join("nested").join("badge.svg");

        let args = Args::parse_from([
            "gtfs-guru",
            "--stdout",
            "--badge",
            json_path.to_str().unwrap(),
            "--badge-svg",
            svg_path.to_str().unwrap(),
            "--badge-label",
            "MBTA feed",
        ]);

        let mut notices = NoticeContainer::new();
        notices.push(ValidationNotice::new(
            "duplicate_key",
            NoticeSeverity::Error,
            "duplicate",
        ));
        write_badges(&args, &notices).unwrap();

        let json = std::fs::read_to_string(&json_path).unwrap();
        assert!(json.contains("\"label\": \"MBTA feed\""), "{json}");
        assert!(json.contains("\"message\": \"1 error\""), "{json}");
        // A nested destination is created rather than failing the run.
        assert!(std::fs::read_to_string(&svg_path).unwrap().contains("<svg"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn badges_are_skipped_when_neither_flag_is_given() {
        let args = Args::parse_from(["gtfs-guru", "--stdout"]);
        write_badges(&args, &NoticeContainer::new()).unwrap();
    }

    #[test]
    fn stdout_rejects_every_fix_mode() {
        for fix_args in [
            vec!["--fix-dry-run"],
            vec!["--fix"],
            vec!["--fix-unsafe"],
            vec!["--fix", "--fix-output", "fixed.zip"],
        ] {
            let mut argv = vec!["gtfs-guru", "--stdout"];
            argv.extend(fix_args);
            let error = Args::try_parse_from(argv).expect_err("fix mode must conflict with stdout");
            assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        }
    }
}
