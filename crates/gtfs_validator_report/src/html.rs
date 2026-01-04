use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::Context;
use chrono::{Local, NaiveDate, SecondsFormat};
use serde_json::{Number, Value};

use gtfs_validator_core::{NoticeContainer, NoticeSeverity, ValidationNotice};

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
    <script src="https://code.jquery.com/jquery-3.6.0.min.js"></script>
    <script src="https://code.jquery.com/ui/1.12.1/jquery-ui.js"></script>
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" integrity="sha256-p4NxAoJBhIIN+hmNHrzRCf9tD/miZyoHS5obTRR9BMY=" crossorigin=""/>
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js" integrity="sha256-20nQCchB9co0qIjJZRGuk2/Z9VM+kNiyxNV1lvTlZBo=" crossorigin=""></script>
    <script>
      $(document).ready(function () {
        $(document).tooltip();
      });
    </script>
    <style>
    body {

        font-family: Helvetica, Arial, sans-serif;
        font-size: 14px;
        min-width: 800px;
        padding: 1em 2em;
    }

    .error:before {
        content: "\1F534  ";
    }

    .warning:before {
        content: "\1F7E0  ";
    }

    .info:before {
        content: "\26AA  ";
    }

    * {
        box-sizing: border-box;
    }

    .version-update {
        font-weight: bold;
        color: red;
    }

    table {
        width: 100%;
    }

    table caption {
        text-align: left;
        margin: 0.5em 0;
    }

    table th {
        text-align: left;
        border-bottom: 2px solid #000;
        padding: 0.5em;
        white-space: nowrap;
    }

    table td {
        border-bottom: 1px solid #ddd;
        padding: 0.5em;
    }

    .desc-content {
        padding: 0.5em;
        border-bottom: 5px solid #000;
        border-top: 5px solid #000;
    }

    .desc-content h3 {
        margin-top: 0;
    }

    .summary {
        display: flex;
        flex-wrap: wrap;
    }

    .summary-row {
        display: flex;
        flex-wrap: wrap;
        width: 100%;
    }

    .summary-cell {
        padding: 5px;
        box-sizing: border-box;
        flex: 1;
    }

    .summary h4 {
        white-space: nowrap;
    }

    .summary dt,
    .summary dd {
        display: inline-block;
    }

    .summary dd {
        font-weight: bold;
        width: 130px;
        margin-inline-start: 0;
    }

    .summary ul,
    .summary ol {
        padding-left: 20px;
    }

    .summary li {
        padding-left: 0px;
    }

    hr {
        border: none;
        border-top: 2px solid #000;
        margin-top: 5px;
        margin-bottom: 15px;
    }

    .spec-feature {
        background-color: #d4d4d4;
        padding: 2px 5px;
        margin-right: 2px;
        margin-bottom: 2px;
        text-align: center;
    }

    .tooltip {
        text-decoration: none;
        position: relative;
        display: inline-block;
        cursor: pointer;
    }

    .tooltip .tooltiptext {
        background-color: #555;
        color: #fff;
        text-align: center;
        border-radius: 6px;
        padding: 5px 5px;
        position: absolute;
        z-index: 1;
        bottom: 100%;
        transform: translateX(-50%);
        opacity: 0;
        transition: opacity 0.3s;
        max-width: 400px;
        min-width: 100px;
        width: max-content;
        white-space: normal;
    }

    .tooltip {
        position: relative;
        display: inline-block;
        cursor: help;
    }

    .tooltip:hover .tooltiptext {
        visibility: visible;
        opacity: 1;
    }

    /* Responsive behavior */
    @media (max-width: 767px) {
        .summary .summary_info,
        .summary .summary_list {
            flex-basis: 100%;
        }
    }

    table.accordion > tbody > tr.notice td,
    table.accordion > tbody > tr.view th {
        cursor: auto;
    }

    table.accordion > tbody > tr.notice td:first-child,
    table.accordion > tbody > tr.notice th:first-child {
        position: relative;
        padding-left: 20px;
    }

    table.accordion > tbody > tr.notice td:first-child:before,
    table.accordion > tbody > tr.notice th:first-child:before {
        position: absolute;
        top: 50%;
        left: 5px;
        width: 9px;
        height: 16px;
        margin-top: -8px;
        color: #000;
        content: "+";
    }

    table.accordion > tbody > tr.notice.open td:first-child:before,
    table.accordion > tbody > tr.notice.open th:first-child:before {
        content: "\2013";
    }

    table.accordion > tbody > tr.notice:hover {
        background: #ddd;
    }

    table.accordion > tbody > tr.notice.open {
        background: #ddd;
        color: black;
    }

    table.accordion > tbody > tr.description {
        display: none;
    }

    table.accordion > tbody > tr.description.open {
        display: table-row;
    }

    #gtfs-features-container > div {
        display: flex;
        justify-content: center;
        align-content: center;
        flex-wrap: wrap;
    }

    /* Map visualization styles */
    #map-modal {
        display: none;
        position: fixed;
        z-index: 1000;
        left: 0;
        top: 0;
        width: 100%;
        height: 100%;
        background-color: rgba(0,0,0,0.7);
    }
    #map-modal.open {
        display: flex;
        align-items: center;
        justify-content: center;
    }
    #map-container {
        width: 90%;
        max-width: 1000px;
        height: 70vh;
        background: #fff;
        border-radius: 8px;
        overflow: hidden;
        position: relative;
    }
    #map {
        width: 100%;
        height: calc(100% - 50px);
    }
    #map-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 10px 15px;
        background: #f5f5f5;
        border-bottom: 1px solid #ddd;
    }
    #map-header h3 {
        margin: 0;
        font-size: 16px;
    }
    #close-map {
        background: none;
        border: none;
        font-size: 24px;
        cursor: pointer;
        color: #666;
    }
    #close-map:hover {
        color: #000;
    }
    .view-map-btn {
        background: #4CAF50;
        color: white;
        border: none;
        padding: 4px 8px;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
        margin-left: 8px;
    }
    .view-map-btn:hover {
        background: #45a049;
    }
