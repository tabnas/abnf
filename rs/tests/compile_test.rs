// Compilation mode: ABNF source to pure-data tabnas grammar text.
// Mirrors `go/compile_test.go` and `ts/test/compile.test.js`.

mod common;

use serde_json::Value as JsonValue;
use tabnas::grammar::BUILTIN_SCHEMA_VERSION;
use tabnas::{Options, RewindOptions, Tabnas};
use tabnas_abnf::{
    abnf_compile, abnf_convert, abnf_rules, to_jsonic, to_pure_spec, to_recognition_spec,
    AbnfCompileOptions, AbnfConvertOptions, JsonicOptions,
};

use common::engine_for;

/// Install strict-jsonic grammar text (which is valid JSON) on a fresh
/// engine.
fn load_jsonic_spec(strict_text: &str) -> Tabnas {
    let options = Options {
        rewind: RewindOptions {
            history: Some(4096),
        },
        ..Options::default()
    };
    let mut parser = Tabnas::with_options(options);
    parser
        .grammar_json(strict_text)
        .unwrap_or_else(|error| panic!("grammar_json: {error}\n---\n{strict_text}"));
    parser
}

fn recognises_jsonic(text: &str, input: &str) -> bool {
    load_jsonic_spec(text).parse(input).is_ok()
}

struct Case {
    name: &'static str,
    src: &'static str,
    accept: &'static [&'static str],
    reject: &'static [&'static str],
}

const RECOGNITION_CASES: &[Case] = &[
    Case {
        name: "greet",
        src: r#"greet = "hi" / "hello""#,
        accept: &["hi", "hello"],
        reject: &["nope", "h"],
    },
    Case {
        name: "pair",
        src: r#"pair = "a" "b""#,
        accept: &["ab"],
        reject: &["a", "ba"],
    },
    Case {
        name: "arith",
        src: "expr = term *(\"+\" term)\nterm = \"(\" expr \")\" / number\nnumber = 1*DIGIT",
        accept: &["1", "1+2", "(1+2)+3"],
        reject: &["1+", "(1"],
    },
];

fn strict() -> AbnfCompileOptions {
    AbnfCompileOptions {
        strict: true,
        ..AbnfCompileOptions::default()
    }
}

#[test]
fn compile_recognition_strict() {
    for case in RECOGNITION_CASES {
        let text = abnf_compile(case.src, &strict()).expect("compiles");
        // The tree builtins must be gone. There is nothing to check about
        // closure refs here: compilation converts with builtins on, and a
        // builtins-mode spec carries no closure refs at all. That case is
        // covered by `to_recognition_spec_drops_closure_hooks`.
        for builtin in ["@node$", "@capture$", "@bubble$"] {
            assert!(
                !text.contains(builtin),
                "{}: recognition spec retained {builtin}:\n{text}",
                case.name
            );
        }
        for input in case.accept {
            assert!(
                recognises_jsonic(&text, input),
                "{}: should accept {input:?}",
                case.name
            );
        }
        for input in case.reject {
            assert!(
                !recognises_jsonic(&text, input),
                "{}: should reject {input:?}",
                case.name
            );
        }
    }
}

#[test]
fn compile_relaxed_format() {
    let text =
        abnf_compile(RECOGNITION_CASES[0].src, &AbnfCompileOptions::default()).expect("compiles");
    assert!(
        text.contains("open:"),
        "expected the bare identifier key 'open:'\n{text}"
    );
    assert!(
        text.contains("'#HI'") || text.contains("'#HELLO'"),
        "expected single-quoted token strings\n{text}"
    );
    assert!(
        !text.contains("\"open\""),
        "keys should not be double-quoted in relaxed mode\n{text}"
    );
}

