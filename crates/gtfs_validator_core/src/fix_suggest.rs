//! Deriving concrete field replacements and structural repairs for notices
//! whose repair is unambiguous.
//!
//! Syntactic normalizations obey the same two rules:
//!
//! * **It round-trips.** The suggested value is fed back through the very
//!   validator that rejected the original; if it does not pass, no fix is
//!   offered.
//! * **It has exactly one reading.** Anything that needs a guess about intent
//!   is left alone (which casing a name should use, whether `01-05-2026` is
//!   January or May) or, where the guess is narrow and reversible, downgraded to
//!   [`FixSafety::RequiresConfirmation`] so `--fix` alone will not apply it.
//!
//! Structural fixes use optimistic guards while rewriting, and the CLI
//! validates the complete repaired feed again after all edits are applied.
//!
//! Notably absent: `mixed_case_recommended_field`. Title-casing `JFK AIRPORT`
//! produces `Jfk Airport`, and nothing in the data says which words are
//! acronyms, so there is no repair to offer.

use gtfs_guru_model::{GtfsColor, GtfsDate, GtfsTime};
use url::Url;

use crate::csv_validation::is_valid_email;
use crate::feed::{STOP_TIMES_FILE, TRANSLATIONS_FILE};
use crate::notice::{Fix, FixOperation, FixSafety, ValidationNotice};

/// Attach a `ReplaceField` fix aimed at an explicit location.
pub(crate) fn attach_fix(
    notice: &mut ValidationNotice,
    description: &str,
    safety: FixSafety,
    file: &str,
    row: u64,
    field: &str,
    original: &str,
    replacement: String,
) {
    notice.fix = Some(Fix {
        description: description.to_string(),
        safety,
        operation: FixOperation::ReplaceField {
            file: file.to_string(),
            row,
            field: field.to_string(),
            original: original.to_string(),
            replacement,
        },
    });
}

/// Attach a fix using the notice's own file/row/field.
///
/// Does nothing when the notice carries no such location — some rules record it
/// only in context fields, and those must pass the location explicitly through
/// [`attach_fix`] instead.
pub(crate) fn attach_field_fix(
    notice: &mut ValidationNotice,
    description: &str,
    safety: FixSafety,
    original: &str,
    replacement: String,
) {
    let (Some(file), Some(row), Some(field)) =
        (notice.file.clone(), notice.row, notice.field.clone())
    else {
        return;
    };
    attach_fix(
        notice,
        description,
        safety,
        &file,
        row,
        &field,
        original,
        replacement,
    );
}

/// Derive structural repairs from notices that already carry an unambiguous
/// row or file target.
///
/// Structural fixes live here rather than in every foreign-key validator so
/// all producers of the shared `foreign_key_violation` notice get identical
/// behavior. Deleting data is always unsafe; sorting `stop_times.txt` is safe
/// because GTFS does not assign semantics to physical row order.
pub(crate) fn structural_fix(notice: &ValidationNotice) -> Option<Fix> {
    match notice.code.as_str() {
        "unsorted_stop_times" => Some(Fix {
            description: "Group stop times by trip and order them by stop_sequence".into(),
            safety: FixSafety::Safe,
            operation: FixOperation::SortStopTimes {
                file: STOP_TIMES_FILE.into(),
            },
        }),
        "foreign_key_violation" => {
            let file = notice.context.get("childFilename")?.as_str()?;
            let row = notice
                .context
                .get("csvRowNumber")
                .and_then(serde_json::Value::as_u64)
                .or(notice.row)?;
            let field = notice.context.get("childFieldName")?.as_str()?;
            let expected = notice.context.get("fieldValue")?.as_str()?;
            Some(Fix {
                description: "Delete the row that references a missing parent record".into(),
                safety: FixSafety::Unsafe,
                operation: FixOperation::DeleteRow {
                    file: file.into(),
                    row,
                    field: field.into(),
                    expected: expected.into(),
                },
            })
        }
        "translation_foreign_key_violation" => {
            let row = notice.context.get("csvRowNumber")?.as_u64()?;
            let expected = notice.context.get("recordId")?.as_str()?;
            Some(Fix {
                description: "Delete the translation that references a missing record".into(),
                safety: FixSafety::Unsafe,
                operation: FixOperation::DeleteRow {
                    file: TRANSLATIONS_FILE.into(),
                    row,
                    field: "record_id".into(),
                    expected: expected.into(),
                },
            })
        }
        _ => None,
    }
}

