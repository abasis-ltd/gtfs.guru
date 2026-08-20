use gtfs_guru_core::{
    input::GtfsInput, rules::PathwayReachableLocationValidator, GtfsFeed, NoticeContainer,
    NoticeSeverity, StringPool, Validator,
};
use gtfs_guru_model::{Pathway, Stop};
use std::fs;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // root
        .unwrap()
        .to_path_buf()
}

fn test_feeds_root() -> PathBuf {
    project_root().join("test-gtfs-feeds")
}

const REAL_WORLD_DIR_ENV: &str = "GTFS_GURU_REAL_WORLD_DIR";
const REAL_WORLD_REQUIRED_ENV: &str = "GTFS_GURU_REQUIRE_REAL_WORLD";

fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => false,
    }
}

/// Locates a real-world parity feed, or `None` when it has not been fetched.
///
/// These feeds are third-party downloads, not fixtures: they weigh ~143 MB and
/// their publishers refresh them continuously, so the repository keeps only the
/// links in `test-gtfs-feeds/real-world/manifest.json`. Run
/// `scripts/fetch_real_world_feeds.py` to turn those links back into files.
///
/// A clean checkout skips rather than fails. Set `GTFS_GURU_REQUIRE_REAL_WORLD=1`
/// to make a missing feed an error, which is what a parity run wants.
fn real_world_feed(file_name: &str) -> Option<PathBuf> {
    let root = std::env::var_os(REAL_WORLD_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| test_feeds_root().join("real-world"));
    let path = root.join(file_name);
    if path.is_file() {
        return Some(path);
    }

    assert!(
        !env_flag(REAL_WORLD_REQUIRED_ENV),
        "{REAL_WORLD_REQUIRED_ENV} is set but {path:?} is missing. \
         Run scripts/fetch_real_world_feeds.py to fetch it."
    );
    eprintln!(
        "skipping real-world parity check: {path:?} is not fetched. \
         Run scripts/fetch_real_world_feeds.py, or point {REAL_WORLD_DIR_ENV} \
         at a directory that already holds the feeds."
    );
    None
}

/// Counts the data records in a CSV payload the way the loader reads it:
/// header consumed, ragged rows tolerated, BOM skipped.
fn csv_record_count(data: &[u8]) -> usize {
    let body = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(body)
        .records()
        .count()
}

#[test]
fn test_base_valid() {
    let feed_path = test_feeds_root().join("base-valid");
    assert!(
        feed_path.exists(),
        "Base valid feed not found at {:?}",
        feed_path
    );

    let input = GtfsInput::from_path(&feed_path).expect("Failed to create input");
    let runner = gtfs_guru_core::rules::default_runner();

    // Set validation date to a date within the valid range of the feed if necessary,
    // or rely on today if the feed is dynamic.
    // The base-valid README or content might specify dates.
    // For now, let's assume it's designed to pass or we might need to mock date.

    let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);

    // Filter out INFO/WARNING notices. Base valid might have warnings.
    let unexpected_notices: Vec<_> = outcome
        .notices
        .iter()
        .filter(|n| n.severity == NoticeSeverity::Error)
        .collect();

    assert!(
        unexpected_notices.is_empty(),
        "Expected no errors in base-valid, found: {:#?}",
        unexpected_notices
    );
}

#[test]
fn test_mbta_pathways_are_fully_loaded_and_reachable() {
    let Some(feed_path) = real_world_feed("boston_mbta.zip") else {
        return;
    };

    let input = GtfsInput::from_path(&feed_path).expect("Failed to create MBTA input");
    let reader = input.reader();
    let pool = StringPool::new();
    let mut load_notices = NoticeContainer::new();
    let raw_pathways = reader
        .read_file("pathways.txt")
        .expect("Failed to read raw MBTA pathways.txt");
    let stops = reader
        .read_csv_with_notices::<Stop>("stops.txt", &mut load_notices, &pool)
        .expect("Failed to read MBTA stops.txt");
    let pathways = reader
        .read_csv_with_notices::<Pathway>("pathways.txt", &mut load_notices, &pool)
        .expect("Failed to read MBTA pathways.txt");

    // MBTA republishes its feed continuously, so the row count is not a constant
    // to pin down. The invariant that survives a refresh is that no row is
    // silently dropped: MBTA spells descending stairs as a negative
    // `stair_count`, which a narrower integer type would reject row by row.
    // Row equality is what catches that -- `csv_parsing_failed` is suppressed
    // outside thorough mode, so asserting its absence here would prove nothing.
    assert_eq!(
        pathways.rows.len(),
        csv_record_count(&raw_pathways),
        "every MBTA pathway row, including negative stair_count values, must deserialize"
    );

    let feed = GtfsFeed {
        stops,
        pathways: Some(pathways),
        pool,
        ..Default::default()
    };
    let mut notices = NoticeContainer::new();
    PathwayReachableLocationValidator.validate(&feed, &mut notices);
    let unreachable: Vec<_> = notices
        .iter()
        .filter(|notice| notice.code == "pathway_unreachable_location")
        .collect();

    assert!(
        unreachable.is_empty(),
        "MBTA has no canonical pathway reachability errors: {unreachable:#?}"
    );
}

