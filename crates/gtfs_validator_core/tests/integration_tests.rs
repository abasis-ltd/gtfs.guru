use gtfs_guru_core::{input::GtfsInput, NoticeSeverity};
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
fn test_errors() {
    let errors_root = test_feeds_root().join("errors");
    assert!(errors_root.exists(), "Errors directory not found");

    visit_dirs(&errors_root, &mut |path| {
        // Only process directories that are "leaf" nodes (contain .txt files)
        // OR simply directories that match an error code name.
        // The structure is errors/category/error_code/*.txt

        if contains_txt_files(path) {
            let error_code = path.file_name().unwrap().to_str().unwrap();
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

            let found = outcome.notices.iter().any(|n| n.code == error_code);

            if !found {
                println!("Notices found: {:#?}", outcome.notices);
                panic!(
                    "Expected notice code '{}' not found in {:?}",
                    error_code, path
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
        if contains_txt_files(path) {
            let warning_code = path.file_name().unwrap().to_str().unwrap();
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
            if let Some(ext) = entry.path().extension() {
                if ext == "txt" {
                    return true;
                }
            }
        }
    }
    false
}
