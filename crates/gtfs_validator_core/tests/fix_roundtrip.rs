//! End-to-end coverage for auto-fix: break a valid feed in every way the
//! suggesters claim to repair, run the real validation pipeline, apply the
//! plan, and confirm the repaired feed validates clean.

use gtfs_guru_core::{apply_fixes, FixPlan, FixSafety, GtfsInput, NoticeSeverity};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
}

fn copy_feed(destination: &Path) {
    let source = project_root().join("test-gtfs-feeds").join("base-valid");
    fs::create_dir_all(destination).expect("create feed dir");
    for entry in fs::read_dir(&source).expect("read base-valid") {
        let entry = entry.expect("entry");
        if entry.path().is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name())).expect("copy");
        }
    }
}

fn break_field(dir: &Path, file: &str, from: &str, to: &str) {
    let path = dir.join(file);
    let text = fs::read_to_string(&path).expect("read");
    assert!(text.contains(from), "{file} does not contain {from:?}");
    fs::write(&path, text.replacen(from, to, 1)).expect("write");
}

fn error_codes(input: &GtfsInput) -> Vec<String> {
    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(input, &runner);
    let mut codes: Vec<String> = outcome
        .notices
        .iter()
        .filter(|notice| notice.severity == NoticeSeverity::Error)
        .map(|notice| notice.code.clone())
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

fn notice_codes(input: &GtfsInput) -> Vec<String> {
    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(input, &runner);
    let mut codes: Vec<String> = outcome
        .notices
        .iter()
        .map(|notice| notice.code.clone())
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

#[test]
fn repairs_every_suggested_defect_and_revalidates_clean() {
    let feed = temp_path("gtfs_fix_roundtrip");
    copy_feed(&feed);

    break_field(
        &feed,
        "agency.txt",
        "https://example.com",
        "www.example.com",
    );
    break_field(&feed, "routes.txt", ",3,FF0000,", ",3,#FF0000,");
    break_field(&feed, "routes.txt", ",3,00FF00,", ",3,0F0,");
    break_field(&feed, "calendar.txt", ",20250101,", ",2025-01-01,");
    break_field(
        &feed,
        "stop_times.txt",
        "trip1,08:10:00,08:10:00",
        "trip1,8:10,8:10",
    );
    break_field(
        &feed,
        "stops.txt",
        "Second Stop,40.7138",
        "Second Stop,\"40,7138\"",
    );

    let input = GtfsInput::from_path(&feed).expect("input");
    let before = error_codes(&input);
    assert_eq!(
        before,
        vec![
            "invalid_color",
            "invalid_date",
            "invalid_float",
            "invalid_time",
            "invalid_url",
        ],
        "the broken feed should trip exactly the rules under test"
    );

    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);

    // The decimal comma is confirm-level, so --fix alone leaves it in place.
    let safe_plan = FixPlan::from_notices(&outcome.notices, FixSafety::Safe);
    assert_eq!(safe_plan.edits().len(), 6);
    assert_eq!(safe_plan.skipped().len(), 1);

    let plan = FixPlan::from_notices(&outcome.notices, FixSafety::Unsafe);
    assert_eq!(plan.edits().len(), 7);
    assert!(plan.skipped().is_empty());

    let fixed = temp_path("gtfs_fix_roundtrip_out");
    let applied = apply_fixes(&input, &plan, &fixed).expect("apply fixes");
    assert_eq!(applied.applied.len(), 7);
    assert!(
        applied.conflicts.is_empty(),
        "unexpected conflicts: {:?}",
        applied.conflicts
    );

    let repaired = GtfsInput::from_path(&fixed).expect("repaired input");
    assert!(
        error_codes(&repaired).is_empty(),
        "repaired feed still has errors: {:?}",
        error_codes(&repaired)
    );

    // Untouched files are copied byte for byte.
    for name in ["trips.txt", "shapes.txt", "feed_info.txt"] {
        assert_eq!(
            fs::read(feed.join(name)).unwrap(),
            fs::read(fixed.join(name)).unwrap(),
            "{name} should be copied verbatim"
        );
    }
    // And the input is left exactly as it was.
    assert!(fs::read_to_string(feed.join("routes.txt"))
        .unwrap()
        .contains("#FF0000"));

    fs::remove_dir_all(&feed).ok();
    fs::remove_dir_all(&fixed).ok();
}

#[test]
fn trims_declared_fields_without_touching_unknown_columns() {
    let _guard = gtfs_guru_core::set_thorough_mode_enabled(true);
    let feed = temp_path("gtfs_fix_whitespace");
    copy_feed(&feed);
    break_field(
        &feed,
        "agency.txt",
        "Test Transit Agency",
        "\" Test Transit Agency \"",
    );

    let input = GtfsInput::from_path(&feed).expect("input");
    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);
    assert!(outcome
        .notices
        .iter()
        .any(|notice| notice.code == "leading_or_trailing_whitespaces"));

    let plan = FixPlan::from_notices(&outcome.notices, FixSafety::Safe);
    assert!(plan
        .edits()
        .iter()
        .any(|edit| edit.notice_code == "leading_or_trailing_whitespaces"));
    let fixed = temp_path("gtfs_fix_whitespace_out");
    apply_fixes(&input, &plan, &fixed).expect("apply");

    let repaired = fs::read_to_string(fixed.join("agency.txt")).expect("read fixed agency");
    assert!(repaired.contains(",Test Transit Agency,"));
    assert!(!repaired.contains("\" Test Transit Agency \""));

    fs::remove_dir_all(&feed).ok();
    fs::remove_dir_all(&fixed).ok();
}

