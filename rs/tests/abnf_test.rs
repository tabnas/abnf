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
