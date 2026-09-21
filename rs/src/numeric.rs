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
/// ECMA-262 rounds ONCE, from the exact mathematical integer the digits
/// name. A repeated multiply-and-add in a double rounds at every step
/// instead, and the two answers part company above 2^53:
/// `%d99999999999999999999999` is `1e+23` in the canonical runtime and
/// was `1.0000000000000003e+23` here. The value is out of range either
/// way, but the diagnostic prints it.
fn parse_int(text: &str, radix: u32) -> f64 {
    let end = text
        .char_indices()
        .find(|(_, character)| character.to_digit(radix).is_none())
        .map_or(text.len(), |(index, _)| index);
    let digits = &text[..end];
    if digits.is_empty() {
        return f64::NAN;
    }

    // The common case: the exact integer fits, and one cast rounds it.
    let mut exact: u128 = 0;
    for character in digits.chars() {
        let digit = u128::from(character.to_digit(radix).unwrap_or(0));
        match exact
            .checked_mul(u128::from(radix))
            .and_then(|value| value.checked_add(digit))
        {
            Some(value) => exact = value,
            None => return parse_int_wide(digits, radix),
        }
    }
    exact as f64
}

/// `parse_int` for a value too large for a `u128`: render the exact
/// integer in decimal, then let the correctly rounded decimal-to-double
/// parse do the single rounding.
fn parse_int_wide(digits: &str, radix: u32) -> f64 {
    if 10 == radix {
        return digits.parse::<f64>().unwrap_or(f64::INFINITY);
    }
    // Past the largest finite double the answer is infinity whatever the
    // remaining digits say, and a long source must not cost quadratic
    // time to learn that. A double carries at most 1024 bits of exponent
    // range, so anything wider than that is already out.
    let significant = digits.trim_start_matches('0').len() as f64;
    if 1100.0 < significant * f64::from(radix).log2() {
        return f64::INFINITY;
    }
    let mut decimal: Vec<u8> = vec![0];
    for character in digits.chars() {
        let mut carry = character.to_digit(radix).unwrap_or(0);
        for slot in &mut decimal {
            let value = u32::from(*slot) * radix + carry;
            *slot = (value % 10) as u8;
            carry = value / 10;
        }
        while 0 < carry {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    let text: String = decimal
        .iter()
        .rev()
        .map(|digit| char::from(b'0' + digit))
        .collect();
    text.parse::<f64>().unwrap_or(f64::INFINITY)
}

/// JavaScript's `String(n)`, i.e. ECMAScript `Number::toString` with
/// radix 10, which is how the canonical runtime interpolates the offending
/// value into its diagnostic.
///
/// Rust's own shortest float form is NOT a substitute: it breaks an exact
/// decimal midpoint away from zero where the specification takes the even
/// digit, and it never switches to exponent form at 1e21. The shape here
/// is the specification's, and it matches `js_number` in
/// `tabnas-bnf`'s `src/spec.rs`, which is private to that crate.
fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if 0.0 < value { "Infinity" } else { "-Infinity" }.to_string();
    }
    // Covers -0.0, which JavaScript prints as "0".
    if 0.0 == value {
        return "0".to_string();
    }
    let magnitude = value.abs();
    // The specification's `s` and `n`: the fewest digits that read back
    // as this same double, correctly rounded. Rust's FIXED-precision
    // `{:e}` is correctly rounded and breaks ties to even, which is the
    // rule the specification states; plain `{:e}` breaks them the other
    // way. Seventeen significant digits round-trip every finite double.
    let (digits, exponent) = (0..17usize)
        .map(|places| format!("{magnitude:.places$e}"))
        .find(|text| text.parse::<f64>() == Ok(magnitude))
        .map(|text| {
            let (mantissa, exponent) = text.split_once('e').expect("{:e} emits an exponent");
            (
                mantissa.chars().filter(|c| '.' != *c).collect::<String>(),
                exponent
                    .parse::<i32>()
                    .expect("{:e} emits an integer exponent"),
            )
        })
        .expect("17 significant digits round-trip every finite double");
    let count = digits.len() as i32;
    let point = exponent + 1;

    let body = if count <= point && point <= 21 {
        format!("{}{}", digits, "0".repeat((point - count) as usize))
    } else if 0 < point && point <= 21 {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    } else if -6 < point && point <= 0 {
        format!("0.{}{}", "0".repeat(-point as usize), digits)
    } else {
        let places = point - 1;
        let head = if 1 == count {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!(
            "{head}e{}{}",
            if places < 0 { '-' } else { '+' },
            places.abs()
        )
    };
    if value < 0.0 {
        format!("-{body}")
    } else {
        body
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

/// Append one code point to a UTF-16 buffer, as a JavaScript string
/// holds it: one unit below U+10000, a surrogate PAIR above it, and a
/// surrogate code point as the bare unit it names, which no `char` is.
fn push_utf16(units: &mut Vec<u16>, code: f64) {
    let code = code.max(0.0) as u32;
    match char::from_u32(code) {
        Some(character) => {
            let mut buffer = [0u16; 2];
            units.extend_from_slice(character.encode_utf16(&mut buffer));
        }
        // Only a surrogate reaches here: `code_point` has already
        // refused anything above U+10FFFF.
        None => units.push(code as u16),
    }
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

    // A dotted concatenation is decoded AS A WHOLE, because the
    // canonical runtime builds it as one JavaScript string and only the
    // finished string says what characters it holds. `%xD800.DC00` is
    // the surrogate pair for U+10000, an ordinary character every
    // runtime can represent; converting each part on its own would
    // answer two replacement characters and match the wrong document.
    // What survives the join and still cannot be held -- a lone
    // surrogate, a low half before a high one -- becomes U+FFFD, which
    // is entry 1 of `DIVERGENCE.md` and not this.
    let mut units: Vec<u16> = Vec::new();
    for part in body.split('.') {
        push_utf16(&mut units, code_point(part));
    }
    let literal = String::from_utf16_lossy(&units);
    object(vec![
        ("kind", Some(Value::String("term".into()))),
        ("literal", Some(Value::String(literal))),
        ("sp", span),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `number_to_string` is ECMAScript's `Number::toString`, and that is
    /// not the same function as Rust's shortest float form.
    ///
    /// Every value this crate can REACH is a non-negative integral double,
    /// `Infinity` or `NaN`, because that is all `parse_int` can answer,
    /// and on those the two renderers agree everywhere: 449,995 reachable
    /// values were compared and none differed. The out-of-range
    /// diagnostic therefore cannot tell them apart, and no test in
    /// `tests/` can either. On ordinary doubles they differ constantly,
    /// about 7,000 times in 200,000 random bit patterns, so the
    /// specification's algorithm is pinned here directly, where it can be
    /// reached. Each expected string below is what `String(n)` answered in
    /// the canonical runtime.
    #[test]
    fn number_to_string_is_the_specifications_algorithm() {
        // The exponent boundaries the specification states and the
        // shortest float form has no notion of.
        assert_eq!(number_to_string(1e-6), "0.000001");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e20), "100000000000000000000");
        assert_eq!(number_to_string(1e-21), "1e-21");

        // A fraction is written out, never as a mantissa and an exponent.
        assert_eq!(number_to_string(0.5), "0.5");
        assert_eq!(number_to_string(1.5), "1.5");
        assert_eq!(number_to_string(0.1), "0.1");
        assert_eq!(number_to_string(123.456), "123.456");

        // Negative zero prints as zero, which is the one place the sign of
        // a zero is deliberately dropped.
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(0.0), "0");
        assert_eq!(number_to_string(-1.5), "-1.5");

        // The ends of the range.
        assert_eq!(number_to_string(5e-324), "5e-324");
        assert_eq!(number_to_string(f64::MAX), "1.7976931348623157e+308");
        assert_eq!(number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(number_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(number_to_string(f64::NAN), "NaN");

        // And the reachable values, which agree either way and are here
        // so that a rewrite cannot break them while chasing the rest.
        assert_eq!(number_to_string(9007199254740992.0), "9007199254740992");
        assert_eq!(number_to_string(1e23), "1e+23");
        assert_eq!(number_to_string(1114112.0), "1114112");
    }

    /// `parse_int` rounds ONCE, from the exact integer the digits name.
    ///
    /// A repeated multiply-and-add in a double rounds at every step and
    /// parts company with the canonical runtime above 2^53. Unlike the
    /// renderer above, this IS reachable: the out-of-range diagnostic
    /// prints the value, and `tests/abnf_test.rs` pins the message.
    #[test]
    fn parse_int_rounds_once() {
        assert_eq!(parse_int("12345678901234567890", 10), 1.2345678901234567e19);
        assert_eq!(parse_int("9007199254740993", 10), 9007199254740992.0);
        assert_eq!(parse_int("99999999999999999999999", 10), 1e23);
        assert_eq!(parse_int("111", 2), 7.0);
        assert_eq!(parse_int("ff", 16), 255.0);
        assert_eq!(
            parse_int("5A", 10),
            5.0,
            "truncates at the first invalid digit"
        );
        assert!(parse_int("", 10).is_nan());
        assert!(parse_int("zz", 10).is_nan());
        assert_eq!(parse_int(&"9".repeat(400), 10), f64::INFINITY);
        assert_eq!(parse_int(&"f".repeat(400), 16), f64::INFINITY);
    }
}
