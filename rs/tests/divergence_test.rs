// Every divergence `../DIVERGENCE.md` records, pinned.
//
// Each case is asserted in BOTH directions: what this port does, and
// what the canonical runtime does that differs. A divergence that
// CLOSES fails here as loudly as one that opens, so the recorded table
// cannot quietly go stale, and the file it records cannot describe a
// behaviour this crate no longer has.
//
// There is no executable register under `test/spec` for these: two of
// the four are invisible to the grammar-to-output comparison every file
// there makes, and the last is about the shape of the API rather than
// about any value. `../DIVERGENCE.md` says so, and this file is what
// stands in for the register.

mod common;

use serde_json::Value as JsonValue;
use tabnas_abnf::{abnf_convert, parse_abnf, AbnfConvertOptions};

/// The fixed-token map a grammar emits.
fn fixed_tokens(src: &str) -> JsonValue {
    let options = AbnfConvertOptions {
        builtins: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(src, Some(&options)).expect("compiles");
    spec.options
        .get("fixed")
        .and_then(|fixed| fixed.get("token"))
        .cloned()
        .unwrap_or(JsonValue::Null)
}

/// DIVERGENCE 1. `%xD800` names one half of a UTF-16 surrogate pair.
/// TypeScript answers a lone surrogate; no Rust `String` can hold one,
/// so the replacement character stands in, as it does in Go.
#[test]
fn a_lone_surrogate_becomes_the_replacement_character() {
    let tokens = fixed_tokens("g = %xD800");
    assert_eq!(
        tokens["#T"], "\u{FFFD}",
        "the canonical runtime emits U+D800 here; see DIVERGENCE.md entry 1"
    );

    // The other direction: a code point that IS a scalar value still
    // arrives unchanged, so this is about surrogates and nothing wider.
    assert_eq!(fixed_tokens("g = %xD7FF")["#T"], "\u{D7FF}");
    assert_eq!(fixed_tokens("g = %xE000")["#T"], "\u{E000}");
    assert_eq!(fixed_tokens("g = %x1F600")["#T"], "\u{1F600}");
}

/// DIVERGENCE 2. A span's offsets count BYTES, where the canonical
/// runtime counts UTF-16 code units.
#[test]
fn span_offsets_count_bytes() {
    // `é` is two bytes and one UTF-16 code unit, so a rule following a
    // literal holding two of them starts at 11 here and at 9 there.
    let src = "a = \"\u{e9}\u{e9}\"\nb = \"x\"\n";
    let grammar = parse_abnf(src).expect("parses");
    let production = grammar
        .productions
        .iter()
        .find(|production| "b" == production.name)
        .expect("the second production");
    let span = production.sp.expect("a span");
    assert_eq!(
        (span.s, span.e),
        (11, 12),
        "the canonical runtime reports 9..10 here; see DIVERGENCE.md entry 2"
    );
    assert_eq!((span.r, span.c), (Some(2), Some(1)));

    // What a consumer actually wants holds in every runtime: slicing the
    // source with the span gives the same TEXT.
    assert_eq!(&src[span.s..span.e], "b");

    // And with nothing non-ASCII in front, the two units agree, so this
    // is about the encoding and not about an off-by-one.
    let plain = "a = \"xx\"\nb = \"x\"\n";
    let grammar = parse_abnf(plain).expect("parses");
    let span = grammar
        .productions
        .iter()
        .find(|production| "b" == production.name)
        .and_then(|production| production.sp)
        .expect("a span");
    assert_eq!((span.s, span.e), (9, 10));
}

/// DIVERGENCE 3. Nested groups are refused sooner than in the canonical
/// runtime, at two caps: the shared compiler's 128 levels of element
/// nesting, and this crate's own limit on the rule stack.
#[test]
fn nested_groups_are_refused_sooner() {
    let nest = |depth: usize| -> String {
        format!("top = {}\"x\"{}", "( ".repeat(depth), " )".repeat(depth))
    };

    // 127 compiles, so the cap is a cap and not a ceiling anyone meets
    // by accident.
    abnf_convert(&nest(127), None).expect("127 nested groups compile");

    // 128 is refused by the shared compiler, which the canonical
    // runtime accepts.
    let error = abnf_convert(&nest(128), None).expect_err("128 is refused");
    assert!(
        error
            .to_string()
            .contains("nests elements more than 128 deep"),
        "got {error}; see DIVERGENCE.md entry 3"
    );

    // The front-end's own cap is further out, and it is what a source
    // deep enough to threaten the stack meets. The canonical runtime
    // raises a catchable range error somewhere past this; Go accepts it.
    let error = parse_abnf(&nest(5000)).expect_err("5000 is refused by the front-end");
    assert!(
        error.to_string().contains("nests too deeply"),
        "got {error}; see DIVERGENCE.md entry 3"
    );
}

/// DIVERGENCE 4. A failure is RETURNED, and there is no instance
/// decoration, so the install path is a free function.
#[test]
fn failures_are_returned_and_there_is_no_decoration() {
    // Returned, not raised, and with the same text the canonical
    // runtime throws.
    let error = parse_abnf("bad = ( \"a\"").expect_err("a malformed grammar is refused");
    assert!(error.to_string().starts_with("abnf: "));

    // The install path takes the engine as an argument, and leaves it
    // carrying the grammar.
    let mut parser = tabnas::Tabnas::new();
    tabnas_abnf::abnf(&mut parser, "greet = \"hi\"", None).expect("installs");
    assert!(parser.rule_names().iter().any(|name| "greet" == name));

    // And the convert-only path leaves an engine alone, which is what
    // the canonical `tn.abnf.toSpec` does.
    let untouched = tabnas::Tabnas::new();
    let before = untouched.rule_names().len();
    abnf_convert("greet = \"hi\"", None).expect("converts");
    assert_eq!(untouched.rule_names().len(), before);
}