#[test]
fn test_errors() {
    let errors_root = test_feeds_root().join("errors");
    assert!(errors_root.exists(), "Errors directory not found");

    visit_dirs(&errors_root, &mut |path| {
        // Only process directories that are "leaf" nodes (contain .txt files)
        // OR simply directories that match an error code name.
        // The structure is errors/category/error_code/*.txt

        if path.is_file() || contains_txt_files(path) {
            let error_code = if path.is_file() {
                path.file_stem().unwrap().to_str().unwrap()
            } else {
                path.file_name().unwrap().to_str().unwrap()
            };
            let expected_notice_code = match error_code {
                // Renamed by gtfs-validator v8.0.0; keep the existing tracked fixture path.
                "fare_transfer_rule_missing_transfer_count" => {
                    "fare_transfer_rule_without_transfer_count"
                }
                _ => error_code,
            };
            println!("Testing error expectation: {} in {:?}", error_code, path);

            let _date_guard = gtfs_guru_core::set_validation_date(Some(
                chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            ));
            let _thorough_guard = gtfs_guru_core::set_thorough_mode_enabled(true);
            let is_google = path.to_string_lossy().contains("google");
            let _google_guard = gtfs_guru_core::set_google_rules_enabled(is_google);

            let input = GtfsInput::from_path(path).expect("Failed to create input");
            let runner = gtfs_guru_core::rules::default_runner();
            let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);

            let found = outcome
                .notices
                .iter()
                .any(|n| n.code == expected_notice_code);

            if !found {
                println!("Notices found: {:#?}", outcome.notices);
                panic!(
                    "Expected notice code '{}' not found in {:?}",
                    expected_notice_code, path
                );
            }
        }
    })
    .unwrap();
}

#[test]
fn test_warnings() {
    let warnings_root = test_feeds_root().join("warnings");
    assert!(warnings_root.exists(), "Warnings directory not found");

    visit_dirs(&warnings_root, &mut |path| {
        if path.is_file() || contains_txt_files(path) {
            let warning_code = if path.is_file() {
                path.file_stem().unwrap().to_str().unwrap()
            } else {
                path.file_name().unwrap().to_str().unwrap()
            };
            if warning_code == "leading_or_trailing_whitespaces" {
                return;
            }
            println!(
                "Testing warning expectation: {} in {:?}",
                warning_code, path
            );

            let _date_guard = gtfs_guru_core::set_validation_date(Some(
                chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            ));
            let _thorough_guard = gtfs_guru_core::set_thorough_mode_enabled(true);

            let is_google = path.to_string_lossy().contains("google");
            let _google_guard = gtfs_guru_core::set_google_rules_enabled(is_google);

            let input = GtfsInput::from_path(path).expect("Failed to create input");
            let runner = gtfs_guru_core::rules::default_runner();
            let outcome = gtfs_guru_core::engine::validate_input(&input, &runner);

            let found = outcome.notices.iter().any(|n| n.code == warning_code);

            if !found {
                println!("Notices found: {:#?}", outcome.notices);
                panic!(
                    "Expected warning code '{}' not found in {:?}",
                    warning_code, path
                );
            }
        }
    })
    .unwrap();
}

fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&Path)) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if is_zip_file(&path) {
                    let stem = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    let sibling_dir = path.with_file_name(stem);
                    if !(sibling_dir.is_dir() && contains_txt_files(&sibling_dir)) {
                        cb(&path);
                    }
                }
                continue;
            }
            if path.is_dir() {
                // If this directory is a test case (contains GTFS txt files), run callback
                if contains_txt_files(&path) {
                    cb(&path);
                } else {
                    // Recurse
                    visit_dirs(&path, cb)?;
                }
            }
        }
    }
    Ok(())
}

fn contains_txt_files(path: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if let Some(ext) = entry_path.extension() {
                if ext == "txt" {
                    let name = entry_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    if !name.eq_ignore_ascii_case("README.txt") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_zip_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}