</style>

</head>
<body>
    <h1>GTFS Schedule Validation Report</h1>
    <p>This report was generated by the <a href="https://github.com/MobilityData/gtfs-validator">Canonical GTFS Schedule
        validator</a>"#,
    );
    if let Some(version) = &context.validator_version {
        out.push_str(", ");
        out.push_str("version ");
        push_escaped(&mut out, version);
    }
    out.push_str(" at ");
    push_escaped(&mut out, &context.validated_at);
    out.push_str(",\n        <br/>\n        for the dataset\n        ");
    push_escaped(&mut out, &context.gtfs_source);
    if is_unknown_country_code(&context.country_code) {
        out.push_str(". No country code was provided.");
    } else {
        out.push_str(", with the country code: ");
        push_escaped(&mut out, &context.country_code);
        out.push('.');
    }
    out.push_str("\n        </br>\n        ");
    if is_different_date(&context.date_for_validation) {
        out.push_str("The date used during validation was ");
        push_escaped(&mut out, &context.date_for_validation);
        out.push('.');
    }
    out.push_str("</p>\n\n    <p>Use this report alongside our <a href=\"https://gtfs-validator.mobilitydata.org/rules.html\">documentation</a>.</p>\n\n");
    if context.new_version_available {
        out.push_str(
            "    <p class=\"version-update\">A new version of the <a\n            href=\"https://github.com/MobilityData/gtfs-validator/releases\">Canonical GTFS Schedule validator</a> is available!\n        Please update to get the latest/best validation results.</p>\n\n",
        );
    }

    out.push_str("    <h2>Summary</h2>\n\n");

    if has_metadata(summary) {
        out.push_str("    <div class=\"summary\">\n        <div class=\"summary-row\">\n");
        render_agencies(&mut out, summary);
        render_feed_info(&mut out, summary);
        render_files(&mut out, summary);
        render_counts(&mut out, summary);
        render_features(&mut out, summary);
        out.push_str("        </div>\n    </div>\n\n");
    }

    let notice_counts = NoticeCounts::from_container(notices);
    out.push_str("    <h2>Specification Compliance report</h2>\n\n    <h3><span>");
    write!(&mut out, "{}", notice_counts.total).ok();
    out.push_str("</span> notices reported\n        (<span>");
    write!(&mut out, "{}", notice_counts.errors).ok();
    out.push_str("</span> errors,\n        <span>");
    write!(&mut out, "{}", notice_counts.warnings).ok();
    out.push_str("</span> warnings,\n        <span>");
    write!(&mut out, "{}", notice_counts.infos).ok();
    out.push_str("</span> infos)\n    </h3>\n\n");

    out.push_str("    <table class=\"accordion\">\n        <thead>\n        <tr>\n            <th>Notice Code</th>\n            <th>Severity</th>\n            <th>Total</th>\n        </tr>\n        </thead>\n        <tbody>\n");
    render_notice_groups(&mut out, notices);
    out.push_str("        </tbody>\n    </table>\n    <br>\n\n    <!-- Map Modal -->\n    <div id=\"map-modal\">\n        <div id=\"map-container\">\n            <div id=\"map-header\">\n                <h3 id=\"map-title\">Geographic Error</h3>\n                <button id=\"close-map\">&times;</button>\n            </div>\n            <div id=\"map\"></div>\n        </div>\n    </div>\n\n    <footer class=\"footer text-center text-muted mt-5\">\n        Made with ");
    out.push('\u{2665}');
    out.push_str(" by <a href=\"https://mobilitydata.org/\">MobilityData</a>\n    </footer>\n</body>\n    <script>\n        $(function () {\n            $(\".accordion tr.notice\").on(\"click\", function () {\n                $(this).toggleClass(\"open\").next(\".description\").toggleClass(\"open\")\n            });\n        });\n\n        // Map functionality\n        var map = null;\n        var mapModal = document.getElementById('map-modal');\n        var closeBtn = document.getElementById('close-map');\n\n        function showMap(stopName, stopLat, stopLon, matchLat, matchLon, shapePath) {\n            if (!map) {\n                map = L.map('map');\n                L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {\n                    attribution: '&copy; <a href=\"https://www.openstreetmap.org/copyright\">OpenStreetMap</a> contributors'\n                }).addTo(map);\n            } else {\n                map.eachLayer(function(layer) {\n                    if (layer instanceof L.Marker || layer instanceof L.Polyline) {\n                        map.removeLayer(layer);\n                    }\n                });\n            }\n\n            var bounds = [[stopLat, stopLon], [matchLat, matchLon]];\n\n            // Draw shape path if available (gray background line)\n            if (shapePath && shapePath.length > 0) {\n                L.polyline(shapePath, {\n                    color: '#3498db',\n                    weight: 4,\n                    opacity: 0.7\n                }).addTo(map);\n                // Add shape points to bounds\n                shapePath.forEach(function(pt) {\n                    bounds.push(pt);\n                });\n            }\n\n            var stopIcon = L.divIcon({\n                className: 'custom-div-icon',\n                html: '<div style=\"background: #e74c3c; width: 12px; height: 12px; border-radius: 50%; border: 2px solid white;\"></div>',\n                iconSize: [12, 12],\n                iconAnchor: [6, 6]\n            });\n            var matchIcon = L.divIcon({\n                className: 'custom-div-icon',\n                html: '<div style=\"background: #2ecc71; width: 12px; height: 12px; border-radius: 50%; border: 2px solid white;\"></div>',\n                iconSize: [12, 12],\n                iconAnchor: [6, 6]\n            });\n\n            L.marker([stopLat, stopLon], {icon: stopIcon})\n                .bindPopup('<b>Stop:</b> ' + stopName + '<br><b>Location:</b> ' + stopLat.toFixed(6) + ', ' + stopLon.toFixed(6))\n                .addTo(map);\n\n            L.marker([matchLat, matchLon], {icon: matchIcon})\n                .bindPopup('<b>Closest point on shape</b><br><b>Location:</b> ' + matchLat.toFixed(6) + ', ' + matchLon.toFixed(6))\n                .addTo(map);\n\n            // Draw error line (dashed red line from stop to match)\n            L.polyline([[stopLat, stopLon], [matchLat, matchLon]], {\n                color: '#e74c3c',\n                weight: 2,\n                dashArray: '5, 5'\n            }).addTo(map);\n\n            map.fitBounds(bounds, {padding: [50, 50]});\n\n            document.getElementById('map-title').textContent = 'Stop: ' + stopName;\n            mapModal.classList.add('open');\n            setTimeout(function() { map.invalidateSize(); }, 100);\n        }\n\n        closeBtn.onclick = function() {\n            mapModal.classList.remove('open');\n        };\n        mapModal.onclick = function(e) {\n            if (e.target === mapModal) {\n                mapModal.classList.remove('open');\n            }\n        };\n\n        // Handle view map button clicks\n        $(document).on('click', '.view-map-btn', function(e) {\n            e.stopPropagation();\n            var btn = $(this);\n            var shapePath = btn.data('shape-path');\n            showMap(\n                btn.data('stop-name'),\n                btn.data('stop-lat'),\n                btn.data('stop-lon'),\n                btn.data('match-lat'),\n                btn.data('match-lon'),\n                shapePath ? shapePath : null\n            );\n        });\n    </script>\n\n</html>\n");

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
    out.push_str("            <div class=\"summary-cell summary_info\">\n                <h4>Agencies included</h4>\n                <hr />\n                <ul>\n");
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
    out.push_str("            <div class=\"summary-cell summary_info\">\n                <h4>Feed Info</h4>\n                <hr />\n                <dl>\n");
    if let Some(info) = summary.feed_info.as_ref() {
        for (key, value) in build_feed_info_entries(info) {
            out.push_str("                    <dd>");
            push_escaped(out, &format!("{key}:"));
            out.push_str("</dd>\n                    <dt>\n");
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
                    "                        <a href=\"#\" class=\"tooltip\" onclick=\"event.preventDefault();\"><span>(?)</span>\n                            <span class=\"tooltiptext\" style=\"transform: translateX(-100%)\">The range of service dates covered by the feed, based on trips with an associated service_id in calendar.txt and/or calendar_dates.txt</span>\n                        </a>\n",
                );
            }
            out.push_str("                    </dt>\n");
        }
    }
    out.push_str("                </dl>\n            </div>\n");
}

