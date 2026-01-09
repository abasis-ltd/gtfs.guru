use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::Context;
use chrono::{Local, NaiveDate, SecondsFormat};
use serde_json::{Number, Value};

use gtfs_guru_core::{NoticeContainer, NoticeSeverity, ValidationNotice};

use crate::{ReportCounts, ReportFeedInfo, ReportSummary};

const DEFAULT_COUNTRY_CODE: &str = "ZZ";
const NOTICE_ROW_LIMIT: usize = 50;
const GTFS_FEATURE_BASE_URL: &str = "https://gtfs.org/getting_started/features/";

pub struct HtmlReportContext {
    pub gtfs_source: String,
    pub country_code: String,
    pub date_for_validation: String,
    pub validated_at: String,
    pub validator_version: Option<String>,
    pub new_version_available: bool,
}

impl HtmlReportContext {
    pub fn from_summary(summary: &ReportSummary, gtfs_source: impl Into<String>) -> Self {
        let now = Local::now();
        let validated_at = summary
            .validated_at
            .clone()
            .unwrap_or_else(|| now.to_rfc3339_opts(SecondsFormat::Secs, true));
        let date_for_validation = summary
            .date_for_validation
            .clone()
            .unwrap_or_else(|| now.date_naive().format("%Y-%m-%d").to_string());
        let country_code = summary
            .country_code
            .clone()
            .unwrap_or_else(|| DEFAULT_COUNTRY_CODE.to_string());

        Self {
            gtfs_source: gtfs_source.into(),
            country_code,
            date_for_validation,
            validated_at,
            validator_version: summary.validator_version.clone(),
            new_version_available: false,
        }
    }

    pub fn with_new_version_available(mut self, available: bool) -> Self {
        self.new_version_available = available;
        self
    }
}

pub fn write_html_report<P: AsRef<Path>>(
    path: P,
    notices: &NoticeContainer,
    summary: &ReportSummary,
    context: HtmlReportContext,
) -> anyhow::Result<()> {
    let html = render_html(notices, summary, &context);
    fs::write(&path, html)
        .with_context(|| format!("write html report to {}", path.as_ref().display()))?;
    Ok(())
}

pub fn generate_html_report_string(
    notices: &NoticeContainer,
    summary: &ReportSummary,
    context: HtmlReportContext,
) -> String {
    render_html(notices, summary, &context)
}