/// `"hi"` is case-insensitive, so it becomes an eager regex match token,
/// emitted as `@~/^hi/i`.
#[test]
fn compile_eager_regex_serialisation() {
    let text = abnf_compile(r#"greet = "hi""#, &AbnfCompileOptions::default()).expect("compiles");
    assert!(
        text.contains("'@~/^hi/i'"),
        "expected the eager regex '@~/^hi/i' in:\n{text}"
    );
}

#[test]
fn compile_full_keeps_tree_builtins() {
    let full = abnf_compile(
        r#"pair = "a" "b""#,
        &AbnfCompileOptions {
            recognition: false,
            ..AbnfCompileOptions::default()
        },
    )
    .expect("compiles");
    assert!(
        full.contains("@node$"),
        "full mode should retain @node$:\n{full}"
    );

    let recognition =
        abnf_compile(r#"pair = "a" "b""#, &AbnfCompileOptions::default()).expect("compiles");
    for builtin in ["@node$", "@capture$", "@bubble$", "node$", "capture$"] {
        assert!(
            !recognition.contains(builtin),
            "recognition mode should drop {builtin:?}:\n{recognition}"
        );
    }
}

/// The full pure-data AST grammar, round-tripped through jsonic text,
/// builds the SAME `{rule, src, kids}` tree as the live closure grammar.
#[test]
fn compile_full_mode_parity() {
    for (name, src, input) in [
        ("greet", r#"greet = "hi" / "hello""#, "hello"),
        ("pair", r#"pair = "a" "b""#, "ab"),
        (
            "arith",
            "expr = term *(\"+\" term)\nterm = \"(\" expr \")\" / number\nnumber = 1*DIGIT",
            "(1+2)+3",
        ),
        ("probe", "R = [ A \"@\" ] A\nA = 1*ALPHA", "a@b"),
    ] {
        let live = engine_for(src).expect("compiles");
        let live_tree = live.parse(input).expect("live parse").to_json();

        let text = abnf_compile(
            src,
            &AbnfCompileOptions {
                recognition: false,
                strict: true,
                ..AbnfCompileOptions::default()
            },
        )
        .expect("compiles");
        let pure_tree = load_jsonic_spec(&text)
            .parse(input)
            .expect("pure parse")
            .to_json();

        assert_eq!(
            pure_tree, live_tree,
            "{name}: the pure-data tree differs from the live tree"
        );
    }
}

#[test]
fn compile_builtins_no_closures() {
    for src in [
        r#"greet = "hi" / "hello""#,
        r#"pair = "a" "b""#,
        "R = [ A \"@\" ] A\nA = 1*ALPHA",
    ] {
        let options = AbnfConvertOptions {
            builtins: true,
            ..AbnfConvertOptions::default()
        };
        let spec = abnf_convert(src, Some(&options)).expect("compiles");
        assert!(
            spec.refs.is_empty(),
            "builtins conversion left closures: {:?}",
            spec.refs.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn to_pure_rejects_closure_spec() {
    // Closure mode: no builtins.
    let spec = abnf_convert(r#"greet = "hi""#, None).expect("compiles");
    assert!(
        to_pure_spec(&spec).is_err(),
        "to_pure_spec should reject a closure spec"
    );
}

#[test]
fn to_recognition_spec_builtins() {
    let options = AbnfConvertOptions {
        builtins: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(r#"greet = "hi" / "hello""#, Some(&options)).expect("compiles");
    let out = to_recognition_spec(&spec).expect("lowers");
    assert_eq!(out["v"], JsonValue::from(BUILTIN_SCHEMA_VERSION));
    let text = to_jsonic(
        &out,
        JsonicOptions {
            strict: true,
            indent: Some(2),
        },
    );
    for builtin in ["@node$", "@capture$", "@bubble$"] {
        assert!(
            !text.contains(builtin),
            "recognition spec retained the tree builtin {builtin:?}:\n{text}"
        );
    }
    for input in ["hi", "hello"] {
        assert!(recognises_jsonic(&text, input), "should accept {input:?}");
    }
    assert!(!recognises_jsonic(&text, "nope"), "should reject \"nope\"");
}

/// A closure-mode spec's AST hooks are refs into the spec's ref map.
/// Like the canonical transform, those are droppable: recognition still
/// succeeds without them.
#[test]
fn to_recognition_spec_drops_closure_hooks() {
    let spec = abnf_convert(r#"greet = "hi" / "hello""#, None).expect("compiles");
    assert!(
        !spec.refs.is_empty(),
        "a closure-mode spec should carry refs"
    );
    let out = to_recognition_spec(&spec).expect("lowers");
    let text = to_jsonic(
        &out,
        JsonicOptions {
            strict: true,
            indent: Some(2),
        },
    );
    // Ask the spec what its refs are called rather than hard-coding a
    // prefix, so the check cannot drift from the names it checks for.
    for name in spec.refs.keys() {
        assert!(
            !text.contains(name.as_str()),
            "recognition spec leaked the closure ref {name}:\n{text}"
        );
    }
    assert!(recognises_jsonic(&text, "hi"), "should accept \"hi\"");
    assert!(!recognises_jsonic(&text, "nope"), "should reject \"nope\"");
}

/// A probe dispatcher converted WITHOUT builtins keeps its control logic
/// (phase guards, the decision) as closures, which pure recognition data
/// cannot represent.
#[test]
fn to_recognition_spec_rejects_closure_probe() {
    let spec = abnf_convert("R = [ A \"@\" ] A\nA = 1*ALPHA", None).expect("compiles");
    let error = to_recognition_spec(&spec)
        .expect_err("to_recognition_spec should refuse a closure-mode probe spec");
    assert!(
        !error.rules.is_empty(),
        "the error should list the offending rules"
    );
    assert!(
        error.message.contains("probe") || error.message.contains("lookahead"),
        "the message should mention probe or lookahead: {:?}",
        error.message
    );
}

#[test]
fn to_pure_spec_builtins() {
    let options = AbnfConvertOptions {
        builtins: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(r#"pair = "a" "b""#, Some(&options)).expect("compiles");
    let out = to_pure_spec(&spec).expect("lowers");
    assert_eq!(out["v"], JsonValue::from(BUILTIN_SCHEMA_VERSION));
    let text = to_jsonic(
        &out,
        JsonicOptions {
            strict: true,
            indent: Some(2),
        },
    );
    assert!(
        text.contains("@node$"),
        "the pure spec should retain the tree builtins:\n{text}"
    );
    let tree = load_jsonic_spec(&text)
        .parse("ab")
        .expect("pure parse")
        .to_json();
    assert_eq!(tree["rule"], "pair");
    assert_eq!(tree["src"], "ab");
}

/// The meta-grammar's rule table is exported as data, and each call
/// hands over a fresh document so a caller may take it apart without
/// disturbing the parser this crate builds from the same text.
#[test]
fn abnf_rules_exported() {
    let rules = abnf_rules();
    for name in ["abnf", "prod", "alts", "seq", "elem", "atom"] {
        let rule = rules
            .get(name)
            .unwrap_or_else(|| panic!("abnf_rules is missing rule {name:?}"));
        let open = rule
            .get("open")
            .and_then(JsonValue::as_array)
            .unwrap_or_else(|| panic!("rule {name:?} has no open alts"));
        assert!(!open.is_empty(), "rule {name:?} has no open alts");
    }
    let mut taken = rules;
    taken.as_object_mut().expect("an object").remove("abnf");
    assert!(
        abnf_rules().get("abnf").is_some(),
        "abnf_rules should return a fresh document per call"
    );
}
