//! A read-only MCP adapter around GTFS Guru.
//!
//! The server supports local stdio and authenticated Streamable HTTP
//! transports. Local file access is restricted to configured roots. URL
//! fetching is opt-in because an agent-controlled URL is a security boundary.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use chrono::{NaiveDate, Utc};
use gtfs_guru_core::notice_schema::NoticeSchemaSeverity;
use gtfs_guru_core::{
    build_notice_schema_map, default_runner, set_google_rules_enabled, set_notice_group_limit,
    set_thorough_mode_enabled, set_validation_country_code, set_validation_date, validate_bytes,
    validate_input, GtfsInput, NoticeContainer, NoticeSeverity, ValidationNotice,
};
use gtfs_guru_profile::{FeedExplanation, FeedProfile, ValidationOverview};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, Json, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

const DEFAULT_MAX_DOWNLOAD_BYTES: usize = 512 * 1024 * 1024;
const DEFAULT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(90);
const DEFAULT_MAX_CONCURRENT_VALIDATIONS: usize = 4;
const DEFAULT_NOTICE_SAMPLES_PER_GROUP: usize = 100;
const DEFAULT_NOTICE_EXAMPLES_PER_GROUP: usize = 3;
/// Bounds for the `list_gtfs_feeds` walk. An allowed root is operator-chosen
/// but can still be a huge tree, and the result has to fit in a model's context.
const MAX_SCAN_DEPTH: usize = 4;
const MAX_SCANNED_ENTRIES: usize = 20_000;
const MAX_LISTED_FEEDS: usize = 200;

/// Files that mark a directory as an unzipped feed rather than a container of
/// feeds. `agency.txt` and `stops.txt` are required by the GTFS specification.
const GTFS_MARKER_FILES: [&str; 2] = ["agency.txt", "stops.txt"];

fn is_gtfs_directory(path: &Path) -> bool {
    GTFS_MARKER_FILES
        .iter()
        .all(|marker| path.join(marker).is_file())
}

#[derive(Debug, Clone)]
pub struct McpConfig {
    pub allowed_roots: Vec<PathBuf>,
    pub allow_urls: bool,
    pub max_download_bytes: usize,
    pub max_concurrent_validations: usize,
    pub notice_samples_per_group: usize,
    pub notice_examples_per_group: usize,
}

impl McpConfig {
    pub fn local(allowed_roots: Vec<PathBuf>) -> anyhow::Result<Self> {
        if allowed_roots.is_empty() {
            bail!("at least one allowed root is required");
        }
        let allowed_roots = allowed_roots
            .into_iter()
            .map(|root| {
                root.canonicalize()
                    .with_context(|| format!("resolve allowed root {}", root.display()))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            allowed_roots,
            allow_urls: false,
            max_download_bytes: DEFAULT_MAX_DOWNLOAD_BYTES,
            max_concurrent_validations: DEFAULT_MAX_CONCURRENT_VALIDATIONS,
            notice_samples_per_group: DEFAULT_NOTICE_SAMPLES_PER_GROUP,
            notice_examples_per_group: DEFAULT_NOTICE_EXAMPLES_PER_GROUP,
        })
    }
}

#[derive(Clone)]
pub struct GtfsGuruMcp {
    tool_router: ToolRouter<Self>,
    config: Arc<McpConfig>,
    validation_permits: Arc<tokio::sync::Semaphore>,
}

impl GtfsGuruMcp {
    pub fn new(config: McpConfig) -> Self {
        let max_concurrent_validations = config.max_concurrent_validations.max(1);
        Self {
            tool_router: Self::tool_router(),
            config: Arc::new(config),
            validation_permits: Arc::new(tokio::sync::Semaphore::new(max_concurrent_validations)),
        }
    }