/// `#00FF00`, `0x00FF00`, or the three-digit shorthand `0F0` for a field that
/// wants exactly six hex digits.
pub(crate) fn color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let mut digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    digits = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .unwrap_or(digits);

    if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    let candidate = match digits.len() {
        6 => digits.to_string(),
        // CSS shorthand: each digit stands for a doubled pair.
        3 => digits.chars().flat_map(|ch| [ch, ch]).collect(),
        _ => return None,
    };

    accept(trimmed, candidate, |value| GtfsColor::parse(value).is_ok())
}

/// A date written with separators instead of the required `YYYYMMDD`.
///
/// Only a leading four-digit year is accepted. `01-05-2026` is left alone: it
/// could be either day-first or month-first, and dropping the separators turns
/// it into a nonsense date that the round-trip check rejects anyway.
pub(crate) fn date(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split(['-', '/', '.']).map(str::trim).collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0].len() != 4 || parts[1].len() > 2 || parts[2].len() > 2 {
        return None;
    }
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return None;
    }

    let candidate = format!("{}{:0>2}{:0>2}", parts[0], parts[1], parts[2]);
    accept(trimmed, candidate, |value| GtfsDate::parse(value).is_ok())
}

/// A time missing its seconds (`9:05`, `24:00`) or carrying an all-zero
/// fractional part (`07:30:00.000`).
pub(crate) fn time(value: &str) -> Option<String> {
    let trimmed = value.trim();

    let base = match trimmed.split_once('.') {
        Some((head, fraction)) => {
            // A non-zero fraction carries information this format cannot hold.
            if fraction.is_empty() || !fraction.chars().all(|ch| ch == '0') {
                return None;
            }
            head
        }
        None => trimmed,
    };

    let candidate = match base.split(':').count() {
        2 => format!("{base}:00"),
        3 => base.to_string(),
        _ => return None,
    };

    accept(trimmed, candidate, |value| GtfsTime::parse(value).is_ok())
}

/// A URL that is only missing its scheme.
pub(crate) fn url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.contains("://") {
        return None;
    }
    if !(trimmed.contains('.') || trimmed.starts_with("www.")) {
        return None;
    }

    let candidate = format!("https://{trimmed}");
    accept(trimmed, candidate, |value| Url::parse(value).is_ok())
}

/// An address wrapped in `mailto:` or angle brackets.
pub(crate) fn email(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let mut candidate = trimmed;
    if let Some(inner) = candidate
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
    {
        candidate = inner;
    }
    if candidate
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("mailto:"))
    {
        candidate = &candidate[7..];
    }

    accept(trimmed, candidate.trim().to_string(), is_valid_email)
}

/// A comma standing in for a decimal point.
///
/// Skipped when exactly three digits follow it: `1,500` reads as either 1.5 or
/// 1500 depending on locale, and there is nothing in the feed to break the tie.
pub(crate) fn decimal_comma(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let (head, tail) = trimmed.split_once(',')?;
    if tail.len() == 3 || head.is_empty() || tail.is_empty() {
        return None;
    }
    if !is_signed_digits(head) || !tail.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let candidate = format!("{head}.{tail}");
    accept(trimmed, candidate, |value| {
        value.parse::<f64>().is_ok_and(f64::is_finite)
    })
}