fn render_html(
    notices: &NoticeContainer,
    summary: &ReportSummary,
    context: &HtmlReportContext,
) -> String {
    let mut out = String::new();
    out.push_str(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>GTFS Schedule Validation Report</title>
    <meta name="robots" content="noindex, nofollow">
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8; width=device-width, initial-scale=1"/>
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" integrity="sha256-p4NxAoJBhIIN+hmNHrzRCf9tD/miZyoHS5obTRR9BMY=" crossorigin=""/>
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js" integrity="sha256-20nQCchB9co0qIjJZRGuk2/Z9VM+kNiyxNV1lvTlZBo=" crossorigin=""></script>
    <script>
      document.addEventListener('DOMContentLoaded', function() {
        // Accordion functionality (vanilla JS)
        document.querySelectorAll('.accordion tr.notice').forEach(function(row) {
            row.addEventListener('click', function() {
                var descRow = this.nextElementSibling;
                if (descRow && descRow.classList.contains('description')) {
                    this.classList.toggle('open');
                    descRow.classList.toggle('open');

                    // Toggle +/- icon
                    var icon = this.querySelector('span');
                    if (icon) {
                        icon.textContent = this.classList.contains('open') ? '–' : '+';
                    }
                }
            });
        });
      });
    </script>
    <style>
    :root {
        --primary: #4f46e5;
        --primary-hover: #4338ca;
        --bg: #f8fafc;
        --card-bg: #ffffff;
        --text-main: #1e293b;
        --text-muted: #64748b;
        --border: #e2e8f0;
        --error: #ef4444;
        --warning: #f59e0b;
        --info: #06b6d4;
        --success: #10b981;
    }

    body {
        font-family: 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
        font-size: 14px;
        line-height: 1.5;
        color: var(--text-main);
        background-color: var(--bg);
        margin: 0;
        padding: 0;
    }

    * {
        box-sizing: border-box;
    }

    a {
        color: var(--primary);
        text-decoration: none;
    }

    a:hover {
        text-decoration: underline;
    }

    .container {
        max-width: 1200px;
        margin: 0 auto;
        padding: 1rem;
    }

    header {
        margin-bottom: 1.5rem;
        background: white;
        padding: 1rem 1.5rem;
        border-radius: 8px;
        box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        border-left: 4px solid var(--primary);
    }

    header h1 {
        margin: 0 0 0.25rem 0;
        font-size: 1.5rem;
        font-weight: 800;
        color: var(--text-main);
    }

    header p {
        margin: 0.25rem 0;
        color: var(--text-muted);
    }

    .badge {
        display: inline-block;
        padding: 0.25rem 0.75rem;
        border-radius: 9999px;
        font-weight: 600;
        font-size: 0.75rem;
        text-transform: uppercase;
        letter-spacing: 0.025em;
    }

    .error-badge { background: #fee2e2; color: #b91c1c; }
    .warning-badge { background: #fef3c7; color: #92400e; }
    .info-badge { background: #e0f2fe; color: #0369a1; }

    .summary-grid {
        display: grid;
        grid-template-columns: 1fr 1.6fr 0.9fr 0.7fr 1fr;
        gap: 0.75rem;
        margin-bottom: 1.5rem;
    }

    @media (max-width: 1100px) {
        .summary-grid {
            grid-template-columns: repeat(3, 1fr);
        }
    }

    @media (max-width: 640px) {
        .summary-grid {
            grid-template-columns: 1fr;
        }
    }


    .card {
        background: var(--card-bg);
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 0.75rem 1rem;
        box-shadow: 0 1px 2px rgba(0,0,0,0.05);
    }

    .card h4 {
        margin: 0 0 0.5rem 0;
        font-size: 0.95rem;
        font-weight: 700;
        border-bottom: 1px solid var(--bg);
        padding-bottom: 0.25rem;
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .card dl {
        margin: 0;
        display: grid;
        grid-template-columns: auto 1fr;
        gap: 0.1rem 0.5rem;
        font-size: 0.85rem;
    }

    .card dt {
        color: var(--text-muted);
        font-weight: 500;
        white-space: nowrap;
    }

    .card dd {
        margin: 0;
        font-weight: 600;
        word-break: break-all;
    }

    .card ul, .card ol {
        margin: 0;
        padding-left: 0;
        list-style: none;
        font-size: 0.85rem;
    }

    .card li {
        margin-bottom: 0.1rem;
    }

    .section-title {
        font-size: 1.25rem;
        font-weight: 700;
        margin: 1.5rem 0 0.75rem 0;
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }
    .tooltip {
        position: relative;
        display: inline-block;
        border-bottom: 1px dotted var(--text-muted);
        cursor: help;
        color: var(--primary);
        font-size: 0.75rem;
        margin-left: 0.25rem;
    }

    .tooltip .tooltiptext {
        visibility: hidden;
        width: 240px;
        background-color: #1e293b;
        color: #fff;
        text-align: center;
        border-radius: 6px;
        padding: 5px;
        position: absolute;
        z-index: 10;
        bottom: 125%;
        left: 50%;
        margin-left: -120px;
        opacity: 0;
        transition: opacity 0.3s;
        font-size: 0.75rem;
        line-height: 1.2;
        font-weight: normal;
        pointer-events: none;
        box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1);
    }

    .tooltip:hover .tooltiptext {
        visibility: visible;
        opacity: 1;
    }

    .compliance-stats {
        display: flex;
        gap: 0.75rem;
        flex-wrap: wrap;
        margin-bottom: 1rem;
    }

    .stat-pill {
        background: white;
        padding: 0.5rem 1rem;
        border-radius: 8px;
        border: 1px solid var(--border);
        display: flex;
        align-items: center;
        gap: 0.5rem;
        box-shadow: 0 1px 2px rgba(0,0,0,0.05);
    }

    .stat-pill .count {
        font-size: 1.1rem;
        font-weight: 800;
    }

    table {
        width: 100%;
        border-collapse: separate;
        border-spacing: 0;
        background: white;
        border-radius: 12px;
        overflow: hidden;
        border: 1px solid var(--border);
        box-shadow: 0 1px 2px rgba(0,0,0,0.05);
    }

    th {
        background: #f1f5f9;
        text-align: left;
        padding: 0.6rem 0.75rem;
        font-weight: 700;
        color: var(--text-muted);
        border-bottom: 1px solid var(--border);
    }

    td {
        padding: 0.6rem 0.75rem;
        border-bottom: 1px solid var(--border);
    }

    tr:last-child td {
        border-bottom: none;
    }

    .accordion tr.notice {
        cursor: pointer;
        transition: background 0.2s;
    }

    .accordion tr.notice:hover {
        background: #f8fafc;
    }

    .accordion tr.notice.open {
        background: #f1f5f9;
    }

    .accordion tr.description {
        display: none;
    }

    .accordion tr.description.open {
        display: table-row;
    }

    .notice-code {
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
        font-weight: 600;
        color: var(--primary);
    }

    .desc-content {
        padding: 1.5rem;
        background: #f8fafc;
        border-radius: 8px;
        margin: 0.5rem;
        border-left: 4px solid var(--primary);
    }

    .desc-content h3 {
        margin: 0 0 0.75rem 0;
    }

    .spec-feature {
        display: inline-block;
        padding: 0.2rem 0.5rem;
        background: #e2e8f0;
        border-radius: 4px;
        font-size: 0.85rem;
        margin: 0.2rem;
    }

    .view-map-btn {
        background: var(--primary);
        color: white;
        border: none;
        padding: 0.5rem 1rem;
        border-radius: 6px;
        font-weight: 600;
        cursor: pointer;
        transition: background 0.2s;
    }

    .view-map-btn:hover {
        background: var(--primary-hover);
    }

    footer {
        margin-top: 4rem;
        padding-top: 2rem;
        border-top: 1px solid var(--border);
        text-align: center;
        color: var(--text-muted);
        padding-bottom: 4rem;
    }

    .footer-links {
        margin-top: 1rem;
        display: flex;
        justify-content: center;
        gap: 1.5rem;
    }

    .footer-links a {
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    /* Map Modal */
    #map-modal {
        display: none;
        position: fixed;
        inset: 0;
        z-index: 1000;
        background: rgba(15, 23, 42, 0.75);
        backdrop-filter: blur(4px);
        align-items: center;
        justify-content: center;
        padding: 2rem;
    }

    #map-modal.open {
        display: flex;
    }

    #map-container {
        width: 100%;
        max-width: 1000px;
        height: 70vh;
        background: white;
        border-radius: 16px;
        box-shadow: 0 20px 25px -5px rgba(0,0,0,0.1);
        display: flex;
        flex-direction: column;
        overflow: hidden;
    }

    #map-header {
        padding: 1.25rem 1.5rem;
        border-bottom: 1px solid var(--border);
        display: flex;
        justify-content: space-between;
        align-items: center;
    }

    #map {
        flex: 1;
    }
</style>

</head>
<body>
    <div class="container">
    <header>
        <h1>GTFS Schedule Validation Report</h1>
        <p>Generated by <strong>GTFS.guru Validator</strong>"#
    );
    if let Some(version) = &context.validator_version {
        out.push_str(" (version ");
        push_escaped(&mut out, version);
        out.push(')');
    }
    out.push_str(" at ");
    push_escaped(&mut out, &context.validated_at);
    out.push_str(
        r#"</p>
        <p>Dataset: <strong>"#,
    );
    push_escaped(&mut out, &context.gtfs_source);
    out.push_str("</strong>");

    if is_unknown_country_code(&context.country_code) {
        out.push_str(". <span class='badge bg-slate-100'>No country code provided</span>");
    } else {
        out.push_str(", Country: <strong>");
        push_escaped(&mut out, &context.country_code);
        out.push_str("</strong>");
    }

    if is_different_date(&context.date_for_validation) {
        out.push_str("<br/>Validation Date: <strong>");
        push_escaped(&mut out, &context.date_for_validation);
        out.push_str("</strong>");
    }
    out.push_str("</p>");

    if context.new_version_available {
        out.push_str(
            r#"<p class="version-update" style="color: var(--error); font-weight: bold; margin-top: 1rem;">
               A new version of the <a href="https://github.com/abasis-ltd/gtfs.guru/releases">GTFS.guru Validator</a> is available!
               Please update for the latest validation rules.
            </p>"#,
        );
    }
    out.push_str("</header>\n\n");

    out.push_str("    <h2 class=\"section-title\">Summary</h2>\n\n");

    if has_metadata(summary) {
        out.push_str("    <div class=\"summary-grid\">\n");
        render_agencies(&mut out, summary);
        render_feed_info(&mut out, summary);
        render_files(&mut out, summary);
        render_counts(&mut out, summary);
        render_features(&mut out, summary);
        out.push_str("    </div>\n\n");
    }

    let notice_counts = NoticeCounts::from_container(notices);
    out.push_str("    <h2 class=\"section-title\">Specification Compliance</h2>\n\n");
    out.push_str("    <div class=\"compliance-stats\">\n");

    write!(
        &mut out,
        r#"<div class="stat-pill"><span class="count">{}</span> Total Notices</div>"#,
        notice_counts.total
    )
    .ok();
    write!(&mut out, r#"<div class="stat-pill"><span class="badge error-badge">ERROR</span> <span class="count">{}</span></div>"#, notice_counts.errors).ok();
    write!(&mut out, r#"<div class="stat-pill"><span class="badge warning-badge">WARNING</span> <span class="count">{}</span></div>"#, notice_counts.warnings).ok();
    write!(&mut out, r#"<div class="stat-pill"><span class="badge info-badge">INFO</span> <span class="count">{}</span></div>"#, notice_counts.infos).ok();

    out.push_str("    </div>\n\n");

    out.push_str(
        r#"    <table class="accordion">
        <thead>
        <tr>
            <th>Notice Code</th>
            <th>Severity</th>
            <th>Total</th>
        </tr>
        </thead>
        <tbody>
"#,
    );
    render_notice_groups(&mut out, notices);
    out.push_str(r#"        </tbody>
    </table>
    <br>

    <!-- Map Modal -->
    <div id="map-modal">
        <div id="map-container">
            <div id="map-header">
                <h3 id="map-title">Geographic Error</h3>
                <button id="close-map" style="background:none; border:none; font-size:24px; cursor:pointer;">&times;</button>
            </div>
            <div id="map"></div>
        </div>
    </div>

    <footer>
        <p><strong>GTFS.guru</strong> - The Gold Standard for GTFS Validation</p>
        <div class="footer-links">
            <a href="https://gtfs.guru" target="_blank">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
                Website
            </a>
            <a href="https://github.com/abasis-ltd/gtfs.guru" target="_blank">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"></path></svg>
                GitHub
            </a>
        </div>
        <p style="font-size: 0.75rem; margin-top: 1.5rem;">Report generated using GTFS.guru ruleset. Based on open standards.</p>
    </footer>
    </div>
"#);

    out
}

fn has_metadata(summary: &ReportSummary) -> bool {
    summary.feed_info.is_some()
        || summary.agencies.is_some()
        || summary.files.is_some()
        || summary.counts.is_some()
        || summary.gtfs_features.is_some()
}

fn render_agencies(out: &mut String, summary: &ReportSummary) {
    out.push_str("            <div class=\"card\">\n                <h4>Agencies Included</h4>\n                <ul>\n");
    if let Some(agencies) = summary.agencies.as_ref() {
        for agency in agencies {
            out.push_str("                    <li>");
            push_escaped(out, &agency.name);
            out.push_str("\n                        <ul>\n                            <li><b>website: </b><a href=\"");
            push_escaped(out, &agency.url);
            out.push_str("\">");
            push_escaped(out, &agency.url);
            out.push_str("</a></li>\n                            <li><b>phone number: </b>");
            push_escaped(out, &agency.phone);
            out.push_str("</li>\n                            <li><b>email: </b>");
            if agency.email.trim().is_empty() {
                out.push_str("Not provided");
            } else {
                push_escaped(out, &agency.email);
            }
            out.push_str("</li>\n                        </ul>\n                    </li>\n");
        }
    }
    out.push_str("                </ul>\n            </div>\n");
}

fn render_feed_info(out: &mut String, summary: &ReportSummary) {
    out.push_str("            <div class=\"card\">\n                <h4>Feed Info</h4>\n                <dl>\n");
    if let Some(info) = summary.feed_info.as_ref() {
        for (key, value) in build_feed_info_entries(info) {
            out.push_str("                    <dt>");
            push_escaped(out, &format!("{key}:"));
            out.push_str("</dt>\n                    <dd>\n");
            if key.contains("URL") && !value.trim().is_empty() {
                out.push_str("                        <a href=\"");
                push_escaped(out, &value);
                out.push_str("\" target=\"_blank\">");
                push_escaped(out, &value);
                out.push_str("</a>\n");
            } else if value.trim().is_empty() {
                out.push_str("                        N/A\n");
            } else {
                out.push_str("                        ");
                push_escaped(out, &value);
                out.push('\n');
            }
            if key == "Service Window" {
                out.push_str(
                    "                        <a href=\"#\" class=\"tooltip\" onclick=\"event.preventDefault();\">(?)<span class=\"tooltiptext\">The range of service dates covered by the feed, based on trips with an associated service_id in calendar.txt and/or calendar_dates.txt</span></a>\n",
                );
            }
            out.push_str("                    </dd>\n");
        }
    }
    out.push_str("                </dl>\n            </div>\n");
}

fn render_files(out: &mut String, summary: &ReportSummary) {
    out.push_str("            <div class=\"card\">\n                <h4>Files Included</h4>\n                <ol>\n");
    if let Some(files) = summary.files.as_ref() {
        for file in files {
            out.push_str("                    <li>");
            push_escaped(out, file);
            out.push_str("</li>\n");
        }
    }
    out.push_str("                </ol>\n            </div>\n");
}

fn render_counts(out: &mut String, summary: &ReportSummary) {
    out.push_str(
        "            <div class=\"card\">\n                <h4>Counts</h4>\n                <ul>\n",
    );
    if let Some(counts) = summary.counts.as_ref() {
        for (key, value) in build_counts_entries(counts) {
            out.push_str("                    <li>");
            push_escaped(out, &format!("{key}: {value}"));
            out.push_str("</li>\n");
        }
    }
    out.push_str("                </ul>\n            </div>\n");
}

fn render_features(out: &mut String, summary: &ReportSummary) {
    if let Some(features) = summary.gtfs_features.as_ref() {
        if !features.is_empty() {
            out.push_str("            <div class=\"card\">\n                <h4>GTFS Features Included</h4>\n                <div style=\"display: flex; flex-wrap: wrap; gap: 4px;\">\n");
            for feature in build_feature_entries(features) {
                out.push_str("                    <span class=\"spec-feature\">");
                out.push_str("<a href=\"");
                push_escaped(out, &feature.doc_url);
                out.push_str("\" target=\"_blank\">");
                push_escaped(out, &feature.name);
                out.push_str("</a></span>\n");
            }
            out.push_str("                </div>\n            </div>\n");
        }
    }
}

fn build_feed_info_entries(info: &ReportFeedInfo) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    entries.push((
        "Publisher Name".to_string(),
        info.publisher_name.clone().unwrap_or_default(),
    ));
    entries.push((
        "Publisher URL".to_string(),
        info.publisher_url.clone().unwrap_or_default(),
    ));
    entries.push((
        "Feed Email".to_string(),
        info.feed_email.clone().unwrap_or_default(),
    ));
    entries.push((
        "Feed Language".to_string(),
        info.feed_language.clone().unwrap_or_default(),
    ));
    if let Some(value) = info.feed_start_date.as_ref() {
        entries.push(("Feed Start Date".to_string(), value.clone()));
    }
    if let Some(value) = info.feed_end_date.as_ref() {
        entries.push(("Feed End Date".to_string(), value.clone()));
    }
    if info.feed_service_window_start.is_some() || info.feed_service_window_end.is_some() {
        entries.push(("Service Window".to_string(), service_window_display(info)));
    }
    entries
}

fn service_window_display(info: &ReportFeedInfo) -> String {
    let start = parse_date(info.feed_service_window_start.as_deref());
    let end = parse_date(info.feed_service_window_end.as_deref());

    match (start, end) {
        (None, None) => String::new(),
        (Some(start), None) => start.format("%B %-d, %Y").to_string(),
        (None, Some(end)) => end.format("%B %-d, %Y").to_string(),
        (Some(start), Some(end)) => format!("{} to {}", start, end),
    }
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
}

fn build_counts_entries(counts: &ReportCounts) -> Vec<(String, usize)> {
    let mut ordered = BTreeMap::new();
    ordered.insert("Shapes".to_string(), counts.shapes);
    ordered.insert("Stops".to_string(), counts.stops);
    ordered.insert("Routes".to_string(), counts.routes);
    ordered.insert("Trips".to_string(), counts.trips);
    ordered.insert("Agencies".to_string(), counts.agencies);
    ordered.insert("Blocks".to_string(), counts.blocks);
    ordered.into_iter().collect()
}

struct FeatureEntry {
    name: String,
    doc_url: String,
}

fn build_feature_entries(features: &[String]) -> Vec<FeatureEntry> {
    features
        .iter()
        .map(|name| FeatureEntry {
            name: name.clone(),
            doc_url: feature_doc_url(name),
        })
        .collect()
}

fn feature_doc_url(name: &str) -> String {
    let group = feature_group(name).unwrap_or("base_add-ons");
    let feature_name = name.to_lowercase().replace(' ', "-");
    let feature_group = group.to_lowercase().replace(' ', "_");
    format!("{GTFS_FEATURE_BASE_URL}{feature_group}/#{feature_name}")
}

fn feature_group(name: &str) -> Option<&'static str> {
    match name {
        "Pathway Connections" => Some("Pathways"),
        "Pathway Signs" => Some("Pathways"),
        "Pathway Details" => Some("Pathways"),
        "Levels" => Some("Pathways"),
        "Fares V1" => Some("Fares"),
        "Fare Products" => Some("Fares"),
        "Fare Media" => Some("Fares"),
        "Zone-Based Fares" => Some("Fares"),
        "Fare Transfers" => Some("Fares"),
        "Time-Based Fares" => Some("Fares"),
        "Rider Categories" => Some("Fares"),
        "Booking Rules" => Some("Flexible Services"),
        "Fixed-Stops Demand Responsive Transit" => Some("Flexible Services"),
        "Route-Based Fares" => Some("Fares"),
        "Continuous Stops" => Some("Flexible Services"),
        "Zone-Based Demand Responsive Services" => Some("Flexible Services"),
        "Predefined Routes with Deviation" => Some("Flexible Services"),
        "In-station Traversal Time" => Some("Pathways"),
        "Text-to-Speech" => Some("Accessibility"),
        "Stops Wheelchair Accessibility" => Some("Accessibility"),
        "Trips Wheelchair Accessibility" => Some("Accessibility"),
        _ => None,
    }
}

struct NoticeCounts {
    total: usize,
    errors: usize,
    warnings: usize,
    infos: usize,
}

impl NoticeCounts {
    fn from_container(container: &NoticeContainer) -> Self {
        let mut counts = Self {
            total: 0,
            errors: 0,
            warnings: 0,
            infos: 0,
        };
        for notice in container.iter() {
            counts.total += 1;
            match notice.severity {
                NoticeSeverity::Error => counts.errors += 1,
                NoticeSeverity::Warning => counts.warnings += 1,
                NoticeSeverity::Info => counts.infos += 1,
            }
        }
        counts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HtmlSeverity {
    Error,
    Warning,
    Info,
}

impl HtmlSeverity {
    fn from_notice(severity: NoticeSeverity) -> Self {
        match severity {
            NoticeSeverity::Error => HtmlSeverity::Error,
            NoticeSeverity::Warning => HtmlSeverity::Warning,
            NoticeSeverity::Info => HtmlSeverity::Info,
        }
    }

    fn label(self) -> &'static str {
        match self {
            HtmlSeverity::Error => "ERROR",
            HtmlSeverity::Warning => "WARNING",
            HtmlSeverity::Info => "INFO",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            HtmlSeverity::Error => "error",
            HtmlSeverity::Warning => "warning",
            HtmlSeverity::Info => "info",
        }
    }
}

fn render_notice_groups(out: &mut String, notices: &NoticeContainer) {
    let grouped = group_notices(notices);
    for severity in [
        HtmlSeverity::Error,
        HtmlSeverity::Warning,
        HtmlSeverity::Info,
    ] {
        if let Some(code_map) = grouped.get(&severity) {
            for (code, notices) in code_map {
                render_notice_group(out, severity, code, notices);
            }
        }
    }
}

fn group_notices(
    notices: &NoticeContainer,
) -> HashMap<HtmlSeverity, BTreeMap<String, Vec<&ValidationNotice>>> {
    let mut grouped: HashMap<HtmlSeverity, BTreeMap<String, Vec<&ValidationNotice>>> =
        HashMap::new();
    for notice in notices.iter() {
        grouped
            .entry(HtmlSeverity::from_notice(notice.severity))
            .or_default()
            .entry(notice.code.clone())
            .or_default()
            .push(notice);
    }
    grouped
}

fn render_notice_group(
    out: &mut String,
    severity: HtmlSeverity,
    code: &str,
    notices: &[&ValidationNotice],
) {
    let fields = notice_fields(notices);
    let description = notices
        .first()
        .map(|notice| notice.message.as_str())
        .unwrap_or("");

    // Check if this is a geographic notice that should have a map button
    let has_map_data = notices
        .iter()
        .any(|n| n.context.contains_key("stopLocation") && n.context.contains_key("match"));

    out.push_str("            <tr class=\"notice\">\n                <td style='position:relative; padding-left: 2rem;'>\n                    <span style='position:absolute; left: 0.75rem;'>+</span>\n                    <span class='notice-code'>");
    push_escaped(out, code);
    out.push_str("</span>\n                </td>\n                <td><span class=\"badge ");
    out.push_str(severity.css_class());
    out.push_str("-badge\">");
    out.push_str(severity.label());
    out.push_str("</span></td>\n                <td style='font-weight: 700;'>");
    write!(out, "{}", notices.len()).ok();
    out.push_str("</td>\n            </tr>\n            <tr class=\"description\">\n                <td colspan=\"3\">\n                    <div class=\"desc-content\">\n                        <h3>");
    push_escaped(out, code);
    out.push_str("</h3>\n                        <p style='font-size: 1.1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.75rem; margin-bottom: 1rem;'>");
    push_escaped(out, description);
    out.push_str("</p>\n                        <p> View documentation for <a\n                                href=\"https://gtfs-validator.mobilitydata.org/rules.html#");
    push_escaped(out, code);
    out.push_str("-rule\" target='_blank'>");
    push_escaped(out, code);
    out.push_str("</a>.\n                        </p>\n");
    if notices.len() > NOTICE_ROW_LIMIT {
        out.push_str("                         <p>Only the first 50 of ");
        write!(out, "{}", notices.len()).ok();
        out.push_str(" affected records are displayed below.</p>\n");
    }

    if !fields.is_empty() {
        out.push_str("                        <table>\n                            <thead>\n                                <tr>\n");
        for field in &fields {
            out.push_str("                                    <th>\n                                        <span>");
            push_escaped(out, field);
            out.push_str("</span>\n                                        <a href=\"#\" class=\"tooltip\" onclick=\"event.preventDefault();\"><span>(?)</span>\n                                            <span class=\"tooltiptext\"></span>\n                                        </a>\n                                    </th>\n");
        }
        // Add Map column header for geographic notices
        if has_map_data {
            out.push_str("                                    <th><span>Map</span></th>\n");
        }
        out.push_str("                                </tr>\n                            </thead>\n                            <tbody>\n");
        for notice in notices.iter().take(NOTICE_ROW_LIMIT) {
            out.push_str("                                <tr>\n");
            for field in &fields {
                out.push_str("                                    <td>");
                render_notice_field_value(out, notice, field);
                out.push_str("</td>\n");
            }
            // Add Map button cell for geographic notices
            if has_map_data {
                render_map_button(out, notice);
            }
            out.push_str("                                </tr>\n");
        }
        out.push_str("                            </tbody>\n                        </table>\n");
    }
    out.push_str("                    </div>\n                </td>\n            </tr>\n");
}

fn render_map_button(out: &mut String, notice: &ValidationNotice) {
    let stop_location = notice.context.get("stopLocation");
    let match_location = notice.context.get("match");
    let shape_path = notice.context.get("shapePath");
    let stop_name = notice
        .context
        .get("stopName")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    if let (Some(stop_loc), Some(match_loc)) = (stop_location, match_location) {
        let (stop_lat, stop_lon) = extract_lat_lng(stop_loc);
        let (match_lat, match_lon) = extract_lat_lng(match_loc);

        if let (Some(s_lat), Some(s_lon), Some(m_lat), Some(m_lon)) =
            (stop_lat, stop_lon, match_lat, match_lon)
        {
            out.push_str("                                    <td>");
            out.push_str("<button class=\"view-map-btn\" ");
            out.push_str("data-stop-name=\"");
            push_escaped(out, stop_name);
            out.push_str("\" ");
            out.push_str(&format!("data-stop-lat=\"{}\" ", s_lat));
            out.push_str(&format!("data-stop-lon=\"{}\" ", s_lon));
            out.push_str(&format!("data-match-lat=\"{}\" ", m_lat));
            out.push_str(&format!("data-match-lon=\"{}\" ", m_lon));
            // Add shape path if available
            if let Some(path) = shape_path {
                if let Ok(json_str) = serde_json::to_string(path) {
                    out.push_str("data-shape-path='");
                    out.push_str(&json_str);
                    out.push_str("' ");
                }
            }
            out.push_str(">📍 View</button>");
            out.push_str("</td>\n");
            return;
        }
    }
    out.push_str("                                    <td>-</td>\n");
}

fn extract_lat_lng(value: &Value) -> (Option<f64>, Option<f64>) {
    if let Some(arr) = value.as_array() {
        if arr.len() >= 2 {
            let lat = arr[0].as_f64();
            let lon = arr[1].as_f64();
            return (lat, lon);
        }
    }
    (None, None)
}

fn notice_fields(notices: &[&ValidationNotice]) -> Vec<String> {
    if notices.is_empty() {
        return Vec::new();
    }

    let mut union = HashSet::new();
    for notice in notices {
        for key in notice.context.keys() {
            union.insert(key.clone());
        }
        if notice.file.is_some() {
            union.insert("filename".to_string());
        }
        if notice.row.is_some() {
            union.insert("csvRowNumber".to_string());
        }
        if notice.field.is_some() {
            union.insert("fieldName".to_string());
        }
    }

    let first = notices[0];
    let mut ordered = if !first.field_order.is_empty() {
        first.field_order.clone()
    } else if !first.context.is_empty() {
        first.context.keys().cloned().collect()
    } else {
        default_notice_fields(notices)
    };

    if !ordered.is_empty() {
        ordered.retain(|field| union.contains(field));
        dedup_fields(&mut ordered);
        return ordered;
    }

    let mut ordered: Vec<String> = union.into_iter().collect();
    ordered.sort();
    ordered
}

fn default_notice_fields(notices: &[&ValidationNotice]) -> Vec<String> {
    let mut fields = Vec::new();
    if notices.iter().any(|notice| notice.file.is_some()) {
        fields.push("filename".to_string());
    }
    if notices.iter().any(|notice| notice.row.is_some()) {
        fields.push("csvRowNumber".to_string());
    }
    if notices.iter().any(|notice| notice.field.is_some()) {
        fields.push("fieldName".to_string());
    }
    fields
}

fn dedup_fields(fields: &mut Vec<String>) {
    let mut seen = HashSet::new();
    fields.retain(|field| seen.insert(field.clone()));
}

fn render_notice_field_value(out: &mut String, notice: &ValidationNotice, field: &str) {
    if let Some(value) = notice_field_value(notice, field) {
        render_json_value(out, &value);
    } else {
        out.push_str("N/A");
    }
}

fn notice_field_value(notice: &ValidationNotice, field: &str) -> Option<Value> {
    match field {
        "filename" => notice.context.get(field).cloned().or_else(|| {
            notice
                .file
                .as_ref()
                .map(|value| Value::String(value.clone()))
        }),
        "csvRowNumber" => notice
            .context
            .get(field)
            .cloned()
            .or_else(|| notice.row.map(|value| Value::Number(Number::from(value)))),
        "fieldName" => notice.context.get(field).cloned().or_else(|| {
            notice
                .field
                .as_ref()
                .map(|value| Value::String(value.clone()))
        }),
        _ => notice.context.get(field).cloned(),
    }
}

fn render_json_value(out: &mut String, value: &Value) {
    match value {
        Value::String(text) => push_escaped(out, text),
        Value::Number(num) => {
            if let Some(text) = num.as_i64().map(|v| v.to_string()) {
                out.push_str(&text);
            } else if let Some(text) = num.as_u64().map(|v| v.to_string()) {
                out.push_str(&text);
            } else if let Some(text) = num.as_f64().map(|v| v.to_string()) {
                out.push_str(&text);
            } else {
                out.push_str("N/A");
            }
        }
        Value::Bool(flag) => {
            out.push_str(if *flag { "true" } else { "false" });
        }
        Value::Null => out.push_str("N/A"),
        other => {
            push_escaped(out, &other.to_string());
        }
    }
}

fn is_unknown_country_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case(DEFAULT_COUNTRY_CODE)
}

fn is_different_date(date_for_validation: &str) -> bool {
    NaiveDate::parse_from_str(date_for_validation, "%Y-%m-%d")
        .map(|date| date != Local::now().date_naive())
        .unwrap_or(false)
}

fn push_escaped(out: &mut String, value: &str) {
    out.push_str(&escape_html(value));
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