    async fn analyze(&self, params: AnalyzeGtfsParams) -> Result<AnalyzedFeed, String> {
        let permit = self
            .validation_permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| format!("validation queue closed: {error}"))?;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            service.analyze_sync(params)
        })
        .await
        .map_err(|error| format!("validation task failed: {error}"))?
        .map_err(|error| error.to_string())
    }

    fn analyze_sync(&self, params: AnalyzeGtfsParams) -> anyhow::Result<AnalyzedFeed> {
        let analysis_date = parse_analysis_date(params.analysis_date.as_deref())?;
        let _date_guard = set_validation_date(Some(analysis_date));
        let _country_guard = params
            .country_code
            .as_ref()
            .map(|country| set_validation_country_code(Some(country.trim().to_uppercase())));
        let _google_guard = params.google_rules.then(|| set_google_rules_enabled(true));
        let _thorough_guard = params.thorough.then(|| set_thorough_mode_enabled(true));
        let stored_notice_limit = self
            .config
            .notice_samples_per_group
            .max(self.config.notice_examples_per_group);
        let _notice_limit_guard = set_notice_group_limit(Some(stored_notice_limit));
        let runner = default_runner();

        let outcome = if looks_like_url(&params.source) {
            if !self.config.allow_urls {
                bail!(
                    "URL validation is disabled; restart gtfs-guru-mcp with --allow-url to enable it"
                );
            }
            let bytes = download_public_feed(
                &params.source,
                self.config.max_download_bytes,
                DEFAULT_DOWNLOAD_TIMEOUT,
            )?;
            validate_bytes(&bytes, &runner)
        } else {
            let path = self.resolve_allowed_path(Path::new(&params.source))?;
            let input = GtfsInput::from_path(&path)
                .with_context(|| format!("open GTFS input {}", path.display()))?;
            validate_input(&input, &runner)
        };

        let validation = ValidationOverview::from_notices(&outcome.notices);
        let notice_examples =
            build_notice_examples(&outcome.notices, self.config.notice_examples_per_group);
        let profile = outcome
            .feed
            .as_ref()
            .map(|feed| FeedProfile::build(feed, &outcome.notices, analysis_date));
        Ok(AnalyzedFeed {
            source: params.source,
            analysis_date,
            profile,
            validation,
            notice_examples,
        })
    }

    /// Walk the allowed roots for things `validate_gtfs` can accept.
    ///
    /// A client is told nothing about the roots otherwise, so without this it
    /// can only reach a feed whose absolute path the user typed out. The walk
    /// is bounded on depth, visited entries, and returned entries: an allowed
    /// root is chosen by an operator, but nothing stops it being a home
    /// directory with a million files under it.
    fn scan_feeds(&self, name_contains: Option<&str>) -> ListFeedsResponse {
        let needle = name_contains
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);
        let matches_name = |name: &str| match needle.as_deref() {
            Some(needle) => name.to_ascii_lowercase().contains(needle),
            None => true,
        };

        // Paths only for now. Collecting every match before truncating is what
        // makes the cut predictable, and holding paths rather than built entries
        // keeps that affordable: metadata is read further down, once, for the
        // entries that actually survive.
        let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
        let mut visited = 0usize;
        let mut scan_limit_hit = false;

        'roots: for root in &self.config.allowed_roots {
            let mut stack = vec![(root.clone(), 0usize)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                // `read_dir` does not guarantee an order. Sorting here fixes the
                // order the walk *visits* entries in, which the global sort
                // below cannot: that one decides which matches survive the
                // result cap, but by then the entry budget may already have
                // stopped the walk. Together they make both cuts repeatable.
                let mut entries = entries.flatten().collect::<Vec<_>>();
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    visited += 1;
                    // This budget bounds the walk itself, so unlike the result
                    // cap it cannot be applied over the full match set: hitting
                    // it means part of the tree went unseen — the same part on
                    // every run, thanks to the sort above, but unseen. Abandon
                    // the remaining roots too; the budget is already spent.
                    if visited > MAX_SCANNED_ENTRIES {
                        scan_limit_hit = true;
                        break 'roots;
                    }
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    // Hidden entries are editor state and VCS metadata far more
                    // often than feeds, and a symlink is the one way a walk
                    // rooted inside the boundary can report a path outside it.
                    if name.starts_with('.') {
                        continue;
                    }
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    if file_type.is_symlink() {
                        continue;
                    }

                    if file_type.is_file() {
                        if name.to_ascii_lowercase().ends_with(".zip") && matches_name(&name) {
                            candidates.push((path, "zip"));
                        }
                    } else if file_type.is_dir() {
                        // An unzipped feed is a leaf: descending into it would
                        // report its own .txt files as if they were feeds. That
                        // holds whether or not the filter kept it.
                        if is_gtfs_directory(&path) {
                            if matches_name(&name) {
                                candidates.push((path, "directory"));
                            }
                        } else if depth < MAX_SCAN_DEPTH {
                            stack.push((path, depth + 1));
                        }
                    }
                }
            }
        }

        // Order first, truncate second. The other way round returns an arbitrary
        // subset in tidy order, which reads as a complete listing; this way the
        // cut is always the alphabetically-last entries, and repeats.
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        let total_matches = candidates.len();
        let truncated = scan_limit_hit || total_matches > MAX_LISTED_FEEDS;
        candidates.truncate(MAX_LISTED_FEEDS);

        let feeds: Vec<FeedEntry> = candidates
            .into_iter()
            .map(|(path, kind)| {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let size_bytes = match kind {
                    "zip" => std::fs::metadata(&path).ok().map(|meta| meta.len()),
                    _ => None,
                };
                FeedEntry {
                    path: path.to_string_lossy().into_owned(),
                    name,
                    kind: kind.to_string(),
                    size_bytes,
                }
            })
            .collect();

        ListFeedsResponse {
            roots: self
                .config
                .allowed_roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
            feed_count: feeds.len(),
            total_matches,
            feeds,
            truncated,
        }
    }

    fn resolve_allowed_path(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("resolve GTFS path {}", path.display()))?;
        if self
            .config
            .allowed_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            Ok(canonical)
        } else {
            bail!(
                "path {} is outside the MCP server's allowed roots",
                canonical.display()
            )
        }
    }
}