fn render_files(out: &mut String, summary: &ReportSummary) {
    out.push_str("            <div class=\"summary-cell summary_list\">\n                <h4>Files included</h4>\n                <hr />\n                <ol>\n");
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
    out.push_str("            <div class=\"summary-cell summary_list\">\n                <h4>Counts</h4>\n                <hr />\n                <ul>\n");
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
    out.push_str("            <div class=\"summary-cell summary_list\" id=\"gtfs-features-container\">\n                <h4>\n                    GTFS Features included\n                    <a href=\"#\" class=\"tooltip\" onclick=\"event.preventDefault();\"><span>(?)</span>\n                        <span class=\"tooltiptext\" style=\"transform: translateX(-100%)\">GTFS features provide a standardized vocabulary to define and describe features that are officially adopted in GTFS.</span>\n                    </a>\n                </h4>\n                <hr />\n                <div>\n");
    if let Some(features) = summary.gtfs_features.as_ref() {
        for feature in build_feature_entries(features) {
            out.push_str("                    <span class=\"spec-feature\">");
            out.push_str("<a href=\"");
            push_escaped(out, &feature.doc_url);
            out.push_str("\" target=\"_blank\">");
            push_escaped(out, &feature.name);
            out.push_str("</a></span>\n");
        }
    }
    out.push_str("                </div>\n            </div>\n");
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

    out.push_str("            <tr class=\"notice\">\n                <td>");
    push_escaped(out, code);
    out.push_str("</td>\n                <td class=\"");
    out.push_str(severity.css_class());
    out.push_str("\">");
    out.push_str(severity.label());
    out.push_str("</td>\n                <td>");
    write!(out, "{}", notices.len()).ok();
    out.push_str("</td>\n            </tr>\n            <tr class=\"description\">\n                <td colspan=\"4\">\n                    <div class=\"desc-content\">\n                        <h3>");
    push_escaped(out, code);
    out.push_str("</h3>\n                        <p>");
    push_escaped(out, description);
    out.push_str("</p>\n                        <p> You can see more about this notice <a\n                                href=\"https://gtfs-validator.mobilitydata.org/rules.html#");
    push_escaped(out, code);
    out.push_str("-rule\">here</a>.\n                        </p>\n");
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

        if stop_lat.is_some() && match_lat.is_some() {
            out.push_str("                                    <td>");
            out.push_str("<button class=\"view-map-btn\" ");
            out.push_str("data-stop-name=\"");
            push_escaped(out, stop_name);
            out.push_str("\" ");
            out.push_str(&format!("data-stop-lat=\"{}\" ", stop_lat.unwrap()));
            out.push_str(&format!("data-stop-lon=\"{}\" ", stop_lon.unwrap()));
            out.push_str(&format!("data-match-lat=\"{}\" ", match_lat.unwrap()));
            out.push_str(&format!("data-match-lon=\"{}\" ", match_lon.unwrap()));
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
