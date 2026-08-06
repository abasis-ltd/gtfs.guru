//! Applying auto-fixes to a GTFS feed.
//!
//! Rules attach a [`Fix`](crate::Fix) to notices whose repair is unambiguous.
//! [`FixPlan`] turns those into a concrete edit list and [`apply_fixes`] writes
//! a repaired copy of the feed.
//!
//! Two properties matter here:
//!
//! * **The input is never modified.** `apply_fixes` writes to a separate zip or
//!   directory and refuses to run when the output resolves to the input path or
//!   to something that already exists.
//! * **The rewrite is byte-surgical.** Field fixes re-serialize only their CSV
//!   records, row deletes remove only the guarded record, and sorting moves the
//!   original raw records. Every other byte of every other file is copied
//!   verbatim. Line endings, quoting, column order, and a UTF-8 BOM survive.
//!   Untouched fields *inside* a field-edited row may lose redundant quoting,
//!   since that row is re-serialized as a whole.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::input::{GtfsInput, GtfsInputError, GtfsInputSource};
use crate::notice::{FixOperation, FixSafety};
use crate::{NoticeContainer, ValidationNotice};

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// One planned edit, resolved to a concrete `(file, row, field)` target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEdit {
    pub notice_code: String,
    pub description: String,
    pub safety: FixSafety,
    pub file: String,
    /// Physical line number in the CSV file (the header is line 1).
    pub row: u64,
    pub field: String,
    pub original: String,
    pub replacement: String,
    /// The concrete operation. The scalar fields above remain available for
    /// callers that render the long-standing field-replacement plan shape.
    pub operation: FixOperation,
}

/// How many fixes each safety level contributes, across the whole notice set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FixCounts {
    pub safe: usize,
    pub requires_confirmation: usize,
    pub unsafe_: usize,
}

impl FixCounts {
    pub fn total(&self) -> usize {
        self.safe + self.requires_confirmation + self.unsafe_
    }
}

/// The set of edits selected from a validation run at a given safety ceiling.
#[derive(Debug, Clone, Default)]
pub struct FixPlan {
    edits: Vec<PlannedEdit>,
    skipped: Vec<PlannedEdit>,
}

impl FixPlan {
    /// Collect every fixable notice, keeping those at or below `max_safety` and
    /// recording the rest as skipped.
    pub fn from_notices(notices: &NoticeContainer, max_safety: FixSafety) -> Self {
        let mut plan = FixPlan::default();
        for notice in notices.iter() {
            plan.push_notice(notice, max_safety);
        }
        normalize_edits(&mut plan.edits);
        normalize_edits(&mut plan.skipped);
        // Deterministic order so dry-run output and applied output agree.
        let sort_key = |edit: &PlannedEdit| {
            (
                edit.file.clone(),
                edit.row,
                operation_rank(&edit.operation),
                edit.field.clone(),
            )
        };
        plan.edits.sort_by_key(sort_key);
        plan.skipped.sort_by_key(sort_key);
        plan
    }

    fn push_notice(&mut self, notice: &ValidationNotice, max_safety: FixSafety) {
        let Some(fix) = notice
            .fix
            .clone()
            .or_else(|| crate::fix_suggest::structural_fix(notice))
        else {
            return;
        };

        let (file, row, field, original, replacement) = match &fix.operation {
            FixOperation::ReplaceField {
                file,
                row,
                field,
                original,
                replacement,
            } => (
                file.clone(),
                *row,
                field.clone(),
                original.clone(),
                replacement.clone(),
            ),
            FixOperation::DeleteRow {
                file,
                row,
                field,
                expected,
            } => (
                file.clone(),
                *row,
                field.clone(),
                expected.clone(),
                String::new(),
            ),
            FixOperation::SortStopTimes { file } => (
                file.clone(),
                0,
                "trip_id, stop_sequence".into(),
                String::new(),
                String::new(),
            ),
        };

        let edit = PlannedEdit {
            notice_code: notice.code.clone(),
            description: fix.description,
            safety: fix.safety,
            file,
            row,
            field,
            original,
            replacement,
            operation: fix.operation,
        };

        if fix.safety.allowed_by(max_safety) {
            self.edits.push(edit);
        } else {
            self.skipped.push(edit);
        }
    }

    pub fn edits(&self) -> &[PlannedEdit] {
        &self.edits
    }

    /// Edits that exist but sit above the requested safety ceiling.
    pub fn skipped(&self) -> &[PlannedEdit] {
        &self.skipped
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Selected edits grouped by file, in file/row/field order.
    pub fn by_file(&self) -> BTreeMap<&str, Vec<&PlannedEdit>> {
        let mut grouped: BTreeMap<&str, Vec<&PlannedEdit>> = BTreeMap::new();
        for edit in &self.edits {
            grouped.entry(edit.file.as_str()).or_default().push(edit);
        }
        grouped
    }

    /// Counts across selected *and* skipped edits, i.e. everything the run
    /// found that could be fixed at some safety level.
    pub fn counts(&self) -> FixCounts {
        let mut counts = FixCounts::default();
        for edit in self.edits.iter().chain(self.skipped.iter()) {
            match edit.safety {
                FixSafety::Safe => counts.safe += 1,
                FixSafety::RequiresConfirmation => counts.requires_confirmation += 1,
                FixSafety::Unsafe => counts.unsafe_ += 1,
            }
        }
        counts
    }
}

fn operation_rank(operation: &FixOperation) -> u8 {
    match operation {
        FixOperation::SortStopTimes { .. } => 0,
        FixOperation::DeleteRow { .. } => 1,
        FixOperation::ReplaceField { .. } => 2,
    }
}

/// Collapse repeated whole-file and row-delete suggestions. When a selected
/// plan deletes a row, field replacements for that same row are redundant and
/// are omitted from both the preview and the applied count.
fn normalize_edits(edits: &mut Vec<PlannedEdit>) {
    let deleted_rows: HashSet<(String, u64)> = edits
        .iter()
        .filter_map(|edit| match &edit.operation {
            FixOperation::DeleteRow { file, row, .. } => Some((file.clone(), *row)),
            _ => None,
        })
        .collect();

    edits.retain(|edit| match &edit.operation {
        FixOperation::ReplaceField { file, row, .. } => {
            !deleted_rows.contains(&(file.clone(), *row))
        }
        _ => true,
    });

    let mut seen_sorts = HashSet::new();
    let mut seen_deletes = HashSet::new();
    edits.retain(|edit| match &edit.operation {
        FixOperation::SortStopTimes { file } => seen_sorts.insert(file.clone()),
        FixOperation::DeleteRow { file, row, .. } => seen_deletes.insert((file.clone(), *row)),
        FixOperation::ReplaceField { .. } => true,
    });
}

/// Why a planned edit could not be applied to the file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictReason {
    /// The file named by the fix is not in the feed.
    FileMissing,
    /// No CSV record starts on that line.
    RowNotFound,
    /// The file has no such column.
    FieldNotFound,
    /// The field no longer holds the value the fix was computed from.
    ValueMismatch { found: String },
    /// Two fixes target the same field with different replacements.
    ConflictingEdits { other: String },
    /// The file cannot be safely reordered without guessing.
    CannotSort { message: String },
}

