//! Byte-level pass that hands the csv crate the records univocity would see.
//!
//! The canonical validator parses with univocity configured as in
//! `CsvFile.createDefaultParserSettings()`: `\n` is the only line separator,
//! whitespace (any char `<= U+0020`, so `\r` too) around an unquoted value is
//! dropped, a quote that follows such whitespace still opens a quoted value,
//! whitespace inside quotes is kept, a line starting with `#` is a comment,
//! and a value longer than 4096 chars or a record over 512 columns aborts the file. RFC 4180 readers such
//! as the csv crate disagree on nearly all of that, so this pass rewrites the
//! bytes once, before parsing, until the csv crate (configured with a `\n`
//! terminator, see [`crate::csv_reader::csv_reader_builder`]) agrees:
//!
//! * a UTF-8 byte order mark at the very start is dropped (Java wraps the
//!   stream in a `BOMInputStream`);
//! * whitespace at the start of a field and at the end of an unquoted field
//!   is removed, as is whitespace between a closing quote and the delimiter;
//! * everything inside quotes, `""` escapes included, is copied verbatim;
//! * a line whose first byte is `#` is emptied (its `\n` stays);
//! * a line holding only whitespace is emptied, so the csv crate skips it the
//!   way univocity does -- except before the first record, where univocity
//!   reads it as a header with one empty column, and at the very end of the
//!   input without a newline, where it yields a one-field row that the
//!   canonical validator reports as `empty_row`. Both become `""`, as does
//!   every whitespace-only line under `--thorough`, which reports them all;
//! * every record ends with `\n`: a missing final newline is added, and an
//!   unterminated quote is closed first.
//!
//! Bytes are otherwise only removed, never reordered, and no `\n` is ever
//! removed, so the line on which a record ends -- the canonical validator's
//! row number -- is the csv reader's line count after the record, minus one.
//!
//! The pass also enforces univocity's per-value length limit (see
//! [`CsvLimitError`]), which it can only do here: the limit counts trailing
//! whitespace the pass removes.

use std::io::{self, Read};

/// Univocity's default `maxCharsPerColumn`, which the canonical validator
/// keeps for every table but `areas.txt`.
pub const DEFAULT_MAX_CHARS_PER_COLUMN: usize = 4096;
pub const MAX_COLUMNS: u64 = 512;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Before the first significant byte of a field.
    FieldStart,
    /// Inside a bare field; trailing whitespace is held back in `pending`.
    Unquoted,
    /// Inside a quoted field.
    Quoted,
    /// Just saw a `"` inside a quoted field; it was either an escape or the
    /// closing quote, decided by the next byte.
    QuotedQuote,
    /// After the closing quote, before the delimiter.
    AfterQuoted,
    /// Inside a `#` comment line.
    Comment,
}

/// Whitespace as univocity trims it: every char `<= U+0020` except the line
/// separator.
#[inline]
fn is_ws(b: u8) -> bool {
    b <= b' ' && b != b'\n'
}

/// Whether any byte of `word` is whitespace, a control byte or `"`: a byte
/// `plain_lines` has to look at.
#[inline]
fn has_special_byte(word: u64) -> bool {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;
    // Bytes below 0x21 (only for bytes below 0x80, which is all that matters:
    // a byte with its high bit set is never special).
    let below = word.wrapping_sub(ONES * 0x21) & !word & HIGHS;
    let quotes = word ^ (ONES * u64::from(b'"'));
    let quote = quotes.wrapping_sub(ONES) & !quotes & HIGHS;
    (below | quote) != 0
}

/// UTF-16 code units the UTF-8 bytes decode to, which is how Java measures a
/// string. Continuation bytes add nothing; a four-byte sequence is a
/// surrogate pair.
fn utf16_units(bytes: &[u8]) -> u64 {
    byte_stats(bytes).0
}

/// UTF-16 units and `\n` count of `bytes` in one pass. Counts in `u8` lanes
/// over short blocks so the loop vectorises; this runs over every byte of a
/// table.
pub(crate) fn byte_stats(bytes: &[u8]) -> (u64, u64) {
    let mut units = 0u64;
    let mut newlines = 0u64;
    for block in bytes.chunks(255) {
        let mut continuation = 0u8;
        let mut four_byte = 0u8;
        let mut lines = 0u8;
        for &b in block {
            continuation += ((b & 0xC0) == 0x80) as u8;
            four_byte += (b >= 0xF0) as u8;
            lines += (b == b'\n') as u8;
        }
        units += block.len() as u64 - continuation as u64 + four_byte as u64;
        newlines += lines as u64;
    }
    (units, newlines)
}

/// Width in bytes and length in UTF-16 units of the char starting `bytes`.
/// A malformed sequence is one byte and one char, as Java's decoder replaces
/// it with U+FFFD.
fn decode_char(bytes: &[u8]) -> (usize, usize) {
    let Some(&lead) = bytes.first() else {
        return (0, 0);
    };
    let width = match lead {
        0x00..=0x7F => return (1, 1),
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return (1, 1),
    };
    match bytes.get(..width).map(std::str::from_utf8) {
        Some(Ok(_)) => (width, if width == 4 { 2 } else { 1 }),
        _ => (1, 1),
    }
}