#[test]
fn sorts_stop_times_and_removes_the_unsorted_notice() {
    let feed = temp_path("gtfs_fix_sort");
    copy_feed(&feed);
    let path = feed.join("stop_times.txt");
    let text = fs::read_to_string(&path).expect("read stop_times");
    let mut lines: Vec<&str> = text.lines().collect();
    lines.swap(1, 3);
    fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write unsorted stop_times");

    let input = GtfsInput::from_path(&feed).expect("input");
    assert!(notice_codes(&input).contains(&"unsorted_stop_times".to_string()));
    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);
    let plan = FixPlan::from_notices(&outcome.notices, FixSafety::Safe);
    assert!(plan
        .edits()
        .iter()
        .any(|edit| edit.notice_code == "unsorted_stop_times"));

    let fixed = temp_path("gtfs_fix_sort_out");
    apply_fixes(&input, &plan, &fixed).expect("sort");
    let repaired = GtfsInput::from_path(&fixed).expect("repaired input");
    assert!(!notice_codes(&repaired).contains(&"unsorted_stop_times".to_string()));

    fs::remove_dir_all(&feed).ok();
    fs::remove_dir_all(&fixed).ok();
}

#[test]
fn unsafe_mode_deletes_a_foreign_key_orphan_and_revalidates() {
    let feed = temp_path("gtfs_fix_orphan");
    copy_feed(&feed);
    break_field(
        &feed,
        "stop_times.txt",
        "trip1,08:00:00,08:00:00,stop1,1",
        "trip1,08:00:00,08:00:00,missing-stop,1",
    );

    let input = GtfsInput::from_path(&feed).expect("input");
    assert!(error_codes(&input).contains(&"foreign_key_violation".to_string()));
    let runner = gtfs_guru_core::rules::default_runner();
    let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);
    let foreign_key_notice = outcome
        .notices
        .iter()
        .find(|notice| notice.code == "foreign_key_violation")
        .expect("foreign key notice");

    let safe_plan = FixPlan::from_notices(&outcome.notices, FixSafety::Safe);
    assert!(
        safe_plan
            .skipped()
            .iter()
            .any(|edit| edit.notice_code == "foreign_key_violation"),
        "safe plan did not retain the unsafe orphan repair: {safe_plan:?}; notice: \
         {foreign_key_notice:?}"
    );
    let plan = FixPlan::from_notices(&outcome.notices, FixSafety::Unsafe);
    let fixed = temp_path("gtfs_fix_orphan_out");
    apply_fixes(&input, &plan, &fixed).expect("delete orphan");

    let fixed_stop_times =
        fs::read_to_string(fixed.join("stop_times.txt")).expect("read fixed stop_times");
    assert!(!fixed_stop_times.contains("missing-stop"));
    let repaired = GtfsInput::from_path(&fixed).expect("repaired input");
    assert!(!error_codes(&repaired).contains(&"foreign_key_violation".to_string()));

    fs::remove_dir_all(&feed).ok();
    fs::remove_dir_all(&fixed).ok();
}
