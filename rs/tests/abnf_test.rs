// The core converter and parser suite: the Rust port of
// `go/abnf_test.go` and its TypeScript twin `ts/test/abnf.test.js`.
//
// What each group asserts, in order: the AST a compiled grammar builds,
// the EBNF sugar accept/reject boundaries, ABNF numeric values, case
// sensitivity, the start-rule override, the two refusals a front-end
// owns, and the shared `.abnf` fixture grammars both other runtimes
// read.

mod common;

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value as JsonValue};
use tabnas::Tabnas;
use tabnas_abnf::{
    abnf_convert, emit_grammar_spec, parse_abnf, AbnfConvertOptions, AbnfError, AbnfParseError,
    Kind,
};

use common::{engine_for, repo_root};

/// The shared `.abnf` fixture directory both other runtimes read, so the
/// three suites stay in lockstep.
fn fixture(name: &str) -> String {
    let path: PathBuf = repo_root()
        .join("ts")
        .join("test")
        .join("grammar")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read fixture {name}: {error}"))
}

/// An engine carrying the grammar `src` compiles to.
fn parser(src: &str) -> Tabnas {
    engine_for(src).unwrap_or_else(|error| panic!("abnf({src:?}): {error}"))
}

/// The AST shape the tree builtins produce.
fn node(rule: &str, src: &str, kids: Vec<JsonValue>) -> JsonValue {
    json!({ "rule": rule, "src": src, "kids": kids })
}

fn assert_parse(parser: &Tabnas, input: &str, want: JsonValue) {
    match parser.parse(input) {
        Ok(value) => assert_eq!(value.to_json(), want, "parse {input:?}"),
        Err(error) => panic!("parse {input:?}: {error}"),
    }
}

fn assert_accept(parser: &Tabnas, input: &str) {
    if let Err(error) = parser.parse(input) {
        panic!(
            "expected accept {input:?}, got error: {}",
            error.to_string().lines().next().unwrap_or_default()
        );
    }
}

fn assert_reject(parser: &Tabnas, input: &str) {
    if let Ok(value) = parser.parse(input) {
        panic!(
            "expected reject {input:?}, but it parsed as {}",
            value.to_json()
        );
    }
}

fn src_field(value: &JsonValue) -> String {
    value["src"].as_str().unwrap_or_default().to_string()
}

// ---- AST output contract --------------------------------------------