fn count_newlines(bytes: &[u8]) -> u64 {
    byte_stats(bytes).1
}

/// A value or record exceeding a configured parser limit. Java reports it
/// as `csv_parsing_failed` carrying univocity's `TextParsingException` state,
/// and stops reading the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvLimitError {
    /// Whether the record exceeded maxColumns rather than maxCharsPerColumn.
    pub too_many_columns: bool,
    /// Configured maxCharsPerColumn; usize::MAX represents unlimited.
    pub max_chars: usize,
    /// Univocity's physical line counter at failure.
    pub line_index: u64,
    /// Univocity's column counter (513 when overflowing its 512-slot array).
    pub column_index: u64,
    /// Data records read before this one (the header is not counted).
    pub record_index: u64,
    /// Chars (UTF-16 units) consumed, through the offending one.
    pub char_index: u64,
    /// The first `max_chars` chars of the value.
    pub parsed_content: String,
}

impl std::fmt::Display for CsvLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.too_many_columns {
            return f.write_str(
                "java.lang.ArrayIndexOutOfBoundsException - Index 512 out of bounds for length 512",
            );
        }
        write!(
            f,
            "Length of parsed input ({}) exceeds the maximum number of characters defined in your parser settings ({}).",
            self.max_chars + 1,
            self.max_chars
        )
    }
}

impl std::error::Error for CsvLimitError {}

impl CsvLimitError {
    /// Recover the overflow from an `io::Error` produced by
    /// [`NormalizingReader`], if that is what it is.
    pub fn from_io_error(err: &io::Error) -> Option<&CsvLimitError> {
        err.get_ref()?.downcast_ref::<CsvLimitError>()
    }

    /// Recover the overflow from a csv error wrapping the reader's `io::Error`.
    pub fn from_csv_error(err: &csv::Error) -> Option<&CsvLimitError> {
        match err.kind() {
            csv::ErrorKind::Io(io_err) => Self::from_io_error(io_err),
            _ => None,
        }
    }

    fn into_io_error(self) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, self)
    }
}

/// Length bookkeeping for the value being read.
#[derive(Debug, Default)]
struct FieldTrack {
    /// An upper bound on the value's length in UTF-16 units: every raw byte
    /// emitted for it, held-back whitespace included.
    raw_bytes: usize,
    /// `raw_bytes` at which the exact length must next be computed.
    next_check: usize,
    /// Index in the current output buffer where this value's bytes start.
    out_start: usize,
    /// The value's bytes emitted by earlier `push` calls, capped: only the
    /// prefix holding the first `max_chars + 1` chars is ever needed.
    carry: Vec<u8>,
    /// Raw bytes moved to `carry`, cap ignored.
    carry_len: usize,
    /// Whether the value is quoted (its first raw byte is the opening quote).
    quoted: bool,
    open: bool,
}

/// Incremental normaliser; feed bytes in any chunking, then call `finish`.
#[derive(Debug)]
pub struct Normalizer {
    state: State,
    /// Whitespace not yet known to be leading, trailing, or content.
    pending: Vec<u8>,
    /// True until the first significant byte of the current line.
    at_line_start: bool,
    /// Whether any record has been emitted yet.
    seen_record: bool,
    /// Bytes of a possible BOM seen so far, while undecided.
    bom: Vec<u8>,
    bom_done: bool,
    max_chars: Option<usize>,
    /// Keep a whitespace-only line as an empty one-field row (`--thorough`,
    /// which reports it as `empty_row`) instead of skipping it.
    keep_whitespace_rows: bool,
    field: FieldTrack,
    /// Records completed, header included.
    records: u64,
    /// Index of the current value within its record.
    column: u64,
    /// UTF-16 units and newlines in input consumed by earlier `push` calls.
    units_before: u64,
    lines_before: u64,
    failed: Option<CsvLimitError>,
}

impl Default for Normalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer {
    /// A normaliser without a value-length limit.
    pub fn new() -> Self {
        Self::with_max_chars(None)
    }

    /// A normaliser that fails once a value exceeds `max_chars` chars.
    pub fn with_max_chars(max_chars: Option<usize>) -> Self {
        Self {
            state: State::FieldStart,
            pending: Vec::new(),
            at_line_start: true,
            seen_record: false,
            bom: Vec::new(),
            bom_done: false,
            max_chars,
            keep_whitespace_rows: false,
            field: FieldTrack::default(),
            records: 0,
            column: 0,
            units_before: 0,
            lines_before: 0,
            failed: None,
        }
    }

    /// Keep whitespace-only lines as empty one-field rows rather than
    /// skipping them as the canonical validator does.
    pub fn keep_whitespace_rows(mut self, keep: bool) -> Self {
        self.keep_whitespace_rows = keep;
        self
    }

