//! Status badge output: a shields.io *endpoint* descriptor and a standalone SVG.
//!
//! The endpoint JSON is meant to be published somewhere reachable (a gh-pages
//! branch, a release asset, an S3 bucket) and referenced from a README as
//! `https://img.shields.io/endpoint?url=<url-of-this-file>`. The SVG is the
//! offline equivalent: no third-party request at render time, at the cost of
//! shields' own styling options.

use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use gtfs_guru_core::NoticeContainer;

/// Schema version understood by the shields.io endpoint API.
const ENDPOINT_SCHEMA_VERSION: u8 = 1;

/// Default left-hand text. Short on purpose: badges live in cramped README rows.
pub const DEFAULT_BADGE_LABEL: &str = "GTFS";

/// A rendered badge: what shields.io calls label / message / color.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    /// Schema version; part of the shields endpoint contract.
    pub schema_version: u8,
    /// Left-hand text.
    pub label: String,
    /// Right-hand text.
    pub message: String,
    /// Right-hand background, as a shields color name.
    pub color: String,
}

impl Badge {
    /// Build a badge from the exact severity totals of a completed run.
    ///
    /// The totals come from [`NoticeContainer::severity_counts`], which stays
    /// exact even when the per-group storage cap drops individual notices.
    pub fn from_notices(notices: &NoticeContainer) -> Self {
        let (errors, warnings, _infos) = notices.severity_counts();
        Self::from_counts(errors, warnings)
    }

    /// Build a badge from error and warning counts.
    pub fn from_counts(errors: usize, warnings: usize) -> Self {
        let (message, color) = if errors > 0 {
            (
                format!("{} {}", errors, plural(errors, "error", "errors")),
                "red",
            )
        } else if warnings > 0 {
            (
                format!(
                    "0 errors, {} {}",
                    warnings,
                    plural(warnings, "warning", "warnings")
                ),
                "yellow",
            )
        } else {
            ("valid".to_string(), "brightgreen")
        };

        Self {
            schema_version: ENDPOINT_SCHEMA_VERSION,
            label: DEFAULT_BADGE_LABEL.to_string(),
            message,
            color: color.to_string(),
        }
    }

    /// Override the left-hand text (for example a feed or agency name).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Serialize the shields.io endpoint descriptor.
    pub fn to_endpoint_json(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(self).context("serialize badge endpoint JSON")
    }

    /// Write the shields.io endpoint descriptor.
    pub fn write_endpoint_json<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        write_atomically(path.as_ref(), &format!("{}\n", self.to_endpoint_json()?))
    }

    /// Write a self-contained SVG badge.
    pub fn write_svg<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        write_atomically(path.as_ref(), &self.to_svg())
    }

    /// Render a self-contained SVG in the widely recognized "flat" style.
    ///
    /// Widths are estimated rather than measured — the renderer has no font
    /// metrics — so every text run also carries `textLength`, which makes the
    /// glyphs scale into the box we reserved instead of overflowing it.
    pub fn to_svg(&self) -> String {
        let label = xml_escape(&self.label);
        let message = xml_escape(&self.message);
        let label_width = text_width(&self.label);
        let message_width = text_width(&self.message);
        let total_width = label_width + message_width;
        let color = svg_color(&self.color);

        // Coordinates are in tenths of a pixel (the group is scaled by 0.1) so
        // that half-pixel text centering stays on integer values.
        let label_center = label_width * 5;
        let message_center = label_width * 10 + message_width * 5;
        let label_text_width = (label_width - 10) * 10;
        let message_text_width = (message_width - 10) * 10;
        let accessible_text = format!("{}: {}", label, message);

        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{total_width}" height="20" role="img" aria-label="{accessible_text}">
  <title>{accessible_text}</title>
  <clipPath id="r"><rect width="{total_width}" height="20" rx="3" fill="#fff"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="{label_width}" height="20" fill="#555"/>
    <rect x="{label_width}" width="{message_width}" height="20" fill="{color}"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="110" transform="scale(.1)">
    <text x="{label_center}" y="140" textLength="{label_text_width}">{label}</text>
    <text x="{message_center}" y="140" textLength="{message_text_width}">{message}</text>
  </g>
</svg>
"##
        )
    }
}