#[test]
fn ast_alternation_of_terminals() {
    let parser = parser(r#"g = "hi" / "hello""#);
    assert_parse(&parser, "hi", node("g", "hi", vec![]));
    assert_parse(&parser, "hello", node("g", "hello", vec![]));
}

#[test]
fn ast_single_terminal() {
    let parser = parser(r#"g = "x""#);
    assert_parse(&parser, "x", node("g", "x", vec![]));
}

/// `p = "a" q` keeps `q` as a kid, because `q` is not at the leading
/// position. `q` is a two-element sequence deliberately: a production
/// whose whole body is a single literal is lifted to a named token, and
/// tokens are not AST nodes.
#[test]
fn ast_ref_as_child_node() {
    let parser = parser("p = \"a\" q\nq = \"b\" \"c\"");
    assert_parse(
        &parser,
        "a bc",
        node("p", "abc", vec![node("q", "bc", vec![])]),
    );
}

/// A production whose whole body is a single literal is LIFTED to a
/// named token: `PL = "+"` becomes the fixed token `#PL`, and no `PL`
/// rule is emitted, because it is lexical rather than syntactic.
///
/// Both halves matter. Emitting the token while leaving the rule behind
/// would satisfy the first assertion while making `PL` syntactic, which
/// is the thing lifting exists to prevent.
#[test]
fn lift_binds_production_name_as_token_name() {
    let grammar = parse_abnf("add = NR [ PL add ]\nPL = \"+\"").expect("parses");
    let spec = emit_grammar_spec(&grammar, None).expect("emits");
    let fixed = spec
        .options
        .get("fixed")
        .and_then(|fixed| fixed.get("token"))
        .and_then(JsonValue::as_object)
        .expect("fixed tokens are emitted");
    assert_eq!(
        fixed.get("#PL"),
        Some(&json!("+")),
        "no #PL fixed token; got {fixed:?}"
    );
    assert!(
        !spec.rule.contains_key("PL"),
        "PL is still a rule: it was not lifted, only copied"
    );
}

/// The emit pipeline rewrites the grammar, and lifting REMOVES the
/// production, so without a defensive copy the second emission would
/// find no `PL` left and drop the token entirely.
#[test]
fn emit_is_repeatable() {
    let grammar = parse_abnf("top = \"x\"\nPL = \"+\"").expect("parses");
    let fixed_of = |spec: &tabnas_abnf::GrammarSpec| -> JsonValue {
        spec.options
            .get("fixed")
            .and_then(|fixed| fixed.get("token"))
            .cloned()
            .unwrap_or(JsonValue::Null)
    };
    let first = emit_grammar_spec(&grammar, None).expect("emit #1");
    let second = emit_grammar_spec(&grammar, None).expect("emit #2");
    assert_eq!(fixed_of(&first)["#PL"], json!("+"));
    assert_eq!(
        fixed_of(&second),
        fixed_of(&first),
        "fixed tokens differ between emissions"
    );
    assert_eq!(
        second.rule.len(),
        first.rule.len(),
        "rule count differs between emissions"
    );
}

/// A pure alias (`v = p`) is not inlined, so it survives as its own node
/// wrapping the rule it names.
#[test]
fn ast_pure_alias_survives() {
    let parser = parser("v = p\np = \"a\" q\nq = \"b\" \"c\"");
    assert_parse(
        &parser,
        "a bc",
        node(
            "v",
            "abc",
            vec![node("p", "abc", vec![node("q", "bc", vec![])])],
        ),
    );
}

#[test]
fn ast_composite_rule() {
    let parser = parser("x = \"x\" name \"=\" value\nname = 1*ALPHA\nvalue = 1*DIGIT");
    assert_parse(
        &parser,
        "xfoo=42",
        node(
            "x",
            "xfoo=42",
            vec![node("name", "foo", vec![]), node("value", "42", vec![])],
        ),
    );
}

#[test]
fn ast_star_with_leading_terminal() {
    let parser = parser("list = \"[\" item *(\",\" item) \"]\"\nitem = 1*ALPHA");
    assert_parse(
        &parser,
        "[a,b,c]",
        node(
            "list",
            "[a,b,c]",
            vec![
                node("item", "a", vec![]),
                node("item", "b", vec![]),
                node("item", "c", vec![]),
            ],
        ),
    );
}

// ---- EBNF desugaring -------------------------------------------------

#[test]
fn ebnf_optional() {
    let parser = parser(r#"g = "hi" [ "there" ]"#);
    assert_accept(&parser, "hi");
    assert_accept(&parser, "hi there");
    assert_reject(&parser, "hi nope");
}

#[test]
fn ebnf_star() {
    let parser = parser(r#"g = *"x" "end""#);
    assert_accept(&parser, "end");
    assert_accept(&parser, "x end");
    assert_accept(&parser, "x x x end");
    assert_reject(&parser, "y end");
}

#[test]
fn ebnf_plus() {
    let parser = parser(r#"g = 1*"x" "end""#);
    assert_accept(&parser, "x end");
    assert_accept(&parser, "x x x end");
    assert_reject(&parser, "end");
}

#[test]
fn ebnf_bounded_rep() {
    let parser = parser(r#"g = 2*4"x" "end""#);
    assert_reject(&parser, "end");
    assert_reject(&parser, "x end");
    assert_accept(&parser, "x x end");
    assert_accept(&parser, "x x x end");
    assert_accept(&parser, "x x x x end");
    assert_reject(&parser, "x x x x x end");
}

#[test]
fn ebnf_exact_rep() {
    let parser = parser(r#"g = 3"x" "end""#);
    assert_reject(&parser, "x x end");
    assert_accept(&parser, "x x x end");
    assert_reject(&parser, "x x x x end");
}

#[test]
fn ebnf_upper_bounded_rep() {
    let parser = parser(r#"g = *2"x" "end""#);
    assert_accept(&parser, "end");
    assert_accept(&parser, "x end");
    assert_accept(&parser, "x x end");
    assert_reject(&parser, "x x x end");
}

// ---- ABNF numeric values ---------------------------------------------

#[test]
fn numeric_hex_single_char() {
    let parser = parser("g = %x61");
    assert_accept(&parser, "a");
    assert_reject(&parser, "b");
}

#[test]
fn numeric_concatenated() {
    let parser = parser("g = %x66.6f.6f");
    assert_accept(&parser, "foo");
    assert_reject(&parser, "bar");
}

#[test]
fn numeric_range() {
    let parser = parser("g = %x30-39");
    assert_accept(&parser, "0");
    assert_accept(&parser, "5");
    assert_accept(&parser, "9");
    assert_reject(&parser, "a");
}

#[test]
fn numeric_range_with_repetition() {
    let parser = parser("g = 1*%x30-39");
    assert_accept(&parser, "1");
    assert_accept(&parser, "12345");
    assert_reject(&parser, "abc");
}

// ---- case sensitivity -------------------------------------------------

#[test]
fn case_insensitive_default() {
    let parser = parser(r#"g = "GET""#);
    assert_accept(&parser, "GET");
    assert_accept(&parser, "get");
}

#[test]
fn case_sensitive_explicit() {
    let parser = parser(r#"g = %s"GET""#);
    assert_accept(&parser, "GET");
    assert_reject(&parser, "get");
}

// ---- start rule -------------------------------------------------------

#[test]
fn start_override() {
    let options = AbnfConvertOptions {
        start: Some("b".to_string()),
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert("a = \"x\"\nb = \"y\"", Some(&options)).expect("converts");
    let parser = common::install(&spec).expect("installs");
    assert_parse(&parser, "y", node("b", "y", vec![]));
}

// ---- refusals ---------------------------------------------------------

#[test]
fn reject_unknown_rule() {
    assert!(
        abnf_convert("g = missing", None).is_err(),
        "expected an error for an unknown rule reference"
    );
}

#[test]
fn reject_no_productions() {
    match abnf_convert("; just a comment\n", None) {
        Ok(_) => panic!("expected an error for no productions"),
        Err(AbnfError::Parse(AbnfParseError { message, .. })) => {
            assert_eq!(message, "abnf: no productions found");
        }
        Err(other) => panic!("expected a parse error, got {other}"),
    }
}

// ---- fixture grammars -------------------------------------------------

#[test]
fn fixture_greet() {
    let parser = parser(&fixture("greet.abnf"));
    assert_parse(&parser, "hi", node("greet", "hi", vec![]));
    assert_parse(&parser, "hello", node("greet", "hello", vec![]));
}

#[test]
fn fixture_pair() {
    let parser = parser(&fixture("pair.abnf"));
    assert_parse(&parser, "ab", node("pair", "ab", vec![]));
}

#[test]
fn fixture_arith() {
    let parser = parser(&fixture("arith.abnf"));
    for input in ["1", "1+2", "1+2*3", "(1+2)*3", "1+2-3"] {
        assert_accept(&parser, input);
    }
    assert_reject(&parser, "1+");
    assert_reject(&parser, "(1");
}

#[test]
fn fixture_arith_leftrec() {
    let parser = parser(&fixture("arith-leftrec.abnf"));
    for input in ["1", "1+2", "1+2*3", "(1+2)*3", "1+2-3"] {
        assert_accept(&parser, input);
    }
    assert_reject(&parser, "1+");
}

/// The left-recursive form recognises the same language as the
/// stratified form: the matched source text is identical for every
/// accepted input.
#[test]
fn fixture_arith_leftrec_equivalence() {
    let stratified = parser(&fixture("arith.abnf"));
    let leftrec = parser(&fixture("arith-leftrec.abnf"));
    for input in ["1", "1+2", "1+2*3", "(1+2)*3"] {
        let first = stratified.parse(input).expect("arith parses");
        let second = leftrec.parse(input).expect("arith-leftrec parses");
        assert_eq!(src_field(&first.to_json()), input);
        assert_eq!(src_field(&second.to_json()), input);
    }
}

#[test]
fn fixture_json_subset() {
    let parser = parser(&fixture("json-subset.abnf"));
    assert_parse(&parser, "1", node("value", "1", vec![]));
    assert_parse(&parser, "a", node("value", "a", vec![]));
    for input in ["{a:1}", "[1,2,3]", "{a:{b:2}}", "[a,b]"] {
        assert_accept(&parser, input);
    }
}

// ---- the plugin form --------------------------------------------------

/// The plugin records itself on the instance and, given an `src` option,
/// converts and installs that grammar. A grammar this crate installs is
/// whatever ABNF the caller hands over, so there is nothing fixed for
/// the plugin to carry.
#[test]
fn plugin_installs_a_source_from_the_option_bag() {
    use indexmap::IndexMap;
    use tabnas::Value;

    let mut options = IndexMap::new();
    options.insert(
        "src".to_string(),
        Value::String("greet = \"hi\" / \"hello\"".to_string()),
    );

    let mut parser = Tabnas::new();
    parser
        .use_plugin(tabnas_abnf::plugin(), Some(Value::object(options)))
        .expect("the plugin installs");
    assert_parse(&parser, "hello", node("greet", "hello", vec![]));

    // With no source, the plugin installs no grammar of its own and
    // leaves the instance as it found it.
    let mut bare = Tabnas::new();
    let before = bare.rule_names();
    bare.use_plugin(tabnas_abnf::plugin(), None)
        .expect("the plugin installs");
    assert_eq!(bare.rule_names(), before);
}

/// A numeric value out of Unicode's range is named back to the author,
/// and the number in that diagnostic is rendered as JavaScript's
/// `String(n)` renders it.
///
/// Two things have to hold for the text to match the canonical runtime.
/// The digits are read the way `parseInt` reads them, rounding ONCE from
/// the exact integer rather than once per digit, which a repeated
/// multiply-and-add in a double does not do. And the double is then
/// printed by ECMAScript's own algorithm, which switches to exponent
/// form at 1e21 and breaks a decimal midpoint to the even digit, neither
/// of which Rust's shortest float form does.
#[test]
fn an_out_of_range_numeric_value_renders_as_javascript_prints_it() {
    for (src, shown) in [
        ("g = %x110000", "1114112"),
        ("g = %d1114112", "1114112"),
        ("g = %d9007199254740993", "9007199254740992"),
        ("g = %d12345678901234567890", "12345678901234567000"),
        ("g = %d99999999999999999999999", "1e+23"),
        ("g = %d1000000000000000000000000000000", "1e+30"),
        ("g = %b111111111111111111111111", "16777215"),
        (
            "g = %xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            "1.461501637330903e+48",
        ),
    ] {
        let error = parse_abnf(src).expect_err("an out-of-range code point is refused");
        let message = error.to_string();
        assert!(
            message.contains(&format!(" is {shown}, which is not a Unicode code point")),
            "{src}: got {message}, want the value shown as {shown}"
        );
    }

    // A digit string long enough to overflow a double reads as Infinity,
    // exactly as `parseInt` answers it.
    let error = parse_abnf(&format!("g = %d{}", "9".repeat(400))).expect_err("refused");
    assert!(error.to_string().contains(" is Infinity, "), "got {error}");
}

/// The platform integer parse truncates at the first digit invalid for
/// the base, and a multi-dash range keeps only the first two parts.
/// Both are canonical behaviours, ported deliberately: Go refuses each
/// of them.
#[test]
fn the_numeric_parse_truncates_as_the_canonical_runtime_does() {
    // `%d5A` is `%d5`, so the grammar compiles and matches "\u{5}".
    let spec = abnf_convert("g = %d5A", None).expect("%d5A is %d5");
    assert_eq!(
        spec.options
            .get("fixed")
            .and_then(|fixed| fixed.get("token"))
            .and_then(|token| token.get("#T"))
            .and_then(JsonValue::as_str),
        Some("\u{5}")
    );

    // `%x41-5A-60` is `%x41-5A`.
    let spec = abnf_convert("g = %x41-5A-60", None).expect("the first two parts are the range");
    let text = serde_json::to_string(&spec.options).expect("options serialise");
    assert!(
        text.contains("u0041") && text.contains("u005a") && !text.contains("u0060"),
        "got {text}"
    );
}

/// The literal a one-element production carries, straight off the IR.
fn term_literal(src: &str) -> String {
    let grammar = parse_abnf(src).expect("parses");
    match &grammar.productions[0].alts[0][0].kind {
        Kind::Term { literal, .. } => literal.clone(),
        other => panic!("{src}: expected a term, got {other:?}"),
    }
}

/// A dotted concatenation is decoded as ONE UTF-16 string, so an
/// adjacent surrogate pair is the character it encodes.
///
/// `%xD800.DC00` is the UTF-16 encoding of U+10000. The canonical
/// runtime builds the parts into one JavaScript string before anything
/// asks what characters it holds, so the halves pair up and the grammar
/// matches the single character U+10000. Converting each part on its own
/// answers two replacement characters instead, which REJECTS the
/// character the grammar names and ACCEPTS a document carrying two
/// U+FFFD.
///
/// This is not `../DIVERGENCE.md` entry 1. That entry is about a code
/// point no Rust `String` can hold; the result here is an ordinary
/// character, so a difference would be a plain defect.
#[test]
fn an_adjacent_surrogate_pair_is_the_character_it_encodes() {
    assert_eq!(term_literal("g = %xD800.DC00"), "\u{10000}");
    assert_eq!(term_literal("g = %xD83D.DE00"), "\u{1F600}");

    let pair = parser("g = %xD800.DC00");
    assert_accept(&pair, "\u{10000}");
    assert_reject(&pair, "\u{FFFD}\u{FFFD}");

    let emoji = parser("g = %xD83D.DE00");
    assert_accept(&emoji, "\u{1F600}");
    assert_reject(&emoji, "\u{FFFD}\u{FFFD}");

    // A pair still pairs with the rest of a longer concatenation around
    // it, because the whole sequence is decoded at once.
    assert_eq!(term_literal("g = %x41.D800.DC00.42"), "A\u{10000}B");

    // What no pairing can rescue is entry 1 again, one replacement per
    // stranded half: a low surrogate first, a high one with an ordinary
    // character after it, and a lone half on either side.
    assert_eq!(term_literal("g = %xDC00.D800"), "\u{FFFD}\u{FFFD}");
    assert_eq!(term_literal("g = %xD800.0041"), "\u{FFFD}A");
    assert_eq!(term_literal("g = %x0041.D800"), "A\u{FFFD}");
    assert_eq!(term_literal("g = %xD800"), "\u{FFFD}");

    // A pair split ACROSS a concatenation boundary is two terms and
    // never one character, in every runtime: `parseNumericValue` runs
    // once per numeric value and nothing joins the results afterwards.
    assert_eq!(term_literal("g = %xD800 %xDC00"), "\u{FFFD}");
    let split = parser("g = %xD800 %xDC00");
    assert_reject(&split, "\u{10000}");
    assert_accept(&split, "\u{FFFD}\u{FFFD}");

    // And nothing outside the surrogate range changed: an astral code
    // point written directly is still itself, and an ordinary dotted
    // concatenation is still its characters.
    assert_eq!(term_literal("g = %x1F600"), "\u{1F600}");
    assert_eq!(term_literal("g = %x0D.0A"), "\r\n");
    assert_eq!(term_literal("g = %x66.6f.6f"), "foo");
}
