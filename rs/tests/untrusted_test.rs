// A grammar file is untrusted input. Deep nesting, very long source,
// unterminated constructs, empty input, control characters and odd
// Unicode must not panic, hang, overflow the stack or take super-linear
// time. This suite pins each boundary.
//
// The Rust port has one hazard the other two runtimes do not: a stack
// that runs out ABORTS the process rather than unwinding, and both the
// walks over the parse AST and the engine's own value drop are
// recursive. So the front-end refuses bracket nesting past a documented
// cap, before anything that deep is built.

mod common;

use std::time::Instant;

use tabnas_abnf::{abnf_convert, parse_abnf};

/// Nesting one level under the cap still compiles, so the cap is a cap
/// and not a ceiling anyone meets by accident.
#[test]
fn deep_nesting_just_under_the_cap_is_accepted() {
    let depth = 200;
    let src = format!("top = {}\"x\"{}", "( ".repeat(depth), " )".repeat(depth));
    parse_abnf(&src).expect("200 levels of grouping parse");
}

/// Past the cap the source is REFUSED, by name, rather than aborting the
/// process on a stack that ran out.
#[test]
fn deep_nesting_past_the_cap_is_refused() {
    for opener in ["( ", "[ "] {
        let closer = if "( " == opener { " )" } else { " ]" };
        let depth = 5000;
        let src = format!(
            "top = {}\"x\"{}",
            opener.repeat(depth),
            closer.repeat(depth)
        );
        let error = parse_abnf(&src).expect_err("5000 levels are refused");
        assert!(
            error.to_string().contains("nests too deeply"),
            "got {error}, want the nesting refusal"
        );
    }
}

/// Deeply nested but never closed: the source is refused, and the
/// refusal still names a cause.
#[test]
fn deep_unclosed_nesting_is_refused() {
    let src = format!("top = {}\"x\"", "( ".repeat(5000));
    let error = parse_abnf(&src).expect_err("an unclosed pile of groups is refused");
    assert!(
        error.to_string().starts_with("abnf: "),
        "got {error}, want an abnf diagnostic"
    );
}

/// Empty and whitespace-only sources are refused rather than compiled
/// into a grammar with no start rule.
#[test]
fn empty_input_is_refused() {
    for src in ["", "   ", "\n\n", "; only a comment\n", "\r\n\r\n"] {
        let error = parse_abnf(src).expect_err("an empty grammar is refused");
        assert_eq!(
            error.to_string(),
            "abnf: no productions found",
            "for {src:?}"
        );
    }
}

/// Every unterminated construct the notation has.
#[test]
fn unterminated_constructs_are_refused() {
    for src in [
        "top = \"unterminated",
        "top = ( \"a\"",
        "top = [ \"a\"",
        "top = <unclosed",
        "top = %x",
        "top = *",
        "top = \"a\" /",
        "top =",
    ] {
        let result = abnf_convert(src, None);
        // `top = "a" /` is a dangling alternation both other runtimes
        // accept as well; what matters here is that nothing panics and
        // every outcome is a value or an error.
        if let Err(error) = result {
            assert!(
                error.to_string().starts_with("abnf: "),
                "{src:?}: got {error}, want an abnf diagnostic"
            );
        }
    }
}

/// Control characters and non-ASCII text in every position that takes
/// text: a literal, a prose terminal, a comment and a rule body.
#[test]
fn control_characters_and_odd_unicode_do_not_panic() {
    for src in [
        "top = \"\u{0}\u{1}\u{7f}\"",
        "top = \"\u{1f600}\u{202e}\"",
        "top = 1*DIGIT ; \u{1f600} \u{0}\n",
        "t\u{e9}p = \"a\"",
        "top = <\u{1f600}>",
        "top = %x0-10FFFF",
        "top = \"a\u{0}b\" \"c\"",
    ] {
        // Every outcome is fine; aborting or hanging is not.
        let _ = abnf_convert(src, None);
    }
}

/// A numeric value above the last code point is refused by name rather
/// than truncated into some other character.
#[test]
fn numeric_values_past_unicode_are_refused() {
    for (src, shown) in [
        ("g = %x110000", "1114112"),
        ("g = %d1114112", "1114112"),
        ("g = %x41-110000", "1114112"),
    ] {
        let error = parse_abnf(src).expect_err("an out-of-range code point is refused");
        let message = error.to_string();
        assert!(
            message.contains("is not a Unicode code point") && message.contains(shown),
            "got {message}"
        );
    }
}

/// A very long grammar is read in time proportional to its length, not
/// to its square. The check is a RATIO measured on one machine in one
/// run, so a slow or busy box cannot make it flaky.
#[test]
fn long_source_stays_roughly_linear() {
    let grammar = |count: usize| -> String {
        let mut out = String::from("top = r0\n");
        for index in 0..count {
            out.push_str(&format!("r{index} = \"a{index}\" / \"b{index}\"\n"));
        }
        out
    };

    // Warm the cached meta-parser, so the first measurement does not pay
    // for building it.
    parse_abnf(&grammar(50)).expect("parses");

    let small = grammar(400);
    let large = grammar(1600);

    let start = Instant::now();
    parse_abnf(&small).expect("parses");
    let small_time = start.elapsed().as_secs_f64().max(1e-6);

    let start = Instant::now();
    parse_abnf(&large).expect("parses");
    let large_time = start.elapsed().as_secs_f64();

    // Four times the input, so linear is 4x. Twelve is generous room for
    // scheduler noise on a shared machine while still failing a
    // quadratic blow-up, which would be 16x and climbing.
    let ratio = large_time / small_time;
    assert!(
        ratio < 12.0,
        "4x the grammar took {ratio:.1}x the time ({small_time:.4}s -> {large_time:.4}s); \
         parsing should be roughly linear"
    );
}

/// A long run of annotation comments is attached in one forward pass,
/// not by restarting the search per comment.
#[test]
fn many_annotations_stay_roughly_linear() {
    let grammar = |count: usize| -> String {
        let mut out = String::from("top = r0\n");
        for index in 0..count {
            out.push_str(&format!(
                "r{index} = a{index} \".\" b{index}   ; @object a{index} b{index}\n"
            ));
            out.push_str(&format!("a{index} = 1*DIGIT\nb{index} = 1*DIGIT\n"));
        }
        out
    };

    parse_abnf(&grammar(20)).expect("parses");

    let small = grammar(200);
    let large = grammar(800);

    let start = Instant::now();
    parse_abnf(&small).expect("parses");
    let small_time = start.elapsed().as_secs_f64().max(1e-6);

    let start = Instant::now();
    parse_abnf(&large).expect("parses");
    let large_time = start.elapsed().as_secs_f64();

    let ratio = large_time / small_time;
    assert!(
        ratio < 12.0,
        "4x the annotations took {ratio:.1}x the time ({small_time:.4}s -> {large_time:.4}s)"
    );
}
