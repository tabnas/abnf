// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

//! RFC 5234 numeric values: `%x41`, `%d65`, `%b1000001`, the `%x41-5A`
//! range and the `%x0D.0A` concatenation.
//!
//! Decoding mirrors `parseNumericValue` in `ts/src/converter.ts`,
//! including its use of the platform integer parse, which reads the
//! longest prefix of digits valid in the stated base rather than
//! refusing the rest. The token matcher is deliberately permissive (hex
//! digits cover the decimal and binary subsets), so `%d5A` reaches here
//! and means `%d5`, as it does in the canonical runtime.

use tabnas::{Token, Value};

use crate::parser_abnf::{record_num_err, span_of};

/// Build an object value from named fields, dropping the absent ones.
fn object(fields: Vec<(&str, Option<Value>)>) -> Value {
    let mut map = indexmap::IndexMap::new();
    for (key, value) in fields {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    Value::object(map)
}

/// Read the longest prefix of `text` that is digits in `radix`, as the
/// canonical `parseInt(text, radix)` does. `NaN` when there is none.
///
/// The accumulation is in the same double the canonical runtime uses, so
/// a value past 2^53 loses the same precision on both sides.
fn parse_int(text: &str, radix: u32) -> f64 {
    let mut value = 0.0_f64;
    let mut seen = false;
    for character in text.chars() {
        match character.to_digit(radix) {
            Some(digit) => {
                value = value * f64::from(radix) + f64::from(digit);
                seen = true;
            }
            None => break,
        }
    }
    if seen {
        value
    } else {
        f64::NAN
    }
}

/// Render a number the way the canonical runtime interpolates one into a
/// message: a whole number below 1e21 in full, anything else in the
/// exponential form with a signed exponent.
fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    if 0.0 == value.fract() && value.abs() < 1e21 {
        return format!("{value}");
    }
    let rendered = format!("{value:e}");
    match rendered.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => rendered,
    }
}

/// The character a code point names.
///
/// A surrogate is not a Rust `char`, and no `String` can hold one. The
/// canonical runtime's strings are UTF-16 and can, so `%xD800` is a lone
/// surrogate there and the replacement character here, as it is in the
/// Go port. `DIVERGENCE.md` records it.
fn character(code: f64) -> char {
    u32::try_from(code as i64)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{FFFD}')
}

/// Decode an ABNF numeric value into an IR element.
///
/// RFC 5234 puts no ceiling on a numeric value, but Unicode does:
/// nothing above U+10FFFF is a code point. The check happens here so an
/// out-of-range grammar gets an ABNF diagnostic naming the offending
/// value rather than a silent replacement character. The message is
/// recorded rather than returned, because the caller is an engine alt
/// action whose error channel would add a position the canonical
/// diagnostic does not carry.
pub(crate) fn parse_numeric_value(src: &str, token: Option<&Token>) -> Value {
    let span = span_of(token);
    let base_char = src.chars().nth(1).unwrap_or('x');
    let base = base_char.to_ascii_lowercase();
    let radix = match base {
        'd' => 10,
        'b' => 2,
        _ => 16,
    };
    let body: String = src.chars().skip(2).collect();

    let code_point = |text: &str| -> f64 {
        let value = parse_int(text, radix);
        if !value.is_finite() || value < 0.0 || (0x10FFFF as f64) < value {
            record_num_err(format!(
                "numeric value '%{base_char}{text}' is {}, which is not a Unicode code point \
                 (the maximum is %x10FFFF).",
                number_to_string(value)
            ));
            return 0.0;
        }
        value
    };

    if body.contains('-') {
        let mut parts = body.split('-');
        let low = code_point(parts.next().unwrap_or_default());
        let high = code_point(parts.next().unwrap_or_default());
        if low == high {
            return object(vec![
                ("kind", Some(Value::String("term".into()))),
                ("literal", Some(Value::String(character(low).to_string()))),
                ("sp", span),
            ]);
        }
        // A four-digit escape only reaches U+FFFF, so a range that runs
        // above the basic plane has to be written with the braced form,
        // which in the canonical dialect needs the `u` flag with it.
        // Everything below stays on the plain form, so existing output
        // is unchanged.
        let astral = (0xFFFF as f64) < low || (0xFFFF as f64) < high;
        let escape = |value: f64| -> String {
            let code = value as u64;
            if astral {
                format!("\\u{{{code:x}}}")
            } else {
                format!("\\u{code:04x}")
            }
        };
        return object(vec![
            ("kind", Some(Value::String("regex".into()))),
            (
                "pattern",
                Some(Value::String(format!("[{}-{}]", escape(low), escape(high)))),
            ),
            (
                "flags",
                Some(Value::String(if astral { "u" } else { "" }.to_string())),
            ),
            ("sp", span),
        ]);
    }

    let literal: String = body
        .split('.')
        .map(|part| character(code_point(part)))
        .collect();
    object(vec![
        ("kind", Some(Value::String("term".into()))),
        ("literal", Some(Value::String(literal))),
        ("sp", span),
    ])
}
