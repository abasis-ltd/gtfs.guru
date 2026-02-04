#!/usr/bin/env bash
set -euo pipefail

REPO=${1:-"$(pwd)"}

mkdir -p "$REPO"
mkdir -p "$REPO/crates/gtfs_model/src"
mkdir -p "$REPO/crates/gtfs_validator_core/src"
mkdir -p "$REPO/crates/gtfs_validator_report/src"
mkdir -p "$REPO/crates/gtfs_validator_cli/src"
mkdir -p "$REPO/crates/gtfs_validator_web/src"
mkdir -p "$REPO/crates/gtfs_validator_gui/src"

cat <<'EOT' > "$REPO/Cargo.toml"
[workspace]
members = [
  "crates/gtfs_model",
  "crates/gtfs_validator_core",
  "crates/gtfs_validator_report",
  "crates/gtfs_validator_cli",
  "crates/gtfs_validator_web",
  "crates/gtfs_validator_gui",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "Apache-2.0"

[workspace.dependencies]
anyhow = "1.0"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
csv = "1.3"
zip = { version = "0.6", default-features = false, features = ["deflate"] }
clap = { version = "4.5", features = ["derive"] }
axum = "0.7"
tokio = { version = "1.37", features = ["rt-multi-thread", "macros", "fs", "signal"] }
tower = "0.4"
tracing = "0.1"
tracing-subscriber = "0.3"
EOT

cat <<'EOT' > "$REPO/.gitignore"
/target
**/*.rs.bk
.DS_Store
EOT

cat <<'EOT' > "$REPO/README.md"
# GTFS Validator (Rust)

This repository contains a full-stack Rust rewrite of the GTFS Validator. The goal is full parity with the current Java implementation, including validation rules, notice output, and report formats.

## Workspace layout
- `crates/gtfs_model`: core GTFS schema types.
- `crates/gtfs_validator_core`: parsing, validation engine, and rules.
- `crates/gtfs_validator_report`: JSON/HTML report generation.
- `crates/gtfs_validator_cli`: command-line interface.
- `crates/gtfs_validator_web`: web service (axum + tokio).
- `crates/gtfs_validator_gui`: desktop app (Tauri planned).

## Development
- Build all crates: `cargo build`
- Run tests: `cargo test`
- Run CLI: `cargo run -p gtfs_validator_cli -- --help`
EOT

cat <<'EOT' > "$REPO/crates/gtfs_model/Cargo.toml"
[package]
name = "gtfs_model"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
EOT

cat <<'EOT' > "$REPO/crates/gtfs_model/src/lib.rs"
use std::fmt;

use chrono::NaiveDate;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum GtfsParseError {
    #[error("invalid date format: {0}")]
    InvalidDateFormat(String),
    #[error("invalid date value: {0}")]
    InvalidDateValue(String),
    #[error("invalid time format: {0}")]
    InvalidTimeFormat(String),
    #[error("invalid time value: {0}")]
    InvalidTimeValue(String),
    #[error("invalid color format: {0}")]
    InvalidColorFormat(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GtfsDate {
    year: i32,
    month: u8,
    day: u8,
}

impl GtfsDate {
    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        if value.len() != 8 || !value.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(GtfsParseError::InvalidDateFormat(value.to_string()));
        }

        let year: i32 = value[0..4].parse().map_err(|_| {
            GtfsParseError::InvalidDateFormat(value.to_string())
        })?;
        let month: u8 = value[4..6].parse().map_err(|_| {
            GtfsParseError::InvalidDateFormat(value.to_string())
        })?;
        let day: u8 = value[6..8].parse().map_err(|_| {
            GtfsParseError::InvalidDateFormat(value.to_string())
        })?;

        if NaiveDate::from_ymd_opt(year, month as u32, day as u32).is_none() {
            return Err(GtfsParseError::InvalidDateValue(value.to_string()));
        }

        Ok(Self { year, month, day })
    }

    pub fn year(&self) -> i32 {
        self.year
    }

    pub fn month(&self) -> u8 {
        self.month
    }

    pub fn day(&self) -> u8 {
        self.day
    }
}

impl fmt::Display for GtfsDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}{:02}{:02}", self.year, self.month, self.day)
    }
}

impl Serialize for GtfsDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsDateVisitor;

        impl<'de> Visitor<'de> for GtfsDateVisitor {
            type Value = GtfsDate;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a GTFS date in YYYYMMDD format")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsDate, E> {
                GtfsDate::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsDateVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GtfsTime {
    total_seconds: i32,
}

impl GtfsTime {
    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 3 {
            return Err(GtfsParseError::InvalidTimeFormat(value.to_string()));
        }

        let hours: i32 = parts[0].parse().map_err(|_| {
            GtfsParseError::InvalidTimeFormat(value.to_string())
        })?;
        let minutes: i32 = parts[1].parse().map_err(|_| {
            GtfsParseError::InvalidTimeFormat(value.to_string())
        })?;
        let seconds: i32 = parts[2].parse().map_err(|_| {
            GtfsParseError::InvalidTimeFormat(value.to_string())
        })?;

        if hours < 0 || minutes < 0 || minutes > 59 || seconds < 0 || seconds > 59 {
            return Err(GtfsParseError::InvalidTimeValue(value.to_string()));
        }

        Ok(Self {
            total_seconds: hours * 3600 + minutes * 60 + seconds,
        })
    }

    pub fn total_seconds(&self) -> i32 {
        self.total_seconds
    }

    pub fn hours(&self) -> i32 {
        self.total_seconds / 3600
    }

    pub fn minutes(&self) -> i32 {
        (self.total_seconds % 3600) / 60
    }

    pub fn seconds(&self) -> i32 {
        self.total_seconds % 60
    }
}

impl fmt::Display for GtfsTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}",
            self.hours(),
            self.minutes(),
            self.seconds()
        )
    }
}

impl Serialize for GtfsTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsTimeVisitor;

        impl<'de> Visitor<'de> for GtfsTimeVisitor {
            type Value = GtfsTime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a GTFS time in HH:MM:SS format")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsTime, E> {
                GtfsTime::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsTimeVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GtfsColor {
    rgb: u32,
}

impl GtfsColor {
    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        if value.len() != 6 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(GtfsParseError::InvalidColorFormat(value.to_string()));
        }

        let rgb = u32::from_str_radix(value, 16)
            .map_err(|_| GtfsParseError::InvalidColorFormat(value.to_string()))?;
        Ok(Self { rgb })
    }

    pub fn rgb(&self) -> u32 {
        self.rgb
    }
}

impl fmt::Display for GtfsColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:06X}", self.rgb)
    }
}

impl Serialize for GtfsColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsColorVisitor;

        impl<'de> Visitor<'de> for GtfsColorVisitor {
            type Value = GtfsColor;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a 6-digit GTFS color hex string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsColor, E> {
                GtfsColor::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsColorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gtfs_date() {
        let date = GtfsDate::parse("20240131").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 31);
        assert_eq!(date.to_string(), "20240131");
    }

    #[test]
    fn rejects_invalid_date() {
        assert!(GtfsDate::parse("20240230").is_err());
        assert!(GtfsDate::parse("2024-01-01").is_err());
    }

    #[test]
    fn parses_gtfs_time() {
        let time = GtfsTime::parse("25:10:05").unwrap();
        assert_eq!(time.total_seconds(), 25 * 3600 + 10 * 60 + 5);
        assert_eq!(time.to_string(), "25:10:05");
    }

    #[test]
    fn rejects_invalid_time() {
        assert!(GtfsTime::parse("25:99:00").is_err());
        assert!(GtfsTime::parse("bad").is_err());
    }

    #[test]
    fn parses_gtfs_color() {
        let color = GtfsColor::parse("FF00AA").unwrap();
        assert_eq!(color.rgb(), 0xFF00AA);
        assert_eq!(color.to_string(), "FF00AA");
    }

    #[test]
    fn rejects_invalid_color() {
        assert!(GtfsColor::parse("GG00AA").is_err());
        assert!(GtfsColor::parse("12345").is_err());
    }
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_core/Cargo.toml"
[package]
name = "gtfs_validator_core"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
csv = { workspace = true }
zip = { workspace = true }
tracing = { workspace = true }

[dependencies.gtfs_model]
path = "../gtfs_model"
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_core/src/lib.rs"
pub mod input;
pub mod notice;

pub use input::{GtfsInput, GtfsInputError, GtfsInputSource};
pub use notice::{NoticeContainer, NoticeSeverity, ValidationNotice};
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_core/src/input.rs"
use std::path::{Path, PathBuf};

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
                .map(|ext| ext.eq_ignore_ascii_case("zip"))
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
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_core/src/notice.rs"
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationNotice {
    pub code: String,
    pub severity: NoticeSeverity,
    pub message: String,
    pub file: Option<String>,
    pub row: Option<u64>,
    pub field: Option<String>,
}

impl ValidationNotice {
    pub fn new(
        code: impl Into<String>,
        severity: NoticeSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            file: None,
            row: None,
            field: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct NoticeContainer {
    notices: Vec<ValidationNotice>,
}

impl NoticeContainer {
    pub fn new() -> Self {
        Self { notices: Vec::new() }
    }

    pub fn push(&mut self, notice: ValidationNotice) {
        self.notices.push(notice);
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidationNotice> {
        self.notices.iter()
    }

    pub fn len(&self) -> usize {
        self.notices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_report/Cargo.toml"
[package]
name = "gtfs_validator_report"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dependencies.gtfs_validator_core]
path = "../gtfs_validator_core"
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_report/src/lib.rs"
use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use gtfs_validator_core::{NoticeContainer, ValidationNotice};

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub notices: Vec<ValidationNotice>,
}

impl ValidationReport {
    pub fn from_container(container: &NoticeContainer) -> Self {
        Self {
            notices: container.iter().cloned().collect(),
        }
    }

    pub fn write_json<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self).context("serialize report")?;
        fs::write(&path, json).with_context(|| {
            format!("write report to {}", path.as_ref().display())
        })?;
        Ok(())
    }
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_cli/Cargo.toml"
[package]
name = "gtfs_validator_cli"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
anyhow = { workspace = true }
clap = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[dependencies.gtfs_validator_core]
path = "../gtfs_validator_core"

[dependencies.gtfs_validator_report]
path = "../gtfs_validator_report"
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_cli/src/main.rs"
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing::info;

use gtfs_validator_core::{GtfsInput, NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_validator_report::ValidationReport;

#[derive(Debug, Parser)]
#[command(name = "gtfs-validator")]
#[command(about = "GTFS validator (Rust rewrite)")]
struct Args {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,

    #[arg(short = 'o', long = "output")]
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let args = Args::parse();

    let input = GtfsInput::from_path(&args.input)
        .with_context(|| format!("load input {}", args.input.display()))?;
    info!("input {:?} detected", input.source());

    // TODO: replace with full validation pipeline.
    let mut notices = NoticeContainer::new();
    notices.push(ValidationNotice::new(
        "RUST_PLACEHOLDER",
        NoticeSeverity::Info,
        "Rust validator scaffold running",
    ));

    std::fs::create_dir_all(&args.output).with_context(|| {
        format!("create output dir {}", args.output.display())
    })?;

    let report = ValidationReport::from_container(&notices);
    report.write_json(args.output.join("report.json"))?;

    Ok(())
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_web/Cargo.toml"
[package]
name = "gtfs_validator_web"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
anyhow = { workspace = true }
axum = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }

[dependencies.gtfs_validator_core]
path = "../gtfs_validator_core"
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_web/src/main.rs"
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_gui/Cargo.toml"
[package]
name = "gtfs_validator_gui"
version = "0.9.2"
edition = "2021"
license = "Apache-2.0"

[dependencies]
EOT

cat <<'EOT' > "$REPO/crates/gtfs_validator_gui/src/main.rs"
fn main() {
    println!("GUI scaffold - TODO");
}
EOT
