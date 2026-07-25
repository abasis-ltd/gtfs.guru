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
const NOTICE_DOC_BASE_URL: &str = "https://gtfs.guru/notices/";

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
    <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self' 'unsafe-inline' https://unpkg.com; style-src 'self' 'unsafe-inline' https://unpkg.com; img-src 'self' data: blob: https:; connect-src 'self' https:; worker-src blob:; child-src blob:; object-src 'none'; base-uri 'none'; form-action 'none'"/>
    <link rel="stylesheet" href="https://unpkg.com/maplibre-gl@5.12.0/dist/maplibre-gl.css" crossorigin="anonymous"/>
    <script src="https://unpkg.com/maplibre-gl@5.12.0/dist/maplibre-gl.js" crossorigin="anonymous"></script>
    <script>
      document.addEventListener('DOMContentLoaded', function() {
        document.querySelectorAll('.accordion tr.notice').forEach(function(row) {
            row.addEventListener('click', function() {
                var descRow = this.nextElementSibling;
                if (descRow && descRow.classList.contains('description')) {
                    this.classList.toggle('open');
                    descRow.classList.toggle('open');
                    var icon = this.querySelector('span');
                    if (icon) {
                        icon.textContent = this.classList.contains('open') ? '–' : '+';
                    }
                }
            });
        });

        var modal = document.getElementById('map-modal');
        var closeButton = document.getElementById('close-map');
        var mapTitle = document.getElementById('map-title');
        var mapSubtitle = document.getElementById('map-subtitle');
        var map = null;
        var mapReady = false;
        var pendingGeometry = null;
        var markers = [];

        function emptyCollection() {
            return { type: 'FeatureCollection', features: [] };
        }

        function coordinates(point) {
            if (!point || !Number.isFinite(point.latitude) || !Number.isFinite(point.longitude)) {
                return null;
            }
            return [point.longitude, point.latitude];
        }

        function collectionForLine(points) {
            var line = (points || []).map(coordinates).filter(Boolean);
            return line.length > 1 ? {
                type: 'FeatureCollection',
                features: [{ type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: line } }]
            } : emptyCollection();
        }

        function setSourceData(id, data) {
            var source = map && map.getSource(id);
            if (source) source.setData(data);
        }

        function clearMarkers() {
            markers.forEach(function(marker) { marker.remove(); });
            markers = [];
        }

        function addMarker(point, kind, label) {
            var position = coordinates(point);
            if (!position) return;
            var element = document.createElement('div');
            element.className = 'notice-map-marker notice-map-marker--' + kind;
            element.setAttribute('aria-label', label);
            var popupContent = document.createElement('strong');
            popupContent.textContent = label;
            var popup = new maplibregl.Popup({ offset: 18, closeButton: false }).setDOMContent(popupContent);
            markers.push(new maplibregl.Marker({ element: element, anchor: 'center' })
                .setLngLat(position)
                .setPopup(popup)
                .addTo(map));
        }

        function renderGeometry(geometry) {
            if (!mapReady || !geometry) {
                pendingGeometry = geometry;
                return;
            }

            pendingGeometry = null;
            clearMarkers();
            setSourceData('notice-line', emptyCollection());
            setSourceData('notice-connector', emptyCollection());
            setSourceData('notice-bounds-fill', emptyCollection());

            var allCoordinates = [];
            if (geometry.type === 'point') {
                addMarker(geometry.point, 'point', 'Affected location');
                var point = coordinates(geometry.point);
                if (point) allCoordinates.push(point);
            } else if (geometry.type === 'line') {
                setSourceData('notice-line', collectionForLine(geometry.points));
                allCoordinates = (geometry.points || []).map(coordinates).filter(Boolean);
            } else if (geometry.type === 'pointAndLine') {
                setSourceData('notice-line', collectionForLine(geometry.line));
                addMarker(geometry.point, 'point', 'Affected stop');
                addMarker(geometry.nearestPoint, 'nearest', 'Closest point on shape');
                var affected = coordinates(geometry.point);
                var nearest = coordinates(geometry.nearestPoint);
                if (affected && nearest) {
                    setSourceData('notice-connector', collectionForLine([
                        geometry.point,
                        geometry.nearestPoint
                    ]));
                }
                allCoordinates = (geometry.line || []).map(coordinates).filter(Boolean);
                if (affected) allCoordinates.push(affected);
                if (nearest) allCoordinates.push(nearest);
            } else if (geometry.type === 'boundingBox') {
                var southWest = coordinates(geometry.southWest);
                var northEast = coordinates(geometry.northEast);
                if (southWest && northEast) {
                    var northWest = [southWest[0], northEast[1]];
                    var southEast = [northEast[0], southWest[1]];
                    var polygon = {
                        type: 'FeatureCollection',
                        features: [{
                            type: 'Feature',
                            properties: {},
                            geometry: {
                                type: 'Polygon',
                                coordinates: [[southWest, northWest, northEast, southEast, southWest]]
                            }
                        }]
                    };
                    setSourceData('notice-bounds-fill', polygon);
                    allCoordinates = [southWest, northEast];
                }
            }

            map.resize();
            if (allCoordinates.length === 1) {
                map.flyTo({ center: allCoordinates[0], zoom: 16, duration: 650 });
            } else if (allCoordinates.length > 1) {
                var bounds = allCoordinates.reduce(function(result, point) {
                    return result.extend(point);
                }, new maplibregl.LngLatBounds(allCoordinates[0], allCoordinates[0]));
                map.fitBounds(bounds, { padding: 72, maxZoom: 17, duration: 700 });
            }
        }

        function createMap() {
            if (map || typeof maplibregl === 'undefined') return;
            map = new maplibregl.Map({
                container: 'map',
                center: [0, 20],
                zoom: 1.5,
                attributionControl: false,
                style: {
                    version: 8,
                    sources: {
                        basemap: {
                            type: 'raster',
                            tiles: ['https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png'],
                            tileSize: 256,
                            attribution: '&copy; OpenStreetMap contributors &copy; CARTO'
                        }
                    },
                    layers: [{ id: 'basemap', type: 'raster', source: 'basemap' }]
                }
            });
            map.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
            map.addControl(new maplibregl.AttributionControl({ compact: true }), 'bottom-right');
            map.on('load', function() {
                map.addSource('notice-line', { type: 'geojson', data: emptyCollection() });
                map.addSource('notice-connector', { type: 'geojson', data: emptyCollection() });
                map.addSource('notice-bounds-fill', { type: 'geojson', data: emptyCollection() });
                map.addLayer({
                    id: 'notice-bounds-fill',
                    type: 'fill',
                    source: 'notice-bounds-fill',
                    paint: { 'fill-color': '#38bdf8', 'fill-opacity': 0.16 }
                });
                map.addLayer({
                    id: 'notice-bounds-line',
                    type: 'line',
                    source: 'notice-bounds-fill',
                    paint: { 'line-color': '#7dd3fc', 'line-width': 3 }
                });
                map.addLayer({
                    id: 'notice-line',
                    type: 'line',
                    source: 'notice-line',
                    layout: { 'line-cap': 'round', 'line-join': 'round' },
                    paint: { 'line-color': '#38bdf8', 'line-width': 5, 'line-opacity': 0.92 }
                });
                map.addLayer({
                    id: 'notice-connector',
                    type: 'line',
                    source: 'notice-connector',
                    paint: {
                        'line-color': '#fb7185',
                        'line-width': 3,
                        'line-dasharray': [1.5, 1.5]
                    }
                });
                mapReady = true;
                if (pendingGeometry) renderGeometry(pendingGeometry);
            });
        }

        function openMap(button) {
            var geometry;
            try {
                geometry = JSON.parse(button.dataset.geometry);
            } catch (_) {
                return;
            }
            mapTitle.textContent = button.dataset.mapTitle || 'Geographic notice';
            mapSubtitle.textContent = button.dataset.mapSubtitle || 'Affected geometry';
            modal.classList.add('open');
            modal.setAttribute('aria-hidden', 'false');
            document.body.classList.add('map-open');
            createMap();
            if (!map) {
                mapSubtitle.textContent = 'MapLibre could not be loaded. Check the network connection.';
                return;
            }
            renderGeometry(geometry);
            setTimeout(function() { map.resize(); }, 40);
        }

        function closeMap() {
            modal.classList.remove('open');
            modal.setAttribute('aria-hidden', 'true');
            document.body.classList.remove('map-open');
        }

        document.querySelectorAll('.view-map-btn').forEach(function(button) {
            button.addEventListener('click', function(event) {
                event.preventDefault();
                event.stopPropagation();
                openMap(button);
            });
        });
        closeButton.addEventListener('click', closeMap);
        modal.addEventListener('click', function(event) {
            if (event.target === modal) closeMap();
        });
        document.addEventListener('keydown', function(event) {
            if (event.key === 'Escape' && modal.classList.contains('open')) closeMap();
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
        display: inline-flex;
        align-items: center;
        gap: 0.45rem;
        background: #0f172a;
        color: #f8fafc;
        border: 1px solid #334155;
        padding: 0.48rem 0.78rem;
        border-radius: 999px;
        font-weight: 600;
        cursor: pointer;
        white-space: nowrap;
        transition: transform 0.18s ease, background 0.18s ease, border-color 0.18s ease;
    }

    .view-map-btn:hover {
        background: #1e293b;
        border-color: #38bdf8;
        transform: translateY(-1px);
    }

    .view-map-btn svg {
        width: 15px;
        height: 15px;
    }

    body.map-open {
        overflow: hidden;
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
        background: rgba(2, 6, 23, 0.82);
        backdrop-filter: blur(10px);
        align-items: center;
        justify-content: center;
        padding: clamp(0.75rem, 3vw, 2.5rem);
    }

    #map-modal.open {
        display: flex;
    }

    #map-container {
        width: 100%;
        max-width: 1180px;
        height: min(82vh, 780px);
        min-height: 420px;
        background: #0f172a;
        border: 1px solid rgba(148, 163, 184, 0.25);
        border-radius: 22px;
        box-shadow: 0 34px 80px rgba(2, 6, 23, 0.55);
        display: flex;
        flex-direction: column;
        overflow: hidden;
        animation: map-enter 0.24s ease-out both;
    }

    #map-header {
        min-height: 76px;
        padding: 1rem 1.25rem 1rem 1.5rem;
        border-bottom: 1px solid rgba(148, 163, 184, 0.18);
        display: flex;
        justify-content: space-between;
        align-items: center;
        color: #f8fafc;
    }

    #map-header h3 {
        margin: 0;
        font-size: 1.05rem;
        letter-spacing: -0.01em;
    }

    #map-subtitle {
        margin: 0.2rem 0 0;
        color: #94a3b8;
        font-size: 0.82rem;
    }

    #close-map {
        display: grid;
        place-items: center;
        width: 40px;
        height: 40px;
        flex: 0 0 auto;
        background: rgba(148, 163, 184, 0.1);
        border: 1px solid rgba(148, 163, 184, 0.18);
        border-radius: 50%;
        color: #e2e8f0;
        font-size: 24px;
        line-height: 1;
        cursor: pointer;
        transition: background 0.18s ease, transform 0.18s ease;
    }

    #close-map:hover {
        background: rgba(148, 163, 184, 0.2);
        transform: rotate(4deg);
    }

    #map-stage {
        position: relative;
        flex: 1;
        min-height: 0;
    }

    #map {
        position: absolute;
        inset: 0;
    }

    #map-legend {
        position: absolute;
        left: 18px;
        bottom: 18px;
        z-index: 2;
        display: flex;
        gap: 1rem;
        padding: 0.62rem 0.8rem;
        border: 1px solid rgba(148, 163, 184, 0.24);
        border-radius: 999px;
        background: rgba(15, 23, 42, 0.86);
        box-shadow: 0 10px 30px rgba(2, 6, 23, 0.35);
        color: #e2e8f0;
        font-size: 0.75rem;
        backdrop-filter: blur(8px);
        pointer-events: none;
    }

    .legend-item {
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    .legend-dot {
        width: 9px;
        height: 9px;
        border-radius: 50%;
        background: #fb7185;
        box-shadow: 0 0 0 3px rgba(251, 113, 133, 0.22);
    }

    .legend-dot.nearest {
        background: #f8fafc;
        box-shadow: 0 0 0 3px rgba(56, 189, 248, 0.35);
    }

    .legend-line {
        width: 18px;
        height: 3px;
        border-radius: 99px;
        background: #38bdf8;
    }

    .notice-map-marker {
        width: 18px;
        height: 18px;
        border: 3px solid #f8fafc;
        border-radius: 50%;
        cursor: pointer;
    }

    .notice-map-marker--point {
        background: #fb7185;
        box-shadow: 0 0 0 6px rgba(251, 113, 133, 0.2), 0 4px 16px rgba(2, 6, 23, 0.45);
    }

    .notice-map-marker--nearest {
        width: 14px;
        height: 14px;
        background: #38bdf8;
        box-shadow: 0 0 0 5px rgba(56, 189, 248, 0.22), 0 4px 16px rgba(2, 6, 23, 0.45);
    }

    @keyframes map-enter {
        from { opacity: 0; transform: translateY(12px) scale(0.985); }
        to { opacity: 1; transform: translateY(0) scale(1); }
    }

    @media (max-width: 640px) {
        #map-container {
            height: 88vh;
            min-height: 360px;
            border-radius: 16px;
        }

        #map-legend {
            left: 10px;
            bottom: 10px;
            gap: 0.65rem;
            max-width: calc(100% - 20px);
        }
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

    <!-- Geographic notice map -->
    <div id="map-modal" role="dialog" aria-modal="true" aria-labelledby="map-title" aria-hidden="true">
        <div id="map-container">
            <div id="map-header">
                <div>
                    <h3 id="map-title">Geographic notice</h3>
                    <p id="map-subtitle">Affected geometry</p>
                </div>
                <button id="close-map" type="button" aria-label="Close map">&times;</button>
            </div>
            <div id="map-stage">
                <div id="map"></div>
                <div id="map-legend" aria-hidden="true">
                    <span class="legend-item"><span class="legend-dot"></span>Affected stop</span>
                    <span class="legend-item"><span class="legend-dot nearest"></span>Closest point</span>
                    <span class="legend-item"><span class="legend-line"></span>Shape</span>
                </div>
            </div>
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
            out.push_str(
                "\n                        <ul>\n                            <li><b>website: </b>",
            );
            push_safe_link(out, &agency.url, false);
            out.push_str("</li>\n                            <li><b>phone number: </b>");
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
                out.push_str("                        ");
                push_safe_link(out, &value, true);
                out.push('\n');
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
    if let Some(value) = info.feed_version.as_ref() {
        entries.push(("Feed Version".to_string(), value.clone()));
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
        "Contactless EMV Support" => Some("Fares"),
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
        // Exact totals from the container's counters — these include notices
        // dropped by the per-group storage cap.
        let (errors, warnings, infos) = container.severity_counts();
        Self {
            total: errors + warnings + infos,
            errors,
            warnings,
            infos,
        }
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

fn render_notice_groups(out: &mut String, notices_container: &NoticeContainer) {
    let grouped = group_notices(notices_container);
    for severity in [
        HtmlSeverity::Error,
        HtmlSeverity::Warning,
        HtmlSeverity::Info,
    ] {
        if let Some(code_map) = grouped.get(&severity) {
            for (code, group) in code_map {
                let total = notices_container
                    .group_total(code, severity_from_html(severity))
                    .max(group.len());
                render_notice_group(out, severity, code, group, total);
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

fn severity_from_html(severity: HtmlSeverity) -> NoticeSeverity {
    match severity {
        HtmlSeverity::Error => NoticeSeverity::Error,
        HtmlSeverity::Warning => NoticeSeverity::Warning,
        HtmlSeverity::Info => NoticeSeverity::Info,
    }
}

fn render_notice_group(
    out: &mut String,
    severity: HtmlSeverity,
    code: &str,
    notices: &[&ValidationNotice],
    total: usize,
) {
    let fields = notice_fields(notices);
    let description = notices
        .first()
        .map(|notice| notice.message.as_str())
        .unwrap_or("");

    let has_map_data = notices.iter().any(|notice| notice.geometry.is_some());

    out.push_str("            <tr class=\"notice\">\n                <td style='position:relative; padding-left: 2rem;'>\n                    <span style='position:absolute; left: 0.75rem;'>+</span>\n                    <span class='notice-code'>");
    push_escaped(out, code);
    out.push_str("</span>\n                </td>\n                <td><span class=\"badge ");
    out.push_str(severity.css_class());
    out.push_str("-badge\">");
    out.push_str(severity.label());
    out.push_str("</span></td>\n                <td style='font-weight: 700;'>");
    write!(out, "{}", total).ok();
    out.push_str("</td>\n            </tr>\n            <tr class=\"description\">\n                <td colspan=\"3\">\n                    <div class=\"desc-content\">\n                        <h3>");
    push_escaped(out, code);
    out.push_str("</h3>\n                        <p style='font-size: 1.1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.75rem; margin-bottom: 1rem;'>");
    push_escaped(out, description);
    out.push_str("</p>\n                        <p>View the GTFS Guru guide for <a\n                                href=\"");
    out.push_str(NOTICE_DOC_BASE_URL);
    push_escaped(out, code);
    out.push_str("/\" target=\"_blank\" rel=\"noopener noreferrer\">");
    push_escaped(out, code);
    out.push_str("</a>.\n                        </p>\n");
    if total > NOTICE_ROW_LIMIT {
        out.push_str("                         <p>Only the first 50 of ");
        write!(out, "{}", total).ok();
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
    if let Some(geometry) = &notice.geometry {
        if let Ok(json) = serde_json::to_string(geometry) {
            let stop_name = notice
                .context
                .get("stopName")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or("Affected location");
            let distance = notice
                .context
                .get("geoDistanceToShape")
                .and_then(Value::as_f64)
                .map(|meters| format!("{meters:.1} m from shape"))
                .unwrap_or_else(|| notice.code.clone());

            out.push_str("                                    <td>");
            out.push_str("<button type=\"button\" class=\"view-map-btn\" data-geometry=\"");
            push_escaped(out, &json);
            out.push_str("\" data-map-title=\"");
            push_escaped(out, stop_name);
            out.push_str("\" data-map-subtitle=\"");
            push_escaped(out, &distance);
            out.push_str("\">");
            out.push_str(
                r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M20 10c0 5-8 12-8 12S4 15 4 10a8 8 0 1 1 16 0Z"></path><circle cx="12" cy="10" r="2.5"></circle></svg>View map</button>"#,
            );
            out.push_str("</td>\n");
            return;
        }
    }
    out.push_str("                                    <td>-</td>\n");
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
        ordered.retain(|field| !is_internal_geometry_field(field));
        dedup_fields(&mut ordered);
        return ordered;
    }

    let mut ordered: Vec<String> = union.into_iter().collect();
    ordered.retain(|field| !is_internal_geometry_field(field));
    ordered.sort();
    ordered
}

fn is_internal_geometry_field(field: &str) -> bool {
    matches!(field, "stopLocation" | "match" | "shapePath" | "matchIndex")
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

/// Emit an `<a href>` only for `http`/`https` URLs. Any other value (notably
/// `javascript:` / `data:` from an untrusted feed) is rendered as escaped text
/// so it cannot become a clickable, script-executing link in the report.
fn push_safe_link(out: &mut String, url: &str, target_blank: bool) {
    if is_http_url(url) {
        out.push_str("<a href=\"");
        push_escaped(out, url);
        if target_blank {
            out.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\">");
        } else {
            out.push_str("\">");
        }
        push_escaped(out, url);
        out.push_str("</a>");
    } else {
        push_escaped(out, url);
    }
}

fn is_http_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ReportSummaryContext;
    use gtfs_guru_core::{NoticeGeometry, NoticeGeometryPoint};

    #[test]
    fn geographic_notice_renders_a_maplibre_button_and_dialog() {
        let mut notices = NoticeContainer::new();
        let mut notice =
            ValidationNotice::new("geo_test", NoticeSeverity::Warning, "geographic test");
        notice.insert_context_field("stopName", "Harbour Stop");
        notice.insert_context_field("geoDistanceToShape", 42.5);
        notice.insert_context_field("stopLocation", [35.0, 33.0]);
        notice.insert_context_field("shapePath", [[35.0, 33.0], [35.1, 33.1]]);
        notice.geometry = Some(NoticeGeometry::PointAndLine {
            point: NoticeGeometryPoint::new(35.0, 33.0),
            line: vec![
                NoticeGeometryPoint::new(35.0, 33.0),
                NoticeGeometryPoint::new(35.1, 33.1),
            ],
            nearest_point: Some(NoticeGeometryPoint::new(35.02, 33.02)),
        });
        notices.push(notice);

        let summary = ReportSummary::from_context(ReportSummaryContext::new());
        let html = generate_html_report_string(
            &notices,
            &summary,
            HtmlReportContext::from_summary(&summary, "test.zip"),
        );

        assert!(html.contains("maplibre-gl@5.12.0"));
        assert!(html.contains("class=\"view-map-btn\""));
        assert!(html.contains("data-geometry=\"{&quot;type&quot;:&quot;pointAndLine&quot;"));
        assert!(html.contains("id=\"map-modal\" role=\"dialog\""));
        assert!(!html.contains("leaflet@"));
        assert!(!html.contains("<span>shapePath</span>"));
        assert!(!html.contains("<span>stopLocation</span>"));
    }

}