    pub fn push(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), CsvLimitError> {
        if let Some(err) = &self.failed {
            return Err(err.clone());
        }
        if !self.bom_done {
            let mut consumed = 0;
            while self.bom.len() < BOM.len() && consumed < input.len() {
                self.bom.push(input[consumed]);
                consumed += 1;
                if self.bom[..] != BOM[..self.bom.len()] {
                    break;
                }
            }
            if self.bom[..] == BOM[..] {
                self.bom_done = true;
                self.bom.clear();
            } else if self.bom[..] != BOM[..self.bom.len()] {
                // Not a BOM: the held bytes are data.
                self.bom_done = true;
                let held = std::mem::take(&mut self.bom);
                self.process(&held, out)?;
            } else {
                // Still a BOM prefix; wait for more input.
                return Ok(());
            }
            return self.process(&input[consumed..], out);
        }
        self.process(input, out)
    }

    fn process(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), CsvLimitError> {
        if self.field.open {
            self.field.out_start = out.len();
        }
        let result = self.process_inner(input, out);
        if let Err(err) = result {
            self.failed = Some(err.clone());
            return Err(err);
        }
        if self.field.open {
            self.carry_field(out);
        }
        let (units, lines) = byte_stats(input);
        self.units_before += units;
        self.lines_before += lines;
        Ok(())
    }

    /// Move the open value's bytes from this call's output into `carry`.
    fn carry_field(&mut self, out: &[u8]) {
        let Some(max) = self.max_chars else {
            return;
        };
        let bytes = &out[self.field.out_start.min(out.len())..];
        let cap = max.saturating_mul(4).saturating_add(16);
        let room = cap.saturating_sub(self.field.carry.len());
        self.field
            .carry
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
        self.field.carry_len += bytes.len();
    }

    fn begin_field(&mut self, out: &[u8], quoted: bool) {
        if let Some(max) = self.max_chars {
            self.field.raw_bytes = 0;
            self.field.next_check = max + 1;
            self.field.out_start = out.len();
            self.field.carry.clear();
            self.field.carry_len = 0;
            self.field.quoted = quoted;
            self.field.open = true;
        }
    }

    #[inline]
    fn end_field(&mut self) {
        self.field.open = false;
    }

    fn end_record(&mut self) {
        self.end_field();
        self.records += 1;
        self.column = 0;
        self.seen_record = true;
    }

    /// Account for `added` more raw bytes of the open value, ending at
    /// `input[..pos]`, and fail if the value is now too long.
    #[inline]
    fn grow(
        &mut self,
        added: usize,
        input: &[u8],
        pos: usize,
        out: &[u8],
    ) -> Result<(), CsvLimitError> {
        if self.max_chars.is_none() {
            return Ok(());
        }
        self.field.raw_bytes += added;
        if self.field.raw_bytes >= self.field.next_check {
            self.check_length(input, pos, out)?;
        }
        Ok(())
    }

    /// The value's raw bytes so far: carried, emitted in this call, held back.
    fn field_raw_prefix(&self, out: &[u8]) -> Vec<u8> {
        let mut raw = self.field.carry.clone();
        raw.extend_from_slice(&out[self.field.out_start.min(out.len())..]);
        raw.extend_from_slice(&self.pending);
        raw
    }

    #[cold]
    fn check_length(&mut self, input: &[u8], pos: usize, out: &[u8]) -> Result<(), CsvLimitError> {
        let max = self.max_chars.unwrap_or(usize::MAX);
        let raw = self.field_raw_prefix(out);
        // Raw bytes of the value, cap ignored: carried + this call + pending.
        let raw_total = self.field.carry_len
            + out.len().saturating_sub(self.field.out_start)
            + self.pending.len();
        // Walk the value char by char; an escaped quote is one char.
        let mut index = usize::from(self.field.quoted && raw.first() == Some(&b'"'));
        let mut chars = 0usize;
        let mut content: Vec<u8> = Vec::new();
        let mut overflow_at = None;
        while index < raw.len() {
            let (width, units, is_content) = if self.field.quoted && raw[index] == b'"' {
                if raw.get(index + 1) == Some(&b'"') {
                    (2, 1, true)
                } else {
                    (1, 0, false)
                }
            } else {
                let (width, units) = decode_char(&raw[index..]);
                (width, units, true)
            };
            if chars + units > max {
                overflow_at = Some(index);
                break;
            }
            chars += units;
            if is_content {
                if width == 2 && raw[index] == b'"' && self.field.quoted {
                    content.push(b'"');
                } else {
                    content.extend_from_slice(&raw[index..index + width]);
                }
            }
            index += width;
        }
        let Some(offending) = overflow_at else {
            // Every further raw byte adds at most one char.
            self.field.next_check = self.field.raw_bytes + (max - chars) + 1;
            return Ok(());
        };
        // The offending char's first byte, as an index into `input`: every raw
        // byte after it was read from this call's input, in order.
        let after = raw_total.saturating_sub(offending);
        let at = pos.saturating_sub(after).min(input.len());
        let (char_len, _) = decode_char(&input[at..]);
        let through = (at + char_len).min(input.len());
        Err(CsvLimitError {
            too_many_columns: false,
            max_chars: max,
            line_index: self.lines_before + count_newlines(&input[..at]),
            column_index: self.column,
            record_index: self.records.saturating_sub(1),
            char_index: self.units_before + utf16_units(&input[..through]),
            parsed_content: String::from_utf8_lossy(&content).into_owned(),
        })
    }