impl std::fmt::Display for ConflictReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictReason::FileMissing => write!(f, "file is not present in the feed"),
            ConflictReason::RowNotFound => write!(f, "no CSV record starts on that line"),
            ConflictReason::FieldNotFound => write!(f, "column not found in the header"),
            ConflictReason::ValueMismatch { found } => {
                write!(
                    f,
                    "field now holds {found:?}, not the value the fix expected"
                )
            }
            ConflictReason::ConflictingEdits { other } => {
                write!(f, "another fix wants to write {other:?} to the same field")
            }
            ConflictReason::CannotSort { message } => write!(f, "cannot sort rows: {message}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FixConflict {
    pub edit: PlannedEdit,
    pub reason: ConflictReason,
}

/// What [`apply_fixes`] actually did.
#[derive(Debug)]
pub struct FixOutcome {
    /// Path of the repaired feed.
    pub output: PathBuf,
    pub applied: Vec<PlannedEdit>,
    pub conflicts: Vec<FixConflict>,
    /// Names of the files whose bytes changed.
    pub rewritten_files: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FixError {
    #[error("refusing to write fixes over the input feed: {0}")]
    OutputSameAsInput(PathBuf),
    #[error("output path already exists: {0}")]
    OutputExists(PathBuf),
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("zip error for {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error(transparent)]
    Input(#[from] GtfsInputError),
    #[error("cannot rewrite {file}: {message}")]
    Csv { file: String, message: String },
}

fn io_err(path: &Path, source: std::io::Error) -> FixError {
    FixError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn zip_err(path: &Path, source: zip::result::ZipError) -> FixError {
    FixError::Zip {
        path: path.to_path_buf(),
        source,
    }
}

/// Write a copy of `input` with `plan`'s edits applied to `output`.
///
/// The input is left untouched. `output` must not exist and must not resolve to
/// the input path. A zip input produces a zip; a directory input produces a
/// directory.
pub fn apply_fixes(
    input: &GtfsInput,
    plan: &FixPlan,
    output: &Path,
) -> Result<FixOutcome, FixError> {
    if resolves_to_same_path(input.path(), output) {
        return Err(FixError::OutputSameAsInput(output.to_path_buf()));
    }
    if output.exists() {
        return Err(FixError::OutputExists(output.to_path_buf()));
    }

    match input.source() {
        GtfsInputSource::Zip => apply_to_zip(input, plan, output),
        GtfsInputSource::Directory => apply_to_directory(input, plan, output),
    }
}

/// Compare paths without requiring `output` to exist: canonicalize the deepest
/// existing ancestor of each and compare the remainder textually.
fn resolves_to_same_path(input: &Path, output: &Path) -> bool {
    fn resolve(path: &Path) -> PathBuf {
        let mut suffix = Vec::new();
        let mut current = path;
        loop {
            if let Ok(canonical) = current.canonicalize() {
                let mut resolved = canonical;
                for part in suffix.iter().rev() {
                    resolved.push(part);
                }
                return resolved;
            }
            match (current.parent(), current.file_name()) {
                (Some(parent), Some(name)) => {
                    suffix.push(name.to_os_string());
                    current = parent;
                }
                _ => return path.to_path_buf(),
            }
        }
    }

    resolve(input) == resolve(output)
}

/// Rewrite every touched file up front, so a plan that cannot be applied fails
/// before any output is created.
fn rewrite_touched_files(
    input: &GtfsInput,
    plan: &FixPlan,
) -> Result<(HashMap<String, Vec<u8>>, Vec<PlannedEdit>, Vec<FixConflict>), FixError> {
    let reader = input.reader();
    let mut rewritten = HashMap::new();
    let mut applied = Vec::new();
    let mut conflicts = Vec::new();

    for (file, edits) in plan.by_file() {
        let data = match reader.read_file(file) {
            Ok(data) => data,
            Err(GtfsInputError::MissingFile(_)) => {
                conflicts.extend(edits.into_iter().map(|edit| FixConflict {
                    edit: edit.clone(),
                    reason: ConflictReason::FileMissing,
                }));
                continue;
            }
            Err(err) => return Err(err.into()),
        };

        let result = rewrite_csv(file, &data, &edits)?;
        conflicts.extend(result.conflicts);
        if !result.applied.is_empty() {
            applied.extend(result.applied);
            rewritten.insert(file.to_string(), result.bytes);
        }
    }

    Ok((rewritten, applied, conflicts))
}

fn apply_to_zip(input: &GtfsInput, plan: &FixPlan, output: &Path) -> Result<FixOutcome, FixError> {
    let (rewritten, applied, conflicts) = rewrite_touched_files(input, plan)?;

    let source = File::open(input.path()).map_err(|err| io_err(input.path(), err))?;
    let mut archive = ZipArchive::new(source).map_err(|err| zip_err(input.path(), err))?;

    // Map each fix's file name onto the archive member it actually resolves to,
    // matching how the reader picks a member.
    let member_names: Vec<String> = (0..archive.len())
        .map(|index| {
            archive
                .by_index_raw(index)
                .map(|member| member.name().to_string())
                .map_err(|err| zip_err(input.path(), err))
        })
        .collect::<Result<_, _>>()?;
    let mut by_member: HashMap<String, Vec<u8>> = HashMap::new();
    for (file, bytes) in rewritten {
        match resolve_member_name(&member_names, &file) {
            Some(member) => {
                by_member.insert(member, bytes);
            }
            // The reader found the file but no root-level member matches it,
            // so there is nothing to replace; leave the archive as-is.
            None => continue,
        }
    }

    let out_file = File::create(output).map_err(|err| io_err(output, err))?;
    let mut writer = ZipWriter::new(out_file);
    let mut rewritten_files = Vec::new();

    for index in 0..archive.len() {
        let member = archive
            .by_index_raw(index)
            .map_err(|err| zip_err(input.path(), err))?;
        let name = member.name().to_string();
        let is_dir = member.is_dir();
        // Carry the original metadata over, so a rewritten member differs from
        // its source only in content.
        let mut options = FileOptions::default().last_modified_time(member.last_modified());
        if let Some(mode) = member.unix_mode() {
            options = options.unix_permissions(mode);
        }

        if is_dir {
            drop(member);
            writer
                .add_directory(&name, options)
                .map_err(|err| zip_err(output, err))?;
            continue;
        }

        match by_member.get(&name) {
            Some(bytes) => {
                drop(member);
                let options = options.compression_method(CompressionMethod::Deflated);
                writer
                    .start_file(&name, options)
                    .map_err(|err| zip_err(output, err))?;
                writer.write_all(bytes).map_err(|err| io_err(output, err))?;
                rewritten_files.push(name);
            }
            None => {
                // Copy still-compressed bytes: no re-deflate, no inflate budget.
                writer
                    .raw_copy_file(member)
                    .map_err(|err| zip_err(output, err))?;
            }
        }
    }

    writer.finish().map_err(|err| zip_err(output, err))?;
    rewritten_files.sort();

    Ok(FixOutcome {
        output: output.to_path_buf(),
        applied,
        conflicts,
        rewritten_files,
    })
}

/// Pick the member a GTFS file name resolves to, mirroring the zip reader:
/// an exact name wins, otherwise the lexicographically smallest case-insensitive
/// match among root-level members.
fn resolve_member_name(members: &[String], file: &str) -> Option<String> {
    if members.iter().any(|name| name == file) {
        return Some(file.to_string());
    }
    let target = file.to_ascii_lowercase();
    members
        .iter()
        .filter(|name| !(name.contains('/') || name.contains('\\')))
        .filter(|name| name.to_ascii_lowercase() == target)
        .min_by_key(|name| name.to_ascii_lowercase())
        .cloned()
}

fn apply_to_directory(
    input: &GtfsInput,
    plan: &FixPlan,
    output: &Path,
) -> Result<FixOutcome, FixError> {
    let (rewritten, applied, conflicts) = rewrite_touched_files(input, plan)?;

    std::fs::create_dir_all(output).map_err(|err| io_err(output, err))?;
    let mut rewritten_files = Vec::new();
    copy_tree(
        input.path(),
        input.path(),
        output,
        &rewritten,
        &mut rewritten_files,
    )?;
    rewritten_files.sort();

    Ok(FixOutcome {
        output: output.to_path_buf(),
        applied,
        conflicts,
        rewritten_files,
    })
}

fn copy_tree(
    root: &Path,
    current: &Path,
    destination: &Path,
    rewritten: &HashMap<String, Vec<u8>>,
    rewritten_files: &mut Vec<String>,
) -> Result<(), FixError> {
    for entry in std::fs::read_dir(current).map_err(|err| io_err(current, err))? {
        let entry = entry.map_err(|err| io_err(current, err))?;
        let source = entry.path();
        let relative = source.strip_prefix(root).unwrap_or(&source);
        let target = destination.join(relative);

        if source.is_dir() {
            std::fs::create_dir_all(&target).map_err(|err| io_err(&target, err))?;
            copy_tree(root, &source, destination, rewritten, rewritten_files)?;
            continue;
        }

        // Only root-level files can carry fixes, matching the directory reader.
        let name = relative.to_string_lossy();
        match rewritten.get(name.as_ref()) {
            Some(bytes) => {
                std::fs::write(&target, bytes).map_err(|err| io_err(&target, err))?;
                rewritten_files.push(name.into_owned());
            }
            None => {
                std::fs::copy(&source, &target).map_err(|err| io_err(&target, err))?;
            }
        }
    }
    Ok(())
}

struct RewriteResult {
    bytes: Vec<u8>,
    applied: Vec<PlannedEdit>,
    conflicts: Vec<FixConflict>,
}

/// Apply field replacements, row deletions, and an optional canonical sort.
fn rewrite_csv(file: &str, data: &[u8], edits: &[&PlannedEdit]) -> Result<RewriteResult, FixError> {
    let bom_len = if data.starts_with(&UTF8_BOM) {
        UTF8_BOM.len()
    } else {
        0
    };
    let body = &data[bom_len..];

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::None)
        .from_reader(body);

    let headers = reader
        .byte_headers()
        .map_err(|err| FixError::Csv {
            file: file.to_string(),
            message: err.to_string(),
        })?
        .clone();

    // First column wins on a duplicated header, matching deserialization.
    let mut exact: HashMap<String, usize> = HashMap::new();
    let mut lowercase: HashMap<String, usize> = HashMap::new();
    for (index, header) in headers.iter().enumerate() {
        let name = String::from_utf8_lossy(header).trim().to_string();
        lowercase.entry(name.to_ascii_lowercase()).or_insert(index);
        exact.entry(name).or_insert(index);
    }

    // Record start offsets for every record, including malformed ones, so a
    // preceding record can never absorb bytes from a row we could not parse.
    let mut starts: Vec<(u64, usize)> = Vec::new();
    let mut records: HashMap<u64, csv::ByteRecord> = HashMap::new();
    for result in reader.into_byte_records() {
        match result {
            Ok(record) => {
                if let Some(position) = record.position() {
                    let row = position.record() + 1;
                    starts.push((row, position.byte() as usize));
                    records.insert(row, record.clone());
                }
            }
            Err(err) => {
                if let Some(position) = err.position() {
                    starts.push((position.record() + 1, position.byte() as usize));
                }
            }
        }
    }
    starts.sort_by_key(|(_, byte)| *byte);

    // A chunk owns one record plus the exact terminators and blank lines after
    // it. Reordering chunks therefore preserves raw CSV bytes.
    let mut content_starts = Vec::with_capacity(starts.len());
    let mut content_ends = Vec::with_capacity(starts.len());
    for (index, (_, raw_start)) in starts.iter().enumerate() {
        let raw_end = starts
            .get(index + 1)
            .map(|(_, byte)| *byte)
            .unwrap_or(body.len());
        let (start, end) = content_span(body, *raw_start, raw_end);
        content_starts.push(start);
        content_ends.push(end);
    }
    let mut bounds: HashMap<u64, (usize, usize, usize)> = HashMap::new();
    for (index, (row, _)) in starts.iter().enumerate() {
        let chunk_end = content_starts.get(index + 1).copied().unwrap_or(body.len());
        bounds.insert(
            *row,
            (content_starts[index], content_ends[index], chunk_end),
        );
    }

    let mut edits_by_row: BTreeMap<u64, Vec<&PlannedEdit>> = BTreeMap::new();
    let mut sort_edits = Vec::new();
    for edit in edits {
        if matches!(edit.operation, FixOperation::SortStopTimes { .. }) {
            sort_edits.push(*edit);
        } else {
            edits_by_row.entry(edit.row).or_default().push(edit);
        }
    }

    let mut applied = Vec::new();
    let mut conflicts = Vec::new();
    let mut replacements: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut deleted_rows = HashSet::new();

    for (row, row_edits) in edits_by_row {
        let Some(record) = records.get(&row) else {
            conflicts.extend(row_edits.into_iter().map(|edit| FixConflict {
                edit: edit.clone(),
                reason: ConflictReason::RowNotFound,
            }));
            continue;
        };

        let mut fields: Vec<String> = record
            .iter()
            .map(|field| String::from_utf8_lossy(field).into_owned())
            .collect();
        let mut written: HashMap<usize, String> = HashMap::new();
        let mut row_applied = Vec::new();
        let mut delete_row = false;

        for edit in row_edits {
            match &edit.operation {
                FixOperation::DeleteRow {
                    field, expected, ..
                } => {
                    let Some(&index) = exact
                        .get(field)
                        .or_else(|| lowercase.get(&field.to_ascii_lowercase()))
                    else {
                        conflicts.push(FixConflict {
                            edit: edit.clone(),
                            reason: ConflictReason::FieldNotFound,
                        });
                        continue;
                    };
                    let current = fields.get(index).map(String::as_str).unwrap_or("");
                    if !field_matches(current, expected) {
                        conflicts.push(FixConflict {
                            edit: edit.clone(),
                            reason: ConflictReason::ValueMismatch {
                                found: current.to_string(),
                            },
                        });
                        continue;
                    }
                    delete_row = true;
                    row_applied.push(edit.clone());
                }
                FixOperation::ReplaceField {
                    field,
                    original,
                    replacement,
                    ..
                } => {
                    let Some(&index) = exact
                        .get(field)
                        .or_else(|| lowercase.get(&field.to_ascii_lowercase()))
                    else {
                        conflicts.push(FixConflict {
                            edit: edit.clone(),
                            reason: ConflictReason::FieldNotFound,
                        });
                        continue;
                    };

                    if let Some(previous) = written.get(&index) {
                        if previous != replacement {
                            conflicts.push(FixConflict {
                                edit: edit.clone(),
                                reason: ConflictReason::ConflictingEdits {
                                    other: previous.clone(),
                                },
                            });
                        }
                        continue;
                    }

                    let current = fields.get(index).map(String::as_str).unwrap_or("");
                    if !field_matches(current, original) {
                        conflicts.push(FixConflict {
                            edit: edit.clone(),
                            reason: ConflictReason::ValueMismatch {
                                found: current.to_string(),
                            },
                        });
                        continue;
                    }
                    if index >= fields.len() {
                        fields.resize(index + 1, String::new());
                    }
                    fields[index] = replacement.clone();
                    written.insert(index, replacement.clone());
                    row_applied.push(edit.clone());
                }
                FixOperation::SortStopTimes { .. } => unreachable!("partitioned above"),
            }
        }

        if row_applied.is_empty() {
            continue;
        }
        if !bounds.contains_key(&row) {
            conflicts.extend(row_applied.into_iter().map(|edit| FixConflict {
                edit,
                reason: ConflictReason::RowNotFound,
            }));
            continue;
        }
        if delete_row {
            deleted_rows.insert(row);
        } else {
            replacements.insert(row, serialize_record(&fields)?);
        }
        applied.extend(row_applied);
    }

    // Sorting composes with replacements and unsafe deletions. If a malformed
    // record or ambiguous sequence makes it unsafe, keep the other edits and
    // report only the sort as a conflict.
    if let Some(sort_edit) = sort_edits.first().copied() {
        match sort_stop_time_chunks(
            body,
            &headers,
            &starts,
            &records,
            &bounds,
            &replacements,
            &deleted_rows,
        ) {
            Ok(sorted_body) => {
                applied.push(sort_edit.clone());
                let mut out = Vec::with_capacity(data.len() + 64);
                out.extend_from_slice(&data[..bom_len]);
                out.extend_from_slice(&sorted_body);
                return Ok(RewriteResult {
                    bytes: out,
                    applied,
                    conflicts,
                });
            }
            Err(message) => conflicts.push(FixConflict {
                edit: sort_edit.clone(),
                reason: ConflictReason::CannotSort { message },
            }),
        }
    }

    // No sort, or a safely refused sort: splice only the selected rows.
    let mut splices: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    for row in &deleted_rows {
        if let Some((start, _, chunk_end)) = bounds.get(row) {
            splices.push((*start, *chunk_end, Vec::new()));
        }
    }
    for (row, replacement) in replacements {
        if let Some((start, end, _)) = bounds.get(&row) {
            splices.push((*start, *end, replacement));
        }
    }
    splices.sort_by_key(|(start, _, _)| *start);

    let mut out = Vec::with_capacity(data.len() + 64);
    out.extend_from_slice(&data[..bom_len]);
    let mut cursor = 0usize;
    for (start, end, replacement) in splices {
        out.extend_from_slice(&body[cursor..start]);
        out.extend_from_slice(&replacement);
        cursor = end;
    }
    out.extend_from_slice(&body[cursor..]);

    Ok(RewriteResult {
        bytes: out,
        applied,
        conflicts,
    })
}

fn sort_stop_time_chunks(
    body: &[u8],
    headers: &csv::ByteRecord,
    starts: &[(u64, usize)],
    records: &HashMap<u64, csv::ByteRecord>,
    bounds: &HashMap<u64, (usize, usize, usize)>,
    replacements: &HashMap<u64, Vec<u8>>,
    deleted_rows: &HashSet<u64>,
) -> Result<Vec<u8>, String> {
    if starts.len() != records.len() {
        return Err("the file contains a malformed CSV record".into());
    }

    let mut header_index = HashMap::new();
    for (index, header) in headers.iter().enumerate() {
        header_index
            .entry(String::from_utf8_lossy(header).trim().to_ascii_lowercase())
            .or_insert(index);
    }
    let trip_index = header_index
        .get("trip_id")
        .copied()
        .ok_or_else(|| "trip_id column is missing".to_string())?;
    let sequence_index = header_index
        .get("stop_sequence")
        .copied()
        .ok_or_else(|| "stop_sequence column is missing".to_string())?;

    struct SortableRow {
        row: u64,
        trip_order: usize,
        sequence: u64,
        source_order: usize,
    }

    let mut first_trip_order: HashMap<String, usize> = HashMap::new();
    let mut seen_sequences: HashSet<(String, u64)> = HashSet::new();
    let mut sortable = Vec::new();
    for (source_order, (row, _)) in starts.iter().enumerate() {
        if deleted_rows.contains(row) {
            continue;
        }
        let record = records
            .get(row)
            .ok_or_else(|| format!("row {row} could not be parsed"))?;
        let trip_id = record
            .get(trip_index)
            .map(|value| String::from_utf8_lossy(value).trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("row {row} has no trip_id"))?;
        let sequence_text = record
            .get(sequence_index)
            .map(|value| String::from_utf8_lossy(value).trim().to_string())
            .ok_or_else(|| format!("row {row} has no stop_sequence"))?;
        let sequence = sequence_text
            .parse::<u64>()
            .map_err(|_| format!("row {row} has invalid stop_sequence {sequence_text:?}"))?;
        if !seen_sequences.insert((trip_id.clone(), sequence)) {
            return Err(format!(
                "trip {trip_id:?} has duplicate stop_sequence {sequence}"
            ));
        }
        let next_order = first_trip_order.len();
        let trip_order = *first_trip_order.entry(trip_id).or_insert(next_order);
        sortable.push(SortableRow {
            row: *row,
            trip_order,
            sequence,
            source_order,
        });
    }
    if sortable.is_empty() {
        return Err("no sortable rows remain".into());
    }

    let original_order: Vec<u64> = sortable.iter().map(|entry| entry.row).collect();
    sortable.sort_by_key(|entry| (entry.trip_order, entry.sequence, entry.source_order));
    if sortable
        .iter()
        .map(|entry| entry.row)
        .eq(original_order.iter().copied())
    {
        return Err("rows are already in canonical order".into());
    }

    let first_start = starts
        .first()
        .and_then(|(row, _)| bounds.get(row))
        .map(|(start, _, _)| *start)
        .ok_or_else(|| "cannot locate the first data row".to_string())?;
    let mut out = Vec::with_capacity(body.len() + 64);
    out.extend_from_slice(&body[..first_start]);
    for entry in sortable {
        let (start, end, chunk_end) = bounds
            .get(&entry.row)
            .copied()
            .ok_or_else(|| format!("cannot locate row {}", entry.row))?;
        if let Some(replacement) = replacements.get(&entry.row) {
            out.extend_from_slice(replacement);
        } else {
            out.extend_from_slice(&body[start..end]);
        }
        out.extend_from_slice(&body[end..chunk_end]);
    }
    Ok(out)
}

/// Whether the field on disk still holds the value a fix was computed from.
///
/// Rules hand the loader's already-trimmed value to the fix, and the loader
/// trims two different ways: Rust's Unicode-aware `trim` in some paths, and the
/// Java-compatible "strip anything at or below space" in the row validator.
/// Accepting either keeps a fix from being rejected over whitespace it was
/// never going to write anyway.
fn field_matches(current: &str, original: &str) -> bool {
    current == original
        || current.trim() == original
        || current.trim_matches(|ch: char| ch <= ' ') == original
}

/// Narrow a reported record span down to the record's own bytes.
///
/// The csv reader's positions can sit on a line terminator rather than on the
/// first content byte — notably on CRLF input, where a record is reported one
/// byte early — and consecutive records may be separated by blank lines.
/// Trimming terminator bytes off both ends leaves exactly the record, so the
/// surrounding terminators are copied through untouched instead of being
/// re-emitted. Record content never starts or ends with a bare newline: those
/// only appear inside quotes, which close with `"`.
fn content_span(body: &[u8], raw_start: usize, raw_end: usize) -> (usize, usize) {
    let mut start = raw_start;
    while start < raw_end && matches!(body[start], b'\r' | b'\n') {
        start += 1;
    }
    let mut end = raw_end;
    while end > start && matches!(body[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    (start, end)
}

/// Serialize one record without a line terminator; the original terminator
/// bytes stay in the file untouched.
fn serialize_record(fields: &[String]) -> Result<Vec<u8>, FixError> {
    let mut writer = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Necessary)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer.write_record(fields).map_err(|err| FixError::Csv {
        file: String::from("<record>"),
        message: err.to_string(),
    })?;
    let mut line = writer.into_inner().map_err(|err| FixError::Csv {
        file: String::from("<record>"),
        message: err.to_string(),
    })?;

    if line.last() == Some(&b'\n') {
        line.pop();
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notice::{Fix, FixOperation, NoticeSeverity};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn edit(row: u64, field: &str, original: &str, replacement: &str) -> PlannedEdit {
        PlannedEdit {
            notice_code: "u_r_i_syntax_error".into(),
            description: "Add https:// scheme".into(),
            safety: FixSafety::Safe,
            file: "agency.txt".into(),
            row,
            field: field.into(),
            original: original.into(),
            replacement: replacement.into(),
            operation: FixOperation::ReplaceField {
                file: "agency.txt".into(),
                row,
                field: field.into(),
                original: original.into(),
                replacement: replacement.into(),
            },
        }
    }

    fn delete_edit(file: &str, row: u64, field: &str, expected: &str) -> PlannedEdit {
        PlannedEdit {
            notice_code: "foreign_key_violation".into(),
            description: "Delete orphan".into(),
            safety: FixSafety::Unsafe,
            file: file.into(),
            row,
            field: field.into(),
            original: expected.into(),
            replacement: String::new(),
            operation: FixOperation::DeleteRow {
                file: file.into(),
                row,
                field: field.into(),
                expected: expected.into(),
            },
        }
    }

    fn sort_edit() -> PlannedEdit {
        PlannedEdit {
            notice_code: "unsorted_stop_times".into(),
            description: "Sort stop times".into(),
            safety: FixSafety::Safe,
            file: "stop_times.txt".into(),
            row: 0,
            field: "trip_id, stop_sequence".into(),
            original: String::new(),
            replacement: String::new(),
            operation: FixOperation::SortStopTimes {
                file: "stop_times.txt".into(),
            },
        }
    }

    fn rewrite(data: &[u8], edits: &[PlannedEdit]) -> RewriteResult {
        let refs: Vec<&PlannedEdit> = edits.iter().collect();
        rewrite_csv("agency.txt", data, &refs).expect("rewrite")
    }

    fn temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
    }

    #[test]
    fn replaces_only_the_targeted_field() {
        let data = b"agency_id,agency_name,agency_url\n1,Transit,www.example.com\n";
        let result = rewrite(
            data,
            &[edit(
                2,
                "agency_url",
                "www.example.com",
                "https://www.example.com",
            )],
        );

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            "agency_id,agency_name,agency_url\n1,Transit,https://www.example.com\n"
        );
        assert_eq!(result.applied.len(), 1);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn leaves_other_rows_byte_identical() {
        let data = b"agency_id,agency_url\n1,www.a.com\n2,\"quoted, value\"\n3,www.c.com\n";
        let result = rewrite(
            data,
            &[edit(4, "agency_url", "www.c.com", "https://www.c.com")],
        );

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            "agency_id,agency_url\n1,www.a.com\n2,\"quoted, value\"\n3,https://www.c.com\n"
        );
    }

    #[test]
    fn preserves_crlf_and_bom() {
        let mut data = UTF8_BOM.to_vec();
        data.extend_from_slice(b"agency_id,agency_url\r\n1,www.a.com\r\n2,www.b.com\r\n");
        // Row numbers follow the loader: header is row 1, so the second data
        // row is row 3 regardless of the line terminator.
        let result = rewrite(
            &data,
            &[edit(3, "agency_url", "www.b.com", "https://www.b.com")],
        );

        let mut expected = UTF8_BOM.to_vec();
        expected
            .extend_from_slice(b"agency_id,agency_url\r\n1,www.a.com\r\n2,https://www.b.com\r\n");
        assert_eq!(result.bytes, expected);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn edits_the_only_data_row_of_a_crlf_file() {
        let data = b"agency_id,agency_url\r\n1,www.a.com\r\n".to_vec();
        let result = rewrite(
            &data,
            &[edit(2, "agency_url", "www.a.com", "https://www.a.com")],
        );
        assert!(
            result.conflicts.is_empty(),
            "row 2 must resolve, got {:?}",
            result.conflicts
        );
        assert_eq!(
            result.bytes,
            b"agency_id,agency_url\r\n1,https://www.a.com\r\n".to_vec()
        );
    }

    /// The writer resolves a fix's `row` exactly the way the loader assigns it,
    /// so a notice always points at the record the writer edits. Both number the
    /// header as row 1, independent of the line terminator.
    #[test]
    fn loader_row_numbering_is_reproduced() {
        #[derive(serde::Deserialize)]
        struct Row {
            #[allow(dead_code)]
            stop_id: String,
        }

        for (label, raw, expected) in [
            (
                "lf",
                b"stop_id,stop_name\nS1,A\nS2,B\n".to_vec(),
                vec![2, 3],
            ),
            (
                "crlf",
                b"stop_id,stop_name\r\nS1,A\r\nS2,B\r\n".to_vec(),
                vec![2, 3],
            ),
        ] {
            let dir = temp_path(&format!("gtfs_rows_{label}"));
            std::fs::create_dir_all(&dir).expect("create dir");
            std::fs::write(dir.join("stops.txt"), &raw).expect("write stops");

            let input = GtfsInput::from_path(&dir).expect("input");
            let mut notices = NoticeContainer::new();
            let pool = crate::StringPool::new();
            let table: crate::CsvTable<Row> = input
                .reader()
                .read_csv_with_notices("stops.txt", &mut notices, &pool)
                .expect("read stops");
            assert_eq!(table.row_numbers, expected, "{label} loader rows");

            // The writer finds the same rows the loader numbered.
            let planned: Vec<PlannedEdit> = expected
                .iter()
                .map(|row| PlannedEdit {
                    file: "stops.txt".into(),
                    row: *row,
                    field: "stop_name".into(),
                    original: if *row == expected[0] { "A" } else { "B" }.into(),
                    replacement: "fixed".into(),
                    operation: FixOperation::ReplaceField {
                        file: "stops.txt".into(),
                        row: *row,
                        field: "stop_name".into(),
                        original: if *row == expected[0] { "A" } else { "B" }.into(),
                        replacement: "fixed".into(),
                    },
                    ..edit(*row, "stop_name", "", "")
                })
                .collect();
            let refs: Vec<&PlannedEdit> = planned.iter().collect();
            let result = rewrite_csv("stops.txt", &raw, &refs).expect("rewrite");
            assert_eq!(result.applied.len(), 2, "{label} applied");
            assert!(result.conflicts.is_empty(), "{label} conflicts");

            std::fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn preserves_a_missing_final_terminator() {
        let data = b"agency_id,agency_url\n1,www.a.com";
        let result = rewrite(
            data,
            &[edit(2, "agency_url", "www.a.com", "https://www.a.com")],
        );

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            "agency_id,agency_url\n1,https://www.a.com"
        );
    }

    #[test]
    fn quotes_a_replacement_that_needs_it() {
        let data = b"agency_id,agency_name\n1,Transit\n";
        let mut planned = edit(2, "agency_name", "Transit", "Transit, Inc.");
        planned.field = "agency_name".into();
        let result = rewrite(data, &[planned]);

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            "agency_id,agency_name\n1,\"Transit, Inc.\"\n"
        );
    }

    #[test]
    fn reports_a_value_mismatch_instead_of_overwriting() {
        let data = b"agency_id,agency_url\n1,https://already-fixed.com\n";
        let result = rewrite(
            data,
            &[edit(2, "agency_url", "www.a.com", "https://www.a.com")],
        );

        assert!(result.applied.is_empty());
        assert_eq!(result.bytes, data);
        assert!(matches!(
            result.conflicts[0].reason,
            ConflictReason::ValueMismatch { .. }
        ));
    }

    #[test]
    fn reports_a_missing_column() {
        let data = b"agency_id,agency_name\n1,Transit\n";
        let result = rewrite(
            data,
            &[edit(2, "agency_url", "www.a.com", "https://www.a.com")],
        );

        assert!(result.applied.is_empty());
        assert_eq!(result.conflicts[0].reason, ConflictReason::FieldNotFound);
    }

    #[test]
    fn reports_a_missing_row() {
        let data = b"agency_id,agency_url\n1,www.a.com\n";
        let result = rewrite(
            data,
            &[edit(99, "agency_url", "www.a.com", "https://www.a.com")],
        );

        assert!(result.applied.is_empty());
        assert_eq!(result.conflicts[0].reason, ConflictReason::RowNotFound);
    }

    #[test]
    fn deletes_an_orphan_row_and_its_terminator() {
        let data = b"trip_id,stop_id\r\nT1,missing\r\nT2,S2\r\n";
        let planned = delete_edit("stop_times.txt", 2, "stop_id", "missing");
        let refs = [&planned];
        let result = rewrite_csv("stop_times.txt", data, &refs).expect("rewrite");

        assert_eq!(result.bytes, b"trip_id,stop_id\r\nT2,S2\r\n");
        assert_eq!(result.applied, vec![planned]);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn refuses_to_delete_a_row_that_changed() {
        let data = b"trip_id,stop_id\nT1,S1\n";
        let planned = delete_edit("stop_times.txt", 2, "stop_id", "missing");
        let refs = [&planned];
        let result = rewrite_csv("stop_times.txt", data, &refs).expect("rewrite");

        assert_eq!(result.bytes, data);
        assert!(result.applied.is_empty());
        assert!(matches!(
            result.conflicts[0].reason,
            ConflictReason::ValueMismatch { .. }
        ));
    }

    #[test]
    fn sorts_stop_times_stably_and_preserves_raw_rows() {
        let data = concat!(
            "trip_id,stop_sequence,stop_headsign\r\n",
            "T2,2,\"Second, quoted\"\r\n",
            "T1,2,second\r\n",
            "T2,1,first\r\n",
            "T1,1,first\r\n",
        )
        .as_bytes();
        let planned = sort_edit();
        let refs = [&planned];
        let result = rewrite_csv("stop_times.txt", data, &refs).expect("rewrite");

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            concat!(
                "trip_id,stop_sequence,stop_headsign\r\n",
                "T2,1,first\r\n",
                "T2,2,\"Second, quoted\"\r\n",
                "T1,1,first\r\n",
                "T1,2,second\r\n",
            )
        );
        assert_eq!(result.applied, vec![planned]);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn refuses_to_sort_duplicate_sequences() {
        let data = b"trip_id,stop_sequence\nT1,2\nT1,2\nT1,1\n";
        let planned = sort_edit();
        let refs = [&planned];
        let result = rewrite_csv("stop_times.txt", data, &refs).expect("rewrite");

        assert_eq!(result.bytes, data);
        assert!(result.applied.is_empty());
        assert!(matches!(
            result.conflicts[0].reason,
            ConflictReason::CannotSort { .. }
        ));
    }

    #[test]
    fn skips_a_row_that_lost_its_quoting_context() {
        // A record spanning two physical lines: the next record's start offset
        // must be used, not "start + one line".
        let data =
            b"agency_id,agency_name,agency_url\n1,\"Multi\nline\",www.a.com\n2,Other,www.b.com\n";
        let result = rewrite(
            data,
            &[edit(2, "agency_url", "www.a.com", "https://www.a.com")],
        );

        assert_eq!(
            String::from_utf8(result.bytes).unwrap(),
            "agency_id,agency_name,agency_url\n1,\"Multi\nline\",https://www.a.com\n2,Other,www.b.com\n"
        );
    }

    #[test]
    fn plan_splits_by_safety_ceiling() {
        let mut notices = NoticeContainer::new();
        for (code, safety) in [
            ("safe_one", FixSafety::Safe),
            ("confirm_one", FixSafety::RequiresConfirmation),
            ("unsafe_one", FixSafety::Unsafe),
        ] {
            let mut notice = ValidationNotice::new(code, NoticeSeverity::Error, "boom");
            notice.fix = Some(Fix {
                description: "fix".into(),
                safety,
                operation: FixOperation::ReplaceField {
                    file: "agency.txt".into(),
                    row: 2,
                    field: "agency_url".into(),
                    original: "a".into(),
                    replacement: "b".into(),
                },
            });
            notices.push(notice);
        }

        let safe_only = FixPlan::from_notices(&notices, FixSafety::Safe);
        assert_eq!(safe_only.edits().len(), 1);
        assert_eq!(safe_only.skipped().len(), 2);
        assert_eq!(safe_only.counts().total(), 3);

        let everything = FixPlan::from_notices(&notices, FixSafety::Unsafe);
        assert_eq!(everything.edits().len(), 3);
        assert!(everything.skipped().is_empty());
    }

    #[test]
    fn writes_a_fixed_directory_without_touching_the_input() {
        let dir = temp_path("gtfs_fix_dir");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            dir.join("agency.txt"),
            b"agency_id,agency_url\n1,www.a.com\n",
        )
        .expect("write agency");
        std::fs::write(dir.join("stops.txt"), b"stop_id\nS1\n").expect("write stops");

        let input = GtfsInput::from_path(&dir).expect("input");
        let mut plan = FixPlan::default();
        plan.edits
            .push(edit(2, "agency_url", "www.a.com", "https://www.a.com"));

        let output = temp_path("gtfs_fix_dir_out");
        let outcome = apply_fixes(&input, &plan, &output).expect("apply");

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.rewritten_files, vec!["agency.txt".to_string()]);
        assert_eq!(
            std::fs::read(output.join("agency.txt")).unwrap(),
            b"agency_id,agency_url\n1,https://www.a.com\n"
        );
        // Untouched file copied, input left alone.
        assert_eq!(
            std::fs::read(output.join("stops.txt")).unwrap(),
            b"stop_id\nS1\n"
        );
        assert_eq!(
            std::fs::read(dir.join("agency.txt")).unwrap(),
            b"agency_id,agency_url\n1,www.a.com\n"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&output).ok();
    }

    #[test]
    fn writes_a_fixed_zip_and_keeps_other_members() {
        let dir = temp_path("gtfs_fix_zip");
        std::fs::create_dir_all(&dir).expect("create dir");
        let zip_path = dir.join("feed.zip");

        let zip_file = File::create(&zip_path).expect("create zip");
        let mut zip = ZipWriter::new(zip_file);
        zip.start_file("agency.txt", FileOptions::default())
            .expect("member");
        zip.write_all(b"agency_id,agency_url\n1,www.a.com\n")
            .expect("write");
        zip.start_file("stops.txt", FileOptions::default())
            .expect("member");
        zip.write_all(b"stop_id\nS1\n").expect("write");
        zip.finish().expect("finish");

        let input = GtfsInput::from_path(&zip_path).expect("input");
        let mut plan = FixPlan::default();
        plan.edits
            .push(edit(2, "agency_url", "www.a.com", "https://www.a.com"));

        let output = dir.join("feed.fixed.zip");
        let outcome = apply_fixes(&input, &plan, &output).expect("apply");
        assert_eq!(outcome.applied.len(), 1);

        let fixed = GtfsInput::from_path(&output).expect("fixed input");
        let reader = fixed.reader();
        assert_eq!(
            reader.read_file("agency.txt").unwrap(),
            b"agency_id,agency_url\n1,https://www.a.com\n"
        );
        assert_eq!(reader.read_file("stops.txt").unwrap(), b"stop_id\nS1\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_to_write_over_the_input() {
        let dir = temp_path("gtfs_fix_same");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("agency.txt"), b"agency_id\n1\n").expect("write");

        let input = GtfsInput::from_path(&dir).expect("input");
        let plan = FixPlan::default();
        let err = apply_fixes(&input, &plan, &dir).expect_err("must refuse");
        assert!(matches!(err, FixError::OutputSameAsInput(_)));

        // Also via a non-canonical spelling of the same directory.
        let indirect = dir.join(".").join("..").join(dir.file_name().unwrap());
        let err = apply_fixes(&input, &plan, &indirect).expect_err("must refuse");
        assert!(matches!(err, FixError::OutputSameAsInput(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_an_existing_output() {
        let dir = temp_path("gtfs_fix_exists");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("agency.txt"), b"agency_id\n1\n").expect("write");
        let output = temp_path("gtfs_fix_exists_out");
        std::fs::create_dir_all(&output).expect("create output");

        let input = GtfsInput::from_path(&dir).expect("input");
        let err = apply_fixes(&input, &FixPlan::default(), &output).expect_err("must refuse");
        assert!(matches!(err, FixError::OutputExists(_)));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&output).ok();
    }
}
