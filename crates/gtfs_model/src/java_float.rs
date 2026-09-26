//! Java Double.parseDouble syntax, shared by CSV validation and deserialization.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

pub fn parse_java_double(input: &str) -> Result<f64, &'static str> {
    let text = crate::java_trim(input);
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let sign = if negative { -1.0 } else { 1.0 };
    match unsigned {
        "NaN" => return Ok(f64::NAN),
        "Infinity" => return Ok(sign * f64::INFINITY),
        _ => {}
    }
    let unsigned = unsigned
        .strip_suffix(['d', 'D', 'f', 'F'])
        .unwrap_or(unsigned);
    if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        return parse_hex(hex).map(|value| sign * value);
    }
    // Rust also accepts inf and case-insensitive nan; Java does not.
    if unsigned.is_empty()
        || unsigned
            .bytes()
            .any(|b| !b.is_ascii_digit() && !b".eE+-".contains(&b))
        || unsigned.starts_with(['+', '-'])
    {
        return Err("invalid Java floating point value");
    }
    unsigned
        .parse::<f64>()
        .map(|v| sign * v)
        .map_err(|_| "invalid Java floating point value")
}

fn parse_hex(text: &str) -> Result<f64, &'static str> {
    let invalid = "invalid Java hexadecimal floating point value";
    let (significand, exponent) = text.split_once(['p', 'P']).ok_or(invalid)?;
    let (negative_exp, exponent) = match exponent.as_bytes().first() {
        Some(b'-') => (true, &exponent[1..]),
        Some(b'+') => (false, &exponent[1..]),
        _ => (false, exponent),
    };
    if exponent.is_empty() || !exponent.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid);
    }
    let exp = exponent
        .bytes()
        .fold(0i64, |e, b| (e * 10 + i64::from(b - b'0')).min(1_000_000));
    let mut exp = if negative_exp { -exp } else { exp };
    let mut dot = false;
    let mut digits = 0;
    let mut bits = 0i64;
    let mut top = 0u64;
    let mut kept = 0;
    let mut sticky = false;
    for b in significand.bytes() {
        if b == b'.' && !dot {
            dot = true;
            continue;
        }
        let digit = (b as char).to_digit(16).ok_or(invalid)?;
        digits += 1;
        if dot {
            exp -= 4;
        }
        for bit in (0..4).rev() {
            let one = (digit >> bit) & 1;
            if bits == 0 && one == 0 {
                continue;
            }
            bits += 1;
            if kept < 54 {
                top = (top << 1) | u64::from(one);
                kept += 1;
            } else {
                sticky |= one != 0;
            }
        }
    }
    if digits == 0 {
        return Err(invalid);
    }
    if bits == 0 {
        return Ok(0.0);
    }
    let e = exp + bits - 1;
    if e > 1023 {
        return Ok(f64::INFINITY);
    }
    if e < -1075 {
        return Ok(0.0);
    }
    // Retain the guard bit and sticky tail, then round once, ties to even.
    // For subnormals retain fewer than 53 bits to avoid double rounding.
    top <<= 54 - kept;
    let precision = (e + 1075).clamp(0, 53) as u32;
    let shift = 54 - precision;
    let mut mantissa = top >> shift;
    let remainder = top & ((1u64 << shift) - 1);
    let half = 1u64 << (shift - 1);
    if remainder > half || (remainder == half && (sticky || mantissa & 1 != 0)) {
        mantissa += 1;
    }
    let encoded = if e < -1022 {
        mantissa
    } else {
        (((e + 1022) as u64) << 52) + mantissa
    };
    Ok(f64::from_bits(encoded))
}

struct JavaDouble(f64);
impl<'de> Deserialize<'de> for JavaDouble {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct DoubleVisitor;
        impl Visitor<'_> for DoubleVisitor {
            type Value = JavaDouble;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Java floating point value")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                parse_java_double(s).map(JavaDouble).map_err(E::custom)
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(JavaDouble(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(JavaDouble(v as f64))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(JavaDouble(v as f64))
            }
        }
        d.deserialize_any(DoubleVisitor)
    }
}
pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    JavaDouble::deserialize(d).map(|v| v.0)
}
pub(crate) fn deserialize_optional<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<f64>, D::Error> {
    Option::<JavaDouble>::deserialize(d).map(|v| v.map(|v| v.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_syntax_and_rounding() {
        for (text, expected) in [
            ("1.5d", 1.5),
            ("-0X1.8P1f", -3.0),
            ("0x1.00000000000008p0", 1.0),
            (
                "0x1.00000000000008001p0",
                f64::from_bits(1.0f64.to_bits() + 1),
            ),
            ("0x1p-1074", f64::from_bits(1)),
            ("0x1p-1075", 0.0),
            ("0x1.0000000000001p-1075", f64::from_bits(1)),
            ("0x1.fffffffffffffp1023", f64::MAX),
            ("0x1.fffffffffffff8p1023", f64::INFINITY),
            ("-0x0p0", -0.0),
            ("\u{0000}+1.5F\r", 1.5),
        ] {
            assert_eq!(
                parse_java_double(text).unwrap().to_bits(),
                expected.to_bits(),
                "{text}"
            );
        }
        for text in [
            "inf",
            "nan",
            "NaNd",
            "Infinityf",
            "--1",
            "+-1",
            "0x1",
            "0xp1",
            "0x1p",
            "0x1p1p1",
            "0x1..2p1",
            "1_000",
            "１.５",
        ] {
            assert!(parse_java_double(text).is_err(), "{text}");
        }
    }
}