    /// Copy the run of plain lines starting at `input[start]`, which must sit
    /// at the start of a line with nothing held back, and return where the run
    /// ends. A plain line has no quote, no whitespace or control byte except
    /// a `\r` right before its `\n`, does not start with `#`, fits the column
    /// limit and ends with `\n`: univocity reads it exactly as the csv crate
    /// does, so it needs no state machine. This is nearly every line of a
    /// large table, so it is what keeps the pass cheap.
    fn plain_lines(&mut self, input: &[u8], start: usize, out: &mut Vec<u8>) -> usize {
        let max_line = self.max_chars.unwrap_or(usize::MAX);
        let mut i = start;
        // Start of the bytes not yet copied to `out`.
        let mut copied = start;
        let n = input.len();
        'lines: while i < n {
            let line_start = i;
            if input[i] == b'#' {
                break;
            }
            let mut cr = false;
            loop {
                if i >= n {
                    // No newline in this chunk: leave the line to the slow path.
                    i = line_start;
                    break 'lines;
                }
                // Skip eight plain bytes at a time.
                while let Some(word) = input.get(i..i + 8) {
                    let word = u64::from_le_bytes(word.try_into().expect("eight bytes"));
                    if has_special_byte(word) {
                        break;
                    }
                    i += 8;
                }
                if i >= n {
                    continue;
                }
                let b = input[i];
                if b > b' ' && b != b'"' {
                    i += 1;
                    continue;
                }
                if b == b'\n' {
                    break;
                }
                if b == b'\r' && input.get(i + 1) == Some(&b'\n') && i > line_start {
                    cr = true;
                    i += 1;
                    continue;
                }
                i = line_start;
                break 'lines;
            }
            // `input[i]` is the line's `\n`.
            let content = i - line_start - usize::from(cr);
            if content > max_line
                || (content >= MAX_COLUMNS as usize
                    && input[line_start..i].iter().filter(|&&b| b == b',').count()
                        >= MAX_COLUMNS as usize)
            {
                i = line_start;
                break;
            }
            if content > 0 {
                self.records += 1;
                self.seen_record = true;
            }
            if cr {
                out.extend_from_slice(&input[copied..i - 1]);
                copied = i;
            }
            i += 1;
        }
        out.extend_from_slice(&input[copied..i]);
        i
    }

    fn check_columns(&self, input: &[u8], through: usize) -> Result<(), CsvLimitError> {
        if self.column < MAX_COLUMNS {
            return Ok(());
        }
        Err(CsvLimitError {
            too_many_columns: true,
            max_chars: self.max_chars.unwrap_or(usize::MAX),
            line_index: self.lines_before + count_newlines(&input[..through]),
            column_index: self.column + 1,
            record_index: self.records.saturating_sub(1),
            char_index: self.units_before + utf16_units(&input[..through]),
            parsed_content: String::new(),
        })
    }

    fn process_inner(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<(), CsvLimitError> {
        let mut i = 0;
        let n = input.len();
        while i < n {
            if self.state == State::FieldStart && self.at_line_start && self.pending.is_empty() {
                i = self.plain_lines(input, i, out);
                if i >= n {
                    break;
                }
            }
            let b = input[i];
            match self.state {
                State::FieldStart => {
                    if is_ws(b) {
                        self.pending.push(b);
                        i += 1;
                    } else if b == b'\n' {
                        self.check_columns(input, i + 1)?;
                        if self.at_line_start {
                            if !self.pending.is_empty()
                                && (!self.seen_record || self.keep_whitespace_rows)
                            {
                                // A whitespace-only line where the header
                                // belongs: univocity reads one empty column.
                                out.extend_from_slice(b"\"\"\n");
                                self.end_record();
                            } else {
                                // Blank or whitespace-only line: skipped.
                                out.push(b'\n');
                            }
                        } else {
                            // Empty last value after a delimiter.
                            out.push(b'\n');
                            self.end_record();
                        }
                        self.pending.clear();
                        self.at_line_start = true;
                        i += 1;
                    } else if b == b'#' && self.at_line_start && self.pending.is_empty() {
                        self.state = State::Comment;
                        i += 1;
                    } else {
                        self.pending.clear();
                        self.at_line_start = false;
                        self.seen_record = true;
                        if b == b'"' {
                            self.begin_field(out, true);
                            out.push(b);
                            self.state = State::Quoted;
                        } else if b == b',' {
                            self.check_columns(input, i + 1)?;
                            out.push(b);
                            self.column += 1;
                        } else {
                            self.begin_field(out, false);
                            out.push(b);
                            self.state = State::Unquoted;
                            self.grow(1, input, i + 1, out)?;
                        }
                        i += 1;
                    }
                }
                State::Comment => match memchr_newline(&input[i..]) {
                    Some(offset) => {
                        i += offset + 1;
                        out.push(b'\n');
                        self.state = State::FieldStart;
                        self.at_line_start = true;
                    }
                    None => i = n,
                },
                State::Unquoted => {
                    // Copy the run of ordinary bytes in one go.
                    let start = i;
                    while i < n && !(input[i] <= b' ' || input[i] == b',') {
                        i += 1;
                    }
                    if i > start {
                        out.extend_from_slice(&self.pending);
                        self.pending.clear();
                        out.extend_from_slice(&input[start..i]);
                        // Held-back whitespace was counted when it arrived.
                        self.grow(i - start, input, i, out)?;
                    }
                    if i < n {
                        let b = input[i];
                        if is_ws(b) {
                            self.pending.push(b);
                            i += 1;
                            self.grow(1, input, i, out)?;
                        } else {
                            // Delimiter or line end: trailing whitespace goes.
                            self.check_columns(input, i + 1)?;
                            self.pending.clear();
                            out.push(b);
                            self.state = State::FieldStart;
                            if b == b'\n' {
                                self.end_record();
                                self.at_line_start = true;
                            } else {
                                self.end_field();
                                self.column += 1;
                            }
                            i += 1;
                        }
                    }
                }
                State::Quoted => {
                    let start = i;
                    while i < n && input[i] != b'"' {
                        i += 1;
                    }
                    out.extend_from_slice(&input[start..i]);
                    if i > start {
                        self.grow(i - start, input, i, out)?;
                    }
                    if i < n {
                        out.push(b'"');
                        self.state = State::QuotedQuote;
                        i += 1;
                    }
                }
                State::QuotedQuote => {
                    if b == b'"' {
                        out.push(b);
                        self.state = State::Quoted;
                        i += 1;
                        self.grow(1, input, i, out)?;
                    } else {
                        // The previous quote closed the field; reprocess `b`.
                        self.state = State::AfterQuoted;
                    }
                }
                State::AfterQuoted => {
                    if is_ws(b) {
                        // univocity skips whitespace after the closing quote.
                        i += 1;
                    } else if b == b',' || b == b'\n' {
                        self.check_columns(input, i + 1)?;
                        out.push(b);
                        self.state = State::FieldStart;
                        if b == b'\n' {
                            self.end_record();
                            self.at_line_start = true;
                        } else {
                            self.end_field();
                            self.column += 1;
                        }
                        i += 1;
                    } else {
                        // Stray bytes after a closing quote: both parsers keep
                        // them, in slightly different ways; pass them through.
                        out.push(b);
                        i += 1;
                        self.grow(1, input, i, out)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Flush the end of input: close an open quote, terminate the last record,
    /// and turn a final whitespace-only line into the one-field row univocity
    /// reads there.
    pub fn finish(&mut self, out: &mut Vec<u8>) -> Result<(), CsvLimitError> {
        if let Some(err) = &self.failed {
            return Err(err.clone());
        }
        if !self.bom_done {
            self.bom_done = true;
            let held = std::mem::take(&mut self.bom);
            self.process(&held, out)?;
        }
        // Univocity counts the unterminated final record as the next line.
        if let Err(mut err) = self.check_columns(&[], 0) {
            err.line_index += 1;
            return Err(err);
        }
        match self.state {
            State::Comment => {}
            State::FieldStart => {
                if self.at_line_start {
                    if !self.pending.is_empty() {
                        out.extend_from_slice(b"\"\"\n");
                        self.end_record();
                    }
                } else {
                    out.push(b'\n');
                    self.end_record();
                }
            }
            State::Unquoted | State::QuotedQuote | State::AfterQuoted => {
                out.push(b'\n');
                self.end_record();
            }
            State::Quoted => {
                out.extend_from_slice(b"\"\n");
                self.end_record();
            }
        }
        self.pending.clear();
        self.state = State::FieldStart;
        self.at_line_start = true;
        Ok(())
    }
}

#[inline]
fn memchr_newline(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|&b| b == b'\n')
}

/// Normalise a whole buffer with the default column limit.
pub fn normalize(data: &[u8]) -> Result<Vec<u8>, CsvLimitError> {
    normalize_with_limit(data, Some(DEFAULT_MAX_CHARS_PER_COLUMN))
}

/// Normalise a whole buffer.
pub fn normalize_with_limit(
    data: &[u8],
    max_chars: Option<usize>,
) -> Result<Vec<u8>, CsvLimitError> {
    let mut out = Vec::with_capacity(data.len() + 1);
    let mut normalizer = Normalizer::with_max_chars(max_chars);
    normalizer.push(data, &mut out)?;
    normalizer.finish(&mut out)?;
    Ok(out)
}

/// `Read` adapter applying the same pass to a stream. A value over the limit
/// surfaces as an `io::Error` carrying [`CsvLimitError`].
pub struct NormalizingReader<R> {
    inner: R,
    normalizer: Normalizer,
    input: Vec<u8>,
    output: Vec<u8>,
    cursor: usize,
    eof: bool,
    /// An overflow found while normalising a chunk. The records before it are
    /// handed out first, as univocity parses them before it throws.
    pending_error: Option<CsvLimitError>,
}

impl<R: Read> NormalizingReader<R> {
    const CHUNK: usize = 64 * 1024;

    /// A reader without a value-length limit.
    pub fn new(inner: R) -> Self {
        Self::with_max_chars(inner, None)
    }

    pub fn with_max_chars(inner: R, max_chars: Option<usize>) -> Self {
        Self::with_normalizer(inner, Normalizer::with_max_chars(max_chars))
    }

    pub fn with_normalizer(inner: R, normalizer: Normalizer) -> Self {
        Self {
            inner,
            normalizer,
            input: vec![0; Self::CHUNK],
            output: Vec::with_capacity(Self::CHUNK),
            cursor: 0,
            eof: false,
            pending_error: None,
        }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    fn refill(&mut self) -> io::Result<()> {
        self.output.clear();
        self.cursor = 0;
        if let Some(err) = self.pending_error.take() {
            return Err(err.into_io_error());
        }
        while self.output.is_empty() && !self.eof {
            let read = self.inner.read(&mut self.input)?;
            let result = if read == 0 {
                self.eof = true;
                self.normalizer.finish(&mut self.output)
            } else {
                self.normalizer.push(&self.input[..read], &mut self.output)
            };
            if let Err(err) = result {
                self.eof = true;
                if self.output.is_empty() {
                    return Err(err.into_io_error());
                }
                self.pending_error = Some(err);
            }
        }
        Ok(())
    }
}

impl<R: Read> Read for NormalizingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cursor >= self.output.len() {
            self.refill()?;
            if self.output.is_empty() {
                return Ok(0);
            }
        }
        let available = &self.output[self.cursor..];
        let count = available.len().min(buf.len());
        buf[..count].copy_from_slice(&available[..count]);
        self.cursor += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(input: &str) -> String {
        String::from_utf8(normalize_with_limit(input.as_bytes(), None).unwrap()).unwrap()
    }

    fn norm_stream(
        input: &[u8],
        chunk: usize,
        max: Option<usize>,
    ) -> Result<Vec<u8>, CsvLimitError> {
        struct Chunked<'a>(&'a [u8], usize);
        impl Read for Chunked<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.0.len().min(self.1).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let mut out = Vec::new();
        NormalizingReader::with_max_chars(Chunked(input, chunk), max)
            .read_to_end(&mut out)
            .map_err(|err| {
                CsvLimitError::from_io_error(&err)
                    .cloned()
                    .expect("overflow")
            })?;
        Ok(out)
    }

    #[test]
    fn column_limit_counts_fields_not_quoted_commas_in_every_chunking() {
        let valid = format!("{}\"x,y\"\n", "x,".repeat(511));
        let invalid = format!("a,b\n{}y\n", "x,".repeat(512));
        for chunk in [1, 7, 65536] {
            assert!(norm_stream(valid.as_bytes(), chunk, Some(4096)).is_ok());
            for max in [Some(4096), None] {
                let err = norm_stream(invalid.as_bytes(), chunk, max).unwrap_err();
                assert!(err.too_many_columns);
                assert_eq!(
                    (err.line_index, err.column_index, err.char_index),
                    (2, 513, 1030)
                );
            }
            assert!(
                norm_stream(format!("{}\n", ",".repeat(512)).as_bytes(), chunk, None)
                    .unwrap_err()
                    .too_many_columns
            );
            let eof = norm_stream("x,".repeat(512).as_bytes(), chunk, None).unwrap_err();
            assert!(eof.too_many_columns);
            assert_eq!(eof.line_index, 1);
        }
    }

    #[test]
    fn space_before_quote_opens_the_quote() {
        // Latvia (mdb-992): `817, 9506, "DUS", ...`
        assert_eq!(norm("817, 9506, \"DUS\", x\n"), "817,9506,\"DUS\",x\n");
    }

    #[test]
    fn bare_fields_are_trimmed_on_both_sides() {
        assert_eq!(norm(" a , b ,c \n"), "a,b,c\n");
        assert_eq!(norm("\ta\t,\tb\n"), "a,b\n");
    }

    #[test]
    fn whitespace_inside_quotes_is_kept() {
        assert_eq!(norm("\" a \",\"b \"\n"), "\" a \",\"b \"\n");
    }

    #[test]
    fn escaped_quotes_and_delimiters_inside_quotes_survive() {
        assert_eq!(norm("\"a \"\"b\"\" , c\",d\n"), "\"a \"\"b\"\" , c\",d\n");
        assert_eq!(norm("\"line\nbreak\",x\n"), "\"line\nbreak\",x\n");
        assert_eq!(norm("\"line\r\nbreak\",x\r\n"), "\"line\r\nbreak\",x\n");
    }

    #[test]
    fn whitespace_after_closing_quote_is_dropped() {
        assert_eq!(norm("\"a\"  ,b\n\"c\" \n"), "\"a\",b\n\"c\"\n");
    }

    #[test]
    fn quote_inside_bare_field_is_literal() {
        assert_eq!(norm("ab\"c, d\"e \n"), "ab\"c,d\"e\n");
        assert_eq!(norm("a \"b\",c\n"), "a \"b\",c\n");
    }

    #[test]
    fn carriage_return_is_whitespace_not_a_line_end() {
        // CRLF: the `\r` is trailing whitespace of the last value.
        assert_eq!(norm("a ,b \r\nc\t\r\n"), "a,b\nc\n");
        // A bare `\r` inside a value stays in it; univocity splits on `\n` only.
        assert_eq!(norm("1 x\ry,2\n"), "1 x\ry,2\n");
        assert_eq!(norm("a,b\r1,2\r"), "a,b\r1,2\n");
    }

    #[test]
    fn empty_fields_and_trailing_field() {
        assert_eq!(norm(" , ,\n,\n"), ",,\n,\n");
    }

    #[test]
    fn whitespace_only_lines_are_skipped_after_the_first_record() {
        assert_eq!(norm("a,b\n   \nc,d\n"), "a,b\n\nc,d\n");
        assert_eq!(norm("a,b\n  \r\nc,d\n"), "a,b\n\nc,d\n");
        assert_eq!(norm("a,b\n  \n"), "a,b\n\n");
    }

    #[test]
    fn thorough_mode_keeps_whitespace_only_lines_as_empty_rows() {
        let mut out = Vec::new();
        let mut normalizer = Normalizer::new().keep_whitespace_rows(true);
        normalizer.push(b"a,b\n   \n\nc,d\n", &mut out).unwrap();
        normalizer.finish(&mut out).unwrap();
        assert_eq!(out, b"a,b\n\"\"\n\nc,d\n");
    }

    #[test]
    fn whitespace_only_line_before_the_header_is_an_empty_header() {
        assert_eq!(norm("  \na,b\n"), "\"\"\na,b\n");
        assert_eq!(norm("\r\n"), "\"\"\n");
        assert_eq!(norm("\n  \n\na,b\n"), "\n\"\"\n\na,b\n");
        // A blank line is skipped everywhere.
        assert_eq!(norm("\na,b\n"), "\na,b\n");
    }

    #[test]
    fn whitespace_only_last_line_without_newline_is_an_empty_row() {
        assert_eq!(norm("a,b\n  "), "a,b\n\"\"\n");
        assert_eq!(norm("a,b\n\r"), "a,b\n\"\"\n");
        assert_eq!(norm("   "), "\"\"\n");
    }

    #[test]
    fn every_record_ends_with_a_newline() {
        assert_eq!(norm("a,b\n1,2  "), "a,b\n1,2\n");
        assert_eq!(norm("a,b\n1,"), "a,b\n1,\n");
        assert_eq!(norm("a,b\n\"x\""), "a,b\n\"x\"\n");
        // An unterminated quote runs to the end of input, then closes.
        assert_eq!(norm("a,b\n\"x,2\n"), "a,b\n\"x,2\n\"\n");
        assert_eq!(norm(""), "");
        assert_eq!(norm("\n\n"), "\n\n");
    }

    #[test]
    fn comment_lines_are_emptied() {
        assert_eq!(norm("a,b\n#c,d\n1,2\n"), "a,b\n\n1,2\n");
        assert_eq!(norm("#a,b\nx,y\n"), "\nx,y\n");
        assert_eq!(norm("a,b\n#tail"), "a,b\n");
        // Only a `#` in the first byte of a line starts a comment.
        assert_eq!(norm("a,b\n  #c\n"), "a,b\n#c\n");
        assert_eq!(norm("a,b\n\"x\n#y\",2\n"), "a,b\n\"x\n#y\",2\n");
        assert_eq!(norm("a,b\n1,#2\n"), "a,b\n1,#2\n");
    }

    #[test]
    fn byte_order_mark_is_dropped_once() {
        assert_eq!(
            normalize_with_limit(b"\xEF\xBB\xBF a,b\n", None).unwrap(),
            b"a,b\n"
        );
        assert_eq!(normalize_with_limit(b"\xEF\xBB\xBF", None).unwrap(), b"");
        assert_eq!(
            normalize_with_limit(b"\xEF\xBB\xBF\n", None).unwrap(),
            b"\n"
        );
        assert_eq!(
            normalize_with_limit(b"\xEF\xBBx\n", None).unwrap(),
            b"\xEF\xBBx\n"
        );
        for chunk in 1..4 {
            assert_eq!(
                norm_stream(b"\xEF\xBB\xBFa\n", chunk, None).unwrap(),
                b"a\n"
            );
        }
    }

    #[test]
    fn special_byte_detection_matches_the_byte_rule() {
        for special in [0u8, b'\t', b'\n', b'\r', b' ', b'"'] {
            for pos in 0..8 {
                let mut bytes = *b"abcdefgh";
                bytes[pos] = special;
                assert!(
                    has_special_byte(u64::from_le_bytes(bytes)),
                    "{special} at {pos}"
                );
            }
        }
        for plain in [
            *b"abcdefgh",
            *b"!#,09AZ~",
            [0xC3, 0xA9, 0xF0, 0x9F, 0x98, 0x80, 0x21, 0x7F],
        ] {
            assert!(!has_special_byte(u64::from_le_bytes(plain)));
        }
    }

    #[test]
    fn nul_and_control_bytes_count_as_whitespace() {
        assert_eq!(norm("a\u{0},\u{1}b\n"), "a,b\n");
    }

    #[test]
    fn values_up_to_the_limit_pass() {
        let value = "x".repeat(4096);
        let data = format!("a,b\n{value},2\n\"{value}\",3\n");
        assert!(normalize(data.as_bytes()).is_ok());
        // Leading whitespace is not part of the value.
        let data = format!("a,b\n{}{value},2\n", " ".repeat(5000));
        assert!(normalize(data.as_bytes()).is_ok());
        // An escaped quote is one char.
        let data = format!("a,b\n\"{}\"\"\",2\n", "x".repeat(4095));
        assert!(normalize(data.as_bytes()).is_ok());
        // 2048 astral chars are 4096 UTF-16 units.
        let data = format!("a,b\n{},2\n", "\u{1F600}".repeat(2048));
        assert!(normalize(data.as_bytes()).is_ok());
        let data = format!("a,b\n{},2\n", "\u{e9}".repeat(4096));
        assert!(normalize(data.as_bytes()).is_ok());
    }

    #[test]
    fn value_over_the_limit_reports_univocity_state() {
        // Mirrors the canonical validator on stops.txt: header, one row, then
        // a 4097-char stop_name (charIndex 4220, line 2, column 1, record 1).
        let header =
            "stop_id,stop_name,stop_lat,stop_lon,location_type,parent_station,stop_timezone";
        let data = format!(
            "{header}\nstop1,First Stop,40.7128,-74.0060,0,,\nlong1,{},40.7,-74.0,0,,\n",
            "x".repeat(5000)
        );
        let err = normalize(data.as_bytes()).unwrap_err();
        assert_eq!(err.char_index, 4220);
        assert_eq!(err.line_index, 2);
        assert_eq!(err.column_index, 1);
        assert_eq!(err.record_index, 1);
        assert_eq!(err.parsed_content, "x".repeat(4096));
        for chunk in [1, 7, 64, 4096, 5000] {
            assert_eq!(
                norm_stream(data.as_bytes(), chunk, Some(4096)).unwrap_err(),
                err,
                "chunk {chunk}"
            );
        }
        // No limit, no error.
        assert!(normalize_with_limit(data.as_bytes(), None).is_ok());
    }

    #[test]
    fn overflow_counts_java_chars() {
        let data = format!("a,b\n{},2\n", "\u{e9}".repeat(4097));
        let err = normalize(data.as_bytes()).unwrap_err();
        assert_eq!(err.char_index, 4 + 4097);
        assert_eq!(err.parsed_content.chars().count(), 4096);
        let data = format!("a,b\n{}z,2\n", "\u{1F600}".repeat(2048));
        assert!(normalize(data.as_bytes()).is_err());
        // Trailing and inner whitespace count; so does quoted whitespace.
        let data = format!("a,b\nx{},2\n", " ".repeat(4200));
        assert!(normalize(data.as_bytes()).is_err());
        let data = format!("a,b\nx\"{}\",2\n", " ".repeat(4097));
        assert!(normalize(data.as_bytes()).is_err());
        let data = format!("a,b\n1,\"{}\"\n", " ".repeat(4097));
        let err = normalize(data.as_bytes()).unwrap_err();
        // The opening quote is consumed but is not part of the value.
        assert_eq!(err.char_index, 7 + 4097);
        assert_eq!(err.column_index, 1);
        assert_eq!(err.parsed_content, " ".repeat(4096));
    }

    #[test]
    fn overflow_in_the_header() {
        let data = format!("a,{}\n1,2\n", "h".repeat(5000));
        let err = normalize(data.as_bytes()).unwrap_err();
        assert_eq!(err.char_index, 2 + 4097);
        assert_eq!(
            (err.line_index, err.column_index, err.record_index),
            (0, 1, 0)
        );
    }

    #[test]
    fn streaming_matches_buffered_for_every_chunk_size() {
        let input =
            "\u{feff}h1, h2 ,\"h 3\"\r\n 1 , \"x \"\" y\" ,\"z\"  \n   \n#c\n\"a\nb\" , c\t\r\n  ";
        let expected = normalize(input.as_bytes()).unwrap();
        for chunk in 1..=input.len() {
            assert_eq!(
                norm_stream(input.as_bytes(), chunk, Some(4096)).unwrap(),
                expected,
                "chunk size {chunk}"
            );
        }
    }
}