#[tool_router(router = tool_router)]
impl GtfsGuruMcp {
    #[tool(
        description = "Validate a local GTFS ZIP/directory or an explicitly enabled public URL. Returns deterministic feed facts, exact grouped validation totals, and bounded concrete notice examples with file/row/field context. This tool is read-only.",
        annotations(
            title = "Validate GTFS",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn validate_gtfs(
        &self,
        Parameters(params): Parameters<AnalyzeGtfsParams>,
    ) -> Result<Json<ValidateGtfsResponse>, String> {
        let analyzed = self.analyze(params).await?;
        Ok(Json(ValidateGtfsResponse::from(analyzed)))
    }

    #[tool(
        description = "Explain a GTFS feed in human-readable, evidence-backed terms. Returns the same deterministic profile plus bounded concrete notice examples with file/row/field context so claims can be checked. This tool is read-only.",
        annotations(
            title = "Explain GTFS",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    pub async fn explain_gtfs(
        &self,
        Parameters(params): Parameters<AnalyzeGtfsParams>,
    ) -> Result<Json<ExplainGtfsResponse>, String> {
        let analyzed = self.analyze(params).await?;
        let explanation = analyzed
            .profile
            .as_ref()
            .map(FeedExplanation::from_profile)
            .unwrap_or_else(|| {
                FeedExplanation::for_unreadable_feed(
                    analyzed.validation.clone(),
                    analyzed.analysis_date,
                )
            });
        Ok(Json(ExplainGtfsResponse {
            source: analyzed.source,
            profile: analyzed.profile,
            validation: analyzed.validation,
            notice_examples: analyzed.notice_examples,
            explanation,
        }))
    }

    #[tool(
        description = "List the GTFS feeds this server can read: every ZIP archive and unzipped feed directory under its allowed roots, with the absolute path to pass to validate_gtfs or explain_gtfs. Call this first when the user names a feed without giving its full path, passing that name as nameContains. Results are ordered by path; when totalMatches exceeds the returned count, truncated is true and the entries omitted are the ones sorting last. This tool is read-only.",
        annotations(
            title = "List GTFS feeds",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn list_gtfs_feeds(
        &self,
        Parameters(params): Parameters<ListFeedsParams>,
    ) -> Result<Json<ListFeedsResponse>, String> {
        Ok(Json(self.scan_feeds(params.name_contains.as_deref())))
    }

    #[tool(
        description = "Look up the canonical summary, detailed explanation, and specification references for a GTFS Guru validation notice code. This tool does not access a feed.",
        annotations(
            title = "Get GTFS notice details",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub fn get_notice_details(
        &self,
        Parameters(params): Parameters<NoticeDetailsParams>,
    ) -> Result<Json<NoticeDetailsResponse>, String> {
        let schema = notice_schemas()
            .get(params.code.trim())
            .ok_or_else(|| format!("unknown GTFS Guru notice code: {}", params.code))?;
        let references = schema.references.as_ref();
        Ok(Json(NoticeDetailsResponse {
            code: schema.code.clone(),
            severity: notice_schema_severity(schema.severity_level).to_string(),
            summary: schema.short_summary.clone().unwrap_or_default(),
            description: schema.description.clone().unwrap_or_default(),
            files: references
                .map(|value| value.file_references.clone())
                .unwrap_or_default(),
            best_practices_files: references
                .map(|value| value.best_practices_file_references.clone())
                .unwrap_or_default(),
            specification_sections: references
                .map(|value| value.section_references.clone())
                .unwrap_or_default(),
            links: references
                .map(|value| {
                    value
                        .url_references
                        .iter()
                        .map(|reference| NoticeReferenceLink {
                            label: reference.label.clone(),
                            url: reference.url.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GtfsGuruMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("gtfs-guru-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("GTFS Guru")
                    .with_description("Validation-focused MCP server for GTFS schedule feeds")
                    .with_website_url("https://gtfs.guru"),
            )
            .with_instructions(
                "Use list_gtfs_feeds to discover which feeds this server can read and their absolute paths; call it before searching the filesystem yourself when the user names a feed without a full path. Use validate_gtfs for structured validation, explain_gtfs for a concise evidence-backed explanation, and get_notice_details before making claims about a notice. After summarizing the exact grouped totals, present concrete ERROR noticeExamples with their file, row, field, and context, then WARNING examples; omit INFO examples unless the user asks. Do not claim that a feed is guaranteed to be accepted by a downstream trip planner."
            )
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeGtfsParams {
    #[schemars(
        description = "Local GTFS ZIP/directory path under an allowed root, or a public HTTP(S) URL when URL access was explicitly enabled"
    )]
    pub source: String,
    #[schemars(
        description = "Analysis start date in YYYY-MM-DD; defaults to the current UTC date"
    )]
    pub analysis_date: Option<String>,
    #[schemars(description = "Optional ISO 3166-1 alpha-2 country code for region-specific rules")]
    pub country_code: Option<String>,
    #[serde(default)]
    #[schemars(description = "Enable additional Google-compatibility validation rules")]
    pub google_rules: bool,
    #[serde(default)]
    #[schemars(description = "Enable recommended-field and other thorough validation rules")]
    pub thorough: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidateGtfsResponse {
    pub source: String,
    pub profile: Option<FeedProfile>,
    pub validation: ValidationOverview,
    pub notice_examples: Vec<NoticeExample>,
}

impl From<AnalyzedFeed> for ValidateGtfsResponse {
    fn from(value: AnalyzedFeed) -> Self {
        Self {
            source: value.source,
            profile: value.profile,
            validation: value.validation,
            notice_examples: value.notice_examples,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExplainGtfsResponse {
    pub source: String,
    pub profile: Option<FeedProfile>,
    pub validation: ValidationOverview,
    pub notice_examples: Vec<NoticeExample>,
    pub explanation: FeedExplanation,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeExample {
    pub code: String,
    pub severity: String,
    pub total_occurrences: usize,
    pub message: String,
    pub file: Option<String>,
    pub row: Option<u64>,
    pub field: Option<String>,
    pub context: BTreeMap<String, Value>,
    pub fix_available: bool,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListFeedsParams {
    #[serde(default)]
    #[schemars(
        description = "Case-insensitive substring of the feed's file or directory name. Omit to list everything."
    )]
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListFeedsResponse {
    /// The directories this server may read. Paths outside them are rejected.
    pub roots: Vec<String>,
    /// How many entries `feeds` actually contains.
    pub feed_count: usize,
    /// How many entries matched before the result cap was applied. Greater than
    /// `feed_count` means the difference sorts after the last entry returned.
    pub total_matches: usize,
    pub feeds: Vec<FeedEntry>,
    /// True when the listing is partial: either more matched than are returned,
    /// or the walk ran out of its entry budget before seeing the whole tree.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeedEntry {
    /// Absolute path, ready to pass as `source` to validate_gtfs.
    pub path: String,
    pub name: String,
    /// `zip` or `directory`.
    pub kind: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct NoticeDetailsParams {
    #[schemars(description = "Exact GTFS Guru notice code, for example missing_required_field")]
    pub code: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeDetailsResponse {
    pub code: String,
    pub severity: String,
    pub summary: String,
    pub description: String,
    pub files: Vec<String>,
    pub best_practices_files: Vec<String>,
    pub specification_sections: Vec<String>,
    pub links: Vec<NoticeReferenceLink>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NoticeReferenceLink {
    pub label: String,
    pub url: String,
}

struct AnalyzedFeed {
    source: String,
    analysis_date: NaiveDate,
    profile: Option<FeedProfile>,
    validation: ValidationOverview,
    notice_examples: Vec<NoticeExample>,
}

fn build_notice_examples(
    notices: &NoticeContainer,
    examples_per_group: usize,
) -> Vec<NoticeExample> {
    if examples_per_group == 0 {
        return Vec::new();
    }

    // Decorate before sorting: the ordering key costs a schema lookup and up to
    // two String clones, and a comparator would pay that on every comparison.
    let schemas = notice_schemas();
    let mut stored = notices
        .iter()
        .map(|notice| {
            let inferred_file = inferred_notice_file(notice, schemas);
            let location = normalized_notice_location(notice, inferred_file);
            (notice, inferred_file, location)
        })
        .collect::<Vec<_>>();
    stored.sort_by(|(left, _, left_location), (right, _, right_location)| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left_location.cmp(right_location))
            .then_with(|| left.message.cmp(&right.message))
    });

    let mut emitted = HashMap::<(&str, NoticeSeverity), usize>::new();
    let mut examples = Vec::new();
    for (notice, inferred_file, _) in stored {
        let count = emitted
            .entry((notice.code.as_str(), notice.severity))
            .or_default();
        if *count >= examples_per_group {
            continue;
        }
        *count += 1;
        examples.push(NoticeExample::from_notice(
            notice,
            notices.group_total(&notice.code, notice.severity),
            inferred_file,
        ));
    }
    examples
}

/// The notice schema map is derived from static tables and never changes for a
/// build, but assembling it allocates a String per field. Every MCP request
/// touches it, so build it once.
fn notice_schemas() -> &'static BTreeMap<String, gtfs_guru_core::notice_schema::NoticeSchema> {
    static SCHEMAS: OnceLock<BTreeMap<String, gtfs_guru_core::notice_schema::NoticeSchema>> =
        OnceLock::new();
    SCHEMAS.get_or_init(build_notice_schema_map)
}

impl NoticeExample {
    fn from_notice(
        notice: &ValidationNotice,
        total_occurrences: usize,
        inferred_file: Option<&str>,
    ) -> Self {
        let (file, row, field) = normalized_notice_location(notice, inferred_file);
        let mut context = notice.context.clone();
        if file.is_some() {
            context.remove("filename");
        }
        if row.is_some() {
            context.remove("csvRowNumber");
        }
        if field.is_some() {
            context.remove("fieldName");
        }
        Self {
            code: notice.code.clone(),
            severity: validation_notice_severity(notice.severity).to_string(),
            total_occurrences,
            message: notice.message.clone(),
            file,
            row,
            field,
            context,
            fix_available: notice.fix.is_some(),
            suggested_fix: notice.fix.as_ref().map(|fix| fix.description.clone()),
        }
    }
}

fn inferred_notice_file<'a>(
    notice: &ValidationNotice,
    schemas: &'a BTreeMap<String, gtfs_guru_core::notice_schema::NoticeSchema>,
) -> Option<&'a str> {
    let files = schemas
        .get(&notice.code)?
        .references
        .as_ref()?
        .file_references
        .as_slice();
    match files {
        [file] => Some(file.as_str()),
        _ => None,
    }
}

fn normalized_notice_location(
    notice: &ValidationNotice,
    inferred_file: Option<&str>,
) -> (Option<String>, Option<u64>, Option<String>) {
    let file = notice
        .file
        .clone()
        .or_else(|| context_string(notice, "filename"))
        .or_else(|| inferred_file.map(str::to_string));
    let row = notice.row.or_else(|| context_u64(notice, "csvRowNumber"));
    let field = notice
        .field
        .clone()
        .or_else(|| context_string(notice, "fieldName"));
    (file, row, field)
}

fn context_string(notice: &ValidationNotice, key: &str) -> Option<String> {
    notice
        .context
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn context_u64(notice: &ValidationNotice, key: &str) -> Option<u64> {
    let value = notice.context.get(key)?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn validation_notice_severity(severity: NoticeSeverity) -> &'static str {
    match severity {
        NoticeSeverity::Error => "ERROR",
        NoticeSeverity::Warning => "WARNING",
        NoticeSeverity::Info => "INFO",
    }
}

fn parse_analysis_date(value: Option<&str>) -> anyhow::Result<NaiveDate> {
    match value {
        Some(value) => NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
            .with_context(|| format!("parse analysis date {value:?}; expected YYYY-MM-DD")),
        None => Ok(Utc::now().date_naive()),
    }
}

fn notice_schema_severity(severity: NoticeSchemaSeverity) -> &'static str {
    match severity {
        NoticeSchemaSeverity::Info => "INFO",
        NoticeSchemaSeverity::Warning => "WARNING",
        NoticeSchemaSeverity::Error => "ERROR",
    }
}

/// URL schemes are case-insensitive. Matching only lowercase sends
/// `HTTPS://host/feed.zip` down the local-path branch, where it fails with a
/// confusing "resolve GTFS path" error instead of the URL-access-disabled one.
fn looks_like_url(source: &str) -> bool {
    ["https://", "http://"].iter().any(|scheme| {
        source
            .get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
    })
}

fn download_public_feed(
    source: &str,
    max_bytes: usize,
    timeout: Duration,
) -> anyhow::Result<Vec<u8>> {
    let mut url = Url::parse(source).with_context(|| format!("parse GTFS URL {source}"))?;
    let deadline = Instant::now() + timeout;
    for redirects in 0..=5 {
        let addresses = resolve_public_http_url(&url)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("GTFS download exceeded the configured timeout");
        }
        let mut client_builder = reqwest::blocking::Client::builder()
            .user_agent(format!("gtfs-guru-mcp/{}", env!("CARGO_PKG_VERSION")))
            .timeout(remaining)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(domain) = url
            .host_str()
            .filter(|_| matches!(url.host(), Some(Host::Domain(_))))
        {
            client_builder = client_builder.resolve_to_addrs(domain, &addresses);
        }
        let client = client_builder.build().context("build GTFS HTTP client")?;
        let response = client
            .get(url.clone())
            .send()
            .with_context(|| format!("download GTFS feed from {url}"))?;

        if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            if redirects == 5 {
                bail!("GTFS download exceeded the five-redirect limit");
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("GTFS download redirect has no Location header")?
                .to_str()
                .context("GTFS download redirect has an invalid Location header")?;
            url = url
                .join(location)
                .with_context(|| format!("resolve GTFS redirect {location:?}"))?;
            continue;
        }

        let response = response
            .error_for_status()
            .with_context(|| format!("download GTFS feed from {url}"))?;
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            bail!("GTFS download exceeds the configured {max_bytes}-byte limit");
        }

        let mut bytes = Vec::new();
        response
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .context("read GTFS download")?;
        if bytes.len() > max_bytes {
            bail!("GTFS download exceeds the configured {max_bytes}-byte limit");
        }
        return Ok(bytes);
    }
    unreachable!("redirect loop always returns or fails")
}

fn resolve_public_http_url(url: &Url) -> anyhow::Result<Vec<SocketAddr>> {
    if !matches!(url.scheme(), "http" | "https") {
        bail!("only HTTP and HTTPS feed URLs are supported");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("feed URLs containing credentials are not accepted");
    }
    let host = url.host().context("feed URL has no host")?;
    let port = url
        .port_or_known_default()
        .context("feed URL has no port")?;
    match host {
        Host::Ipv4(address) => {
            ensure_public_ip(IpAddr::V4(address))?;
            Ok(vec![SocketAddr::new(IpAddr::V4(address), port)])
        }
        Host::Ipv6(address) => {
            ensure_public_ip(IpAddr::V6(address))?;
            Ok(vec![SocketAddr::new(IpAddr::V6(address), port)])
        }
        Host::Domain(domain) => {
            let addresses = (domain, port)
                .to_socket_addrs()
                .with_context(|| format!("resolve feed host {domain}"))?
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                bail!("feed host {domain} did not resolve");
            }
            for address in &addresses {
                ensure_public_ip(address.ip())?;
            }
            Ok(addresses)
        }
    }
}

fn ensure_public_ip(address: IpAddr) -> anyhow::Result<()> {
    let blocked = match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast()
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 198 && (18..=19).contains(&octets[1]))
                || octets[0] >= 240
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.segments()[0] == 0x2001 && address.segments()[1] == 0x0db8
        }
    };
    if blocked {
        bail!("feed URL resolves to a private, local, or reserved address: {address}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_paths_are_confined_to_allowed_roots() {
        let root = temp_dir("root");
        let outside = temp_dir("outside");
        fs::write(root.join("feed.zip"), b"not a zip").unwrap();
        fs::write(outside.join("feed.zip"), b"not a zip").unwrap();
        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());

        assert!(service.resolve_allowed_path(&root.join("feed.zip")).is_ok());
        assert!(service
            .resolve_allowed_path(&outside.join("feed.zip"))
            .is_err());

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn feed_listing_reports_archives_and_unzipped_feeds() {
        let root = temp_dir("listing");
        fs::write(root.join("berlin_vbb.zip"), b"not a zip").unwrap();
        fs::write(root.join("notes.txt"), b"ignored").unwrap();
        fs::write(root.join(".hidden.zip"), b"ignored").unwrap();

        // An unzipped feed is one entry, not a directory to descend into.
        let unzipped = root.join("unzipped");
        fs::create_dir_all(&unzipped).unwrap();
        fs::write(unzipped.join("agency.txt"), b"agency_id\n").unwrap();
        fs::write(unzipped.join("stops.txt"), b"stop_id\n").unwrap();

        // A plain directory is walked through to the feeds inside it.
        let nested = root.join("archive").join("2026");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("boston_mbta.zip"), b"not a zip").unwrap();

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let listing = service.scan_feeds(None);

        let names: Vec<&str> = listing.feeds.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"berlin_vbb.zip"));
        assert!(names.contains(&"boston_mbta.zip"));
        assert!(names.contains(&"unzipped"));
        assert!(!names.contains(&"notes.txt"));
        assert!(!names.contains(&".hidden.zip"));
        assert!(!names.contains(&"agency.txt"));
        assert_eq!(listing.feed_count, listing.feeds.len());
        assert!(!listing.truncated);
        assert_eq!(listing.roots.len(), 1);

        // Every reported path has to be usable as a `source` straight away.
        for feed in &listing.feeds {
            assert!(service.resolve_allowed_path(Path::new(&feed.path)).is_ok());
        }

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn feed_listing_is_ordered_and_skips_symlinks() {
        let root = temp_dir("listing-order");
        for name in ["c.zip", "a.zip", "b.zip"] {
            fs::write(root.join(name), b"not a zip").unwrap();
        }
        let outside = temp_dir("listing-outside");
        fs::write(outside.join("escaped.zip"), b"not a zip").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("escaped.zip"), root.join("link.zip")).unwrap();

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let listing = service.scan_feeds(None);

        let names: Vec<&str> = listing.feeds.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a.zip", "b.zip", "c.zip"]);

        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn truncated_feed_listing_selects_a_deterministic_prefix() {
        let root = temp_dir("listing-limit");
        // Created in reverse so the filesystem is unlikely to hand entries back
        // in the order asserted below.
        for index in (0..=MAX_LISTED_FEEDS).rev() {
            fs::write(root.join(format!("feed-{index:03}.zip")), b"not a zip").unwrap();
        }

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let listing = service.scan_feeds(None);

        assert!(listing.truncated);
        assert_eq!(listing.feed_count, MAX_LISTED_FEEDS);
        assert_eq!(listing.feeds.first().unwrap().name, "feed-000.zip");
        assert_eq!(listing.feeds.last().unwrap().name, "feed-199.zip");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn feed_listing_filters_by_name_case_insensitively() {
        let root = temp_dir("listing-filter");
        for name in ["berlin_vbb.zip", "boston_mbta.zip", "BERLIN_backup.zip"] {
            fs::write(root.join(name), b"not a zip").unwrap();
        }
        let unzipped = root.join("berlin_unzipped");
        fs::create_dir_all(&unzipped).unwrap();
        fs::write(unzipped.join("agency.txt"), b"agency_id\n").unwrap();
        fs::write(unzipped.join("stops.txt"), b"stop_id\n").unwrap();

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());

        let hit = service.scan_feeds(Some("berlin"));
        let names: Vec<&str> = hit.feeds.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["BERLIN_backup.zip", "berlin_unzipped", "berlin_vbb.zip"]
        );
        assert_eq!(hit.total_matches, 3);
        assert!(!hit.truncated);

        // An empty or whitespace-only filter is the same as no filter, so a
        // client that always sends the field does not get an empty listing.
        assert_eq!(service.scan_feeds(Some("   ")).feed_count, 4);
        assert_eq!(service.scan_feeds(None).feed_count, 4);
        assert_eq!(service.scan_feeds(Some("nothing-matches")).feed_count, 0);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unzipped_feed_requires_both_marker_files() {
        let root = temp_dir("markers");
        let unzipped = root.join("partial");
        fs::create_dir_all(&unzipped).unwrap();
        fs::write(unzipped.join("agency.txt"), b"agency_id\n").unwrap();

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        assert_eq!(service.scan_feeds(None).feed_count, 0);

        fs::write(unzipped.join("stops.txt"), b"stop_id\n").unwrap();
        assert_eq!(service.scan_feeds(None).feed_count, 1);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn feed_listing_truncates_the_entries_that_sort_last() {
        let root = temp_dir("listing-truncate");
        let extra = 5;
        // Zero-padded so lexical order matches numeric order.
        for index in 0..MAX_LISTED_FEEDS + extra {
            fs::write(root.join(format!("feed_{index:04}.zip")), b"not a zip").unwrap();
        }

        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let listing = service.scan_feeds(None);

        assert!(listing.truncated);
        assert_eq!(listing.total_matches, MAX_LISTED_FEEDS + extra);
        assert_eq!(listing.feed_count, MAX_LISTED_FEEDS);
        // The cut is the tail of the sorted list, not an arbitrary subset: the
        // walk order the filesystem chose must not decide who survives.
        assert_eq!(listing.feeds.first().unwrap().name, "feed_0000.zip");
        assert_eq!(listing.feeds.last().unwrap().name, "feed_0199.zip");

        // The filter narrows before the cap, so a targeted lookup reaches an
        // entry that a bare listing would have dropped.
        let hit = service.scan_feeds(Some("feed_0203"));
        assert!(!hit.truncated);
        assert_eq!(hit.feed_count, 1);
        assert_eq!(hit.feeds[0].name, "feed_0203.zip");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn url_detection_ignores_scheme_case() {
        assert!(looks_like_url("https://example.com/feed.zip"));
        assert!(looks_like_url("HTTPS://example.com/feed.zip"));
        assert!(looks_like_url("Http://example.com/feed.zip"));
        assert!(!looks_like_url("/srv/feeds/feed.zip"));
        assert!(!looks_like_url("http"));
    }

    #[test]
    fn private_and_credentialed_urls_are_rejected() {
        assert!(
            resolve_public_http_url(&Url::parse("http://127.0.0.1/feed.zip").unwrap()).is_err()
        );
        assert!(resolve_public_http_url(
            &Url::parse("http://user:secret@example.com/feed.zip").unwrap()
        )
        .is_err());
    }

    #[test]
    fn exposes_the_validation_focused_tool_surface() {
        let root = temp_dir("tools");
        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let names = service
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "explain_gtfs",
                "get_notice_details",
                "list_gtfs_feeds",
                "validate_gtfs"
            ]
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn notice_examples_are_bounded_and_include_locations() {
        let mut notices = NoticeContainer::with_group_limit(None);
        for row in [9, 3, 6] {
            let mut notice = ValidationNotice::new(
                "missing_required_field",
                NoticeSeverity::Error,
                "Missing required stop_name",
            )
            .with_location("stops.txt", "stop_name", row);
            notice.insert_context_field("stopId", format!("stop-{row}"));
            notices.push(notice);
        }
        notices.push(
            ValidationNotice::new(
                "route_color_contrast",
                NoticeSeverity::Warning,
                "Route colors do not have enough contrast",
            )
            .with_location("routes.txt", "route_color", 4),
        );
        let mut contextual_notice = ValidationNotice::new(
            "missing_stop_name",
            NoticeSeverity::Error,
            "stop_name is required for stop locations",
        );
        contextual_notice.insert_context_field("csvRowNumber", 11_u64);
        contextual_notice.insert_context_field("stopId", "contextual-stop");
        notices.push(contextual_notice);

        let examples = build_notice_examples(&notices, 2);

        assert_eq!(examples.len(), 4);
        assert_eq!(examples[0].severity, "ERROR");
        assert_eq!(examples[0].row, Some(3));
        assert_eq!(examples[0].total_occurrences, 3);
        assert_eq!(examples[0].context["stopId"], "stop-3");
        assert_eq!(examples[1].row, Some(6));
        assert_eq!(examples[2].file.as_deref(), Some("stops.txt"));
        assert_eq!(examples[2].row, Some(11));
        assert_eq!(examples[2].field, None);
        assert!(!examples[2].context.contains_key("csvRowNumber"));
        assert_eq!(examples[2].context["stopId"], "contextual-stop");
        assert_eq!(examples[3].severity, "WARNING");
        assert_eq!(examples[3].file.as_deref(), Some("routes.txt"));
        let json = serde_json::to_value(&examples[0]).unwrap();
        assert_eq!(json["totalOccurrences"], 3);
        assert_eq!(json["file"], "stops.txt");
        assert_eq!(json["field"], "stop_name");
    }

    #[tokio::test]
    async fn validates_a_minimal_feed_end_to_end() {
        let root = temp_dir("feed");
        write_minimal_feed(&root);
        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());
        let response = service
            .validate_gtfs(Parameters(AnalyzeGtfsParams {
                source: root.display().to_string(),
                analysis_date: Some("2026-07-27".to_string()),
                country_code: Some("CY".to_string()),
                google_rules: false,
                thorough: false,
            }))
            .await
            .unwrap()
            .0;

        let profile = response.profile.expect("profile");
        assert_eq!(profile.counts.routes, 1);
        assert_eq!(profile.counts.trips, 1);
        assert_eq!(profile.service.days[0].trips, 1);
        fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn returns_actionable_notice_examples_end_to_end() {
        let root = temp_dir("notice-examples");
        write_minimal_feed(&root);
        fs::write(
            root.join("stops.txt"),
            "stop_id,stop_name,stop_lat,stop_lon\nstop,,35.0,33.0\n",
        )
        .unwrap();
        let service = GtfsGuruMcp::new(McpConfig::local(vec![root.clone()]).unwrap());

        let response = service
            .validate_gtfs(Parameters(AnalyzeGtfsParams {
                source: root.display().to_string(),
                analysis_date: Some("2026-07-27".to_string()),
                country_code: Some("CY".to_string()),
                google_rules: false,
                thorough: false,
            }))
            .await
            .unwrap()
            .0;

        let example = response
            .notice_examples
            .iter()
            .find(|example| example.code == "missing_stop_name")
            .expect("missing_stop_name example");
        assert_eq!(example.severity, "ERROR");
        assert_eq!(example.total_occurrences, 1);
        assert_eq!(example.file.as_deref(), Some("stops.txt"));
        assert_eq!(example.row, Some(2));
        assert_eq!(example.context["stopId"], "stop");
        fs::remove_dir_all(root).ok();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "gtfs-guru-mcp-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_minimal_feed(path: &Path) {
        fs::write(
            path.join("agency.txt"),
            "agency_id,agency_name,agency_url,agency_timezone\nagency,Example,https://example.com,Europe/Nicosia\n",
        )
        .unwrap();
        fs::write(
            path.join("stops.txt"),
            "stop_id,stop_name,stop_lat,stop_lon\nstop,Example Stop,35.0,33.0\n",
        )
        .unwrap();
        fs::write(
            path.join("routes.txt"),
            "route_id,agency_id,route_short_name,route_type\nroute,agency,1,3\n",
        )
        .unwrap();
        fs::write(
            path.join("trips.txt"),
            "route_id,service_id,trip_id\nroute,weekday,trip\n",
        )
        .unwrap();
        fs::write(
            path.join("stop_times.txt"),
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\ntrip,06:00:00,06:00:00,stop,1\n",
        )
        .unwrap();
        fs::write(
            path.join("calendar.txt"),
            "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nweekday,1,1,1,1,1,0,0,20260701,20260831\n",
        )
        .unwrap();
    }
}
