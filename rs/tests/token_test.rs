// The built-in token terminals. The Rust twin of `ts/test/token.test.js`
// and `go/token_test.go`.
//
// That TypeScript file had no twin in either port until tabnas/abnf#73:
// `word_keywords` and `#TX` appeared nowhere under `rs/tests` or in
// `go/*_test.go`, so either port could have drifted on them with all
// three suites green.
//
// What a shared fixture reaches is pinned there instead, and is NOT
// repeated here: `test/spec/alignment-abnf-ast.tsv` carries every parse
// the TypeScript file asserts (a bareword through TX, a number through
// NR, a quoted string through ST, two adjacent token terminals, a
// repetition over one, the nullable-optional dispatch and a user rule of
// the same name), and `test/spec/alignment-abnf-rules.tsv` carries the
// rule sets. What is left is the emitted terminal itself, which no
// fixture column reads, and `word_keywords`, which is an option no
// fixture passes.

mod common;

use tabnas::{Options, RewindOptions, Tabnas};
use tabnas_abnf::{AbnfConvertOptions, GrammarSpec};

use common::convert_with;

/// The `s` of each opening alternate of one rule.
fn open_tokens(spec: &GrammarSpec, rule: &str) -> Vec<String> {
    let entry = spec
        .rule
        .get(rule)
        .unwrap_or_else(|| panic!("no rule {rule:?} in the emitted spec"))
        .as_ref()
        .unwrap_or_else(|| panic!("rule {rule:?} is a removal, not a definition"));
    entry
        .open
        .iter()
        .map(|alt| alt.s().unwrap_or_default().to_string())
        .collect()
}

fn tagged() -> AbnfConvertOptions {
    AbnfConvertOptions::tag("tk")
}

#[test]
fn a_bare_token_ref_compiles_to_a_terminal() {
    let spec = convert_with("ident = TX", &tagged()).expect("converts");
    assert_eq!(open_tokens(&spec, "ident"), vec!["#TX".to_string()]);
}

#[test]
fn each_builtin_token_terminal_maps_to_its_lexer_token() {
    let spec = convert_with("w = TX\nn = NR\ns = ST\nv = VL", &tagged()).expect("converts");
    for (rule, want) in [("w", "#TX"), ("n", "#NR"), ("s", "#ST"), ("v", "#VL")] {
        assert_eq!(
            open_tokens(&spec, rule),
            vec![want.to_string()],
            "rule {rule} does not open on {want}"
        );
    }
}

/// A user rule of the same name wins over the built-in, so the reference
/// stays a rule push rather than becoming a token terminal. The parse is
/// pinned by the shared AST fixture; what is asserted here is that the
/// emitted spec carries the rule and that `top` does not take the
/// built-in terminal instead.
#[test]
fn a_user_rule_of_the_same_name_wins_over_the_builtin() {
    let spec = convert_with("top = TX\nTX = \"literal\"", &tagged()).expect("converts");
    assert!(
        spec.rule.get("TX").map(Option::is_some).unwrap_or(false),
        "the user's TX rule is not in the emitted spec"
    );
    let top = open_tokens(&spec, "top");
    assert!(
        !top.iter().any(|s| "#TX" == s),
        "rule top opens on the built-in #TX terminal, but the grammar \
         defines TX itself: {top:?}"
    );
}

/// `word_keywords`: a keyword does not grab the prefix of an identifier.
///
/// `map` is a prefix of the identifier `mapping`. Off, the literal
/// matches that prefix and `name` takes the rest; on, the literal only
/// matches a whole word, so the grammar refuses `mapping ;` and still
/// accepts a real `map foo ;`.
#[test]
fn word_keywords_does_not_grab_an_identifier_prefix() {
    const SRC: &str = "decl = \"map\" name \";\"\nname = TX";

    let engine = |word_keywords: bool| -> Tabnas {
        let opts = AbnfConvertOptions {
            word_keywords,
            ..AbnfConvertOptions::tag("tk")
        };
        let spec = convert_with(SRC, &opts)
            .unwrap_or_else(|error| panic!("convert (word_keywords={word_keywords}): {error}"));
        let options = Options {
            rewind: RewindOptions {
                history: Some(4096),
            },
            ..Options::default()
        };
        let mut parser = Tabnas::with_options(options);
        spec.install(&mut parser)
            .unwrap_or_else(|error| panic!("install (word_keywords={word_keywords}): {error}"));
        parser
    };

    assert!(
        engine(false).parse("mapping ;").is_ok(),
        "off: `mapping ;` should parse, the literal taking the prefix and \
         name taking `ping`"
    );
    assert!(
        engine(true).parse("mapping ;").is_err(),
        "on: `mapping ;` should be refused, `map` is not a whole word there"
    );
    let out = engine(true)
        .parse("map foo ;")
        .expect("on: `map foo ;` parses");
    assert_eq!(out.to_json()["src"], serde_json::json!("mapfoo;"));
}