/// A whole number written with a redundant fractional part in an integer field.
///
/// The same three-digit rule as [`decimal_comma`] applies, so `12,000` and
/// `12.000` are left alone rather than being read as twelve.
pub(crate) fn whole_number(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let (head, tail) = trimmed.split_once(['.', ','])?;
    if tail.len() == 3 || head.is_empty() || tail.is_empty() {
        return None;
    }
    if !is_signed_digits(head) || !tail.chars().all(|ch| ch == '0') {
        return None;
    }

    let candidate = head.parse::<i64>().ok()?.to_string();
    accept(trimmed, candidate, |value| value.parse::<i64>().is_ok())
}

fn is_signed_digits(value: &str) -> bool {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

/// Keep a candidate only when it actually differs from the input and passes the
/// validator that rejected it.
fn accept(original: &str, candidate: String, is_valid: impl Fn(&str) -> bool) -> Option<String> {
    if candidate == original || !is_valid(&candidate) {
        return None;
    }
    Some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notice::NoticeSeverity;

    #[test]
    fn normalizes_colors() {
        assert_eq!(color("#00FF00").as_deref(), Some("00FF00"));
        assert_eq!(color("0x00ff00").as_deref(), Some("00ff00"));
        assert_eq!(color("#0F0").as_deref(), Some("00FF00"));
        assert_eq!(color("0f0").as_deref(), Some("00ff00"));
    }

    #[test]
    fn leaves_unrecoverable_colors_alone() {
        assert_eq!(color("red"), None);
        assert_eq!(color("#12345"), None);
        assert_eq!(color(""), None);
        // Already valid, so there is nothing to suggest.
        assert_eq!(color("00FF00"), None);
    }

    #[test]
    fn normalizes_separated_dates() {
        assert_eq!(date("2026-01-05").as_deref(), Some("20260105"));
        assert_eq!(date("2026/01/05").as_deref(), Some("20260105"));
        assert_eq!(date("2026.1.5").as_deref(), Some("20260105"));
    }

    #[test]
    fn refuses_ambiguous_or_impossible_dates() {
        // Day-first or month-first? Unknowable, and the round-trip rejects it.
        assert_eq!(date("01-05-2026"), None);
        assert_eq!(date("05/01/2026"), None);
        // A real layout but not a real date.
        assert_eq!(date("2026-02-31"), None);
        assert_eq!(date("2026-13-01"), None);
        assert_eq!(date("20260105"), None);
        assert_eq!(date("not a date"), None);
    }

    #[test]
    fn completes_times() {
        assert_eq!(time("9:05").as_deref(), Some("9:05:00"));
        assert_eq!(time("24:00").as_deref(), Some("24:00:00"));
        assert_eq!(time("07:30:00.000").as_deref(), Some("07:30:00"));
        assert_eq!(time("25:10").as_deref(), Some("25:10:00"));
    }

    #[test]
    fn refuses_lossy_or_invalid_times() {
        // Dropping .500 would silently move the departure.
        assert_eq!(time("07:30:00.500"), None);
        assert_eq!(time("9"), None);
        assert_eq!(time("9:75"), None);
        assert_eq!(time("07:30:00"), None);
        assert_eq!(time("9-05-00"), None);
    }

    #[test]
    fn adds_a_missing_url_scheme() {
        assert_eq!(
            url("www.example.com").as_deref(),
            Some("https://www.example.com")
        );
        assert_eq!(
            url("example.com/feed").as_deref(),
            Some("https://example.com/feed")
        );
        assert_eq!(url("https://example.com"), None);
        assert_eq!(url("nonsense"), None);
    }

    #[test]
    fn unwraps_emails() {
        assert_eq!(
            email("mailto:info@example.com").as_deref(),
            Some("info@example.com")
        );
        assert_eq!(
            email("MailTo:info@example.com").as_deref(),
            Some("info@example.com")
        );
        assert_eq!(
            email("<info@example.com>").as_deref(),
            Some("info@example.com")
        );
        assert_eq!(email("info@example.com"), None);
        assert_eq!(email("not an email"), None);
        // Unwrapping still has to yield something valid.
        assert_eq!(email("mailto:nope"), None);
    }

    #[test]
    fn converts_decimal_commas() {
        assert_eq!(decimal_comma("1,5").as_deref(), Some("1.5"));
        assert_eq!(decimal_comma("-0,25").as_deref(), Some("-0.25"));
        assert_eq!(decimal_comma("12,7539").as_deref(), Some("12.7539"));
    }

    #[test]
    fn refuses_thousands_separator_shaped_numbers() {
        // 1.5 or 1500? Locale decides, and the feed does not say.
        assert_eq!(decimal_comma("1,500"), None);
        assert_eq!(decimal_comma("1,234,567"), None);
        assert_eq!(decimal_comma("1.5"), None);
        assert_eq!(decimal_comma("abc,5"), None);
    }

    #[test]
    fn drops_redundant_fractions_for_integers() {
        assert_eq!(whole_number("12.0").as_deref(), Some("12"));
        assert_eq!(whole_number("12,0").as_deref(), Some("12"));
        assert_eq!(whole_number("-3.00").as_deref(), Some("-3"));
        // Would silently turn twelve thousand into twelve.
        assert_eq!(whole_number("12,000"), None);
        assert_eq!(whole_number("12.5"), None);
        assert_eq!(whole_number("12"), None);
    }

    #[test]
    fn attaches_a_fix_to_a_located_notice() {
        let mut notice = ValidationNotice::new("invalid_color", NoticeSeverity::Error, "bad");
        notice.file = Some("routes.txt".into());
        notice.row = Some(4);
        notice.field = Some("route_color".into());
        attach_field_fix(
            &mut notice,
            "Normalize",
            FixSafety::Safe,
            "#0F0",
            "00FF00".into(),
        );

        let fix = notice.fix.expect("fix attached");
        assert_eq!(fix.safety, FixSafety::Safe);
        let FixOperation::ReplaceField {
            file,
            row,
            field,
            original,
            replacement,
        } = fix.operation
        else {
            panic!("expected field replacement");
        };
        assert_eq!(file, "routes.txt");
        assert_eq!(row, 4);
        assert_eq!(field, "route_color");
        assert_eq!(original, "#0F0");
        assert_eq!(replacement, "00FF00");
    }

    #[test]
    fn skips_a_notice_with_no_field_location() {
        let mut notice = ValidationNotice::new("invalid_color", NoticeSeverity::Error, "bad");
        attach_field_fix(
            &mut notice,
            "Normalize",
            FixSafety::Safe,
            "#0F0",
            "00FF00".into(),
        );
        assert!(notice.fix.is_none());
    }

    #[test]
    fn derives_safe_stop_time_sort() {
        let notice = ValidationNotice::new("unsorted_stop_times", NoticeSeverity::Info, "unsorted");
        let fix = structural_fix(&notice).expect("sort fix");
        assert_eq!(fix.safety, FixSafety::Safe);
        assert_eq!(
            fix.operation,
            FixOperation::SortStopTimes {
                file: STOP_TIMES_FILE.into()
            }
        );
    }

    #[test]
    fn derives_unsafe_orphan_delete() {
        let mut notice =
            ValidationNotice::new("foreign_key_violation", NoticeSeverity::Error, "orphan");
        notice.insert_context_field("childFilename", "stop_times.txt");
        notice.insert_context_field("csvRowNumber", 7_u64);
        notice.insert_context_field("childFieldName", "stop_id");
        notice.insert_context_field("fieldValue", "missing-stop");

        let fix = structural_fix(&notice).expect("delete fix");
        assert_eq!(fix.safety, FixSafety::Unsafe);
        assert_eq!(
            fix.operation,
            FixOperation::DeleteRow {
                file: "stop_times.txt".into(),
                row: 7,
                field: "stop_id".into(),
                expected: "missing-stop".into(),
            }
        );
    }
}