/// Approximate the pixel width of a badge segment, padding included.
///
/// Verdana at 11px averages a little under 7px per glyph; digits and spaces are
/// narrower than letters. The estimate only has to be close — `textLength`
/// absorbs the remaining error.
fn text_width(text: &str) -> usize {
    let glyphs: f64 = text
        .chars()
        .map(|c| match c {
            ' ' => 3.5,
            ',' | '.' | ':' | ';' | 'i' | 'j' | 'l' | 'I' | '!' | '|' => 3.5,
            '0'..='9' => 7.0,
            'm' | 'M' | 'w' | 'W' => 10.0,
            c if c.is_uppercase() => 8.0,
            _ => 6.5,
        })
        .sum();
    // 10px of horizontal padding, and never narrower than a single glyph slot.
    (glyphs.ceil() as usize).max(6) + 10
}

fn svg_color(name: &str) -> &'static str {
    match name {
        "brightgreen" => "#4c1",
        "green" => "#97ca00",
        "yellowgreen" => "#a4a61d",
        "yellow" => "#dfb317",
        "orange" => "#fe7d37",
        "red" => "#e05d44",
        "blue" => "#007ec6",
        _ => "#9f9f9f",
    }
}

fn plural(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Write via a sibling temp file so a reader never sees a half-written badge —
/// badges are often served straight out of a checked-out working tree.
fn write_atomically(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create badge directory {}", parent.display()))?;
        }
    }
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, contents)
        .with_context(|| format!("write badge to {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "replace badge at {} with {}",
            path.display(),
            temp_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtfs_guru_core::{NoticeSeverity, ValidationNotice};

    fn container(codes: &[(&str, NoticeSeverity)]) -> NoticeContainer {
        let mut container = NoticeContainer::new();
        for (code, severity) in codes {
            container.push(ValidationNotice::new(*code, *severity, "message"));
        }
        container
    }

    #[test]
    fn clean_feed_is_green() {
        let badge = Badge::from_notices(&NoticeContainer::new());
        assert_eq!(badge.message, "valid");
        assert_eq!(badge.color, "brightgreen");
        assert_eq!(badge.label, "GTFS");
        assert_eq!(badge.schema_version, 1);
    }

    #[test]
    fn warnings_only_are_yellow() {
        let badge = Badge::from_notices(&container(&[
            ("a", NoticeSeverity::Warning),
            ("b", NoticeSeverity::Warning),
            ("c", NoticeSeverity::Info),
        ]));
        assert_eq!(badge.message, "0 errors, 2 warnings");
        assert_eq!(badge.color, "yellow");
    }

    #[test]
    fn errors_win_over_warnings() {
        let badge = Badge::from_notices(&container(&[
            ("a", NoticeSeverity::Error),
            ("b", NoticeSeverity::Warning),
        ]));
        assert_eq!(badge.message, "1 error");
        assert_eq!(badge.color, "red");
    }

    #[test]
    fn endpoint_json_uses_the_shields_field_names() {
        let json = Badge::from_counts(0, 0).to_endpoint_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["label"], "GTFS");
        assert_eq!(value["message"], "valid");
        assert_eq!(value["color"], "brightgreen");
    }

    #[test]
    fn svg_is_well_formed_and_escaped() {
        let svg = Badge::from_counts(3, 0).with_label("Feed <A&B>").to_svg();
        assert!(svg.starts_with("<svg xmlns="));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("Feed &lt;A&amp;B&gt;"));
        assert!(!svg.contains("Feed <A&B>"));
        assert!(svg.contains("#e05d44"));
        assert!(svg.contains("3 errors"));
    }

    #[test]
    fn svg_segments_add_up_to_the_declared_width() {
        let badge = Badge::from_counts(0, 5);
        let svg = badge.to_svg();
        let total: usize = svg
            .split("width=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(total, text_width(&badge.label) + text_width(&badge.message));
    }
}
