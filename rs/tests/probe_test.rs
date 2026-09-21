// The probe plus phase-retry disambiguation pattern, mirroring
// `go/probe_test.go` and `ts/test/probe.test.js`.
//
// For a shape like `[X D] Y`, where X and Y share a character
// vocabulary and D is a terminal disambiguator, the shared compiler
// synthesises a dispatcher rule that marks the token position, runs a
// failure-proof `*vocab` probe, peeks the next token, rewinds, and
// commits to the right branch on a retry pass. This suite documents and
// pins that shape.

mod common;

use regex::Regex;
use tabnas_abnf::{abnf_convert, GrammarSpec};

use common::engine_for;

const PROBE_GRAMMAR: &str = "
top = [ X \"@\" ] Y
X   = *( ALPHA )
Y   = *( ALPHA )
";

const AUTHORITY_GRAMMAR: &str = "
authority  = [ userinfo \"@\" ] host [ \":\" port ]
userinfo   = *( unreserved / \":\" )
host       = reg-name
port       = *DIGIT
reg-name   = *( unreserved )
unreserved = ALPHA / \"-\" / \".\"
";

fn rule_names(spec: &GrammarSpec) -> Vec<String> {
    spec.rule.keys().cloned().collect()
}

fn any_match(names: &[String], pattern: &str) -> bool {
    let regex = Regex::new(pattern).expect("the pattern is valid");
    names.iter().any(|name| regex.is_match(name))
}

// ---- the synthetic [X D] Y pattern ----------------------------------

#[test]
fn probe_y_only() {
    let parser = engine_for(PROBE_GRAMMAR).expect("compiles");
    assert!(parser.parse("abc").is_ok());
}

#[test]
fn probe_x_present() {
    let parser = engine_for(PROBE_GRAMMAR).expect("compiles");
    assert!(parser.parse("ab@cd").is_ok());
}

#[test]
fn probe_empty_x_present() {
    let parser = engine_for(PROBE_GRAMMAR).expect("compiles");
    assert!(parser.parse("@cd").is_ok());
}

#[test]
fn probe_empty_input() {
    let parser = engine_for(PROBE_GRAMMAR).expect("compiles");
    assert!(parser.parse(" ").is_ok());
}

// ---- emitter shape ---------------------------------------------------

#[test]
fn probe_emitter_shape() {
    let spec = abnf_convert(PROBE_GRAMMAR, None).expect("compiles");
    let names = rule_names(&spec);
    for (pattern, description) in [
        (r"^top\$pd\d+$", "dispatcher rule top$pdN"),
        (r"^top\$pd\d+\$probe$", "probe helper rule"),
        (r"^top\$pd\d+\$with$", "with-branch rule"),
        (r"^top\$pd\d+\$no$", "no-branch rule"),
    ] {
        assert!(
            any_match(&names, pattern),
            "expected {description}; rule names: {names:?}"
        );
    }
}

/// The optional's body ends with a terminal `!` that cannot be in the
/// tail's (digit) vocabulary, so first-set dispatch suffices and no
/// probe machinery is synthesised.
#[test]
fn probe_no_ambiguity_left_alone() {
    let spec = abnf_convert("top = [ X \"!\" ] Y\nX   = *( ALPHA )\nY   = *DIGIT", None)
        .expect("compiles");
    let dispatcher = Regex::new(r"\$pd\d+").expect("the pattern is valid");
    for name in rule_names(&spec) {
        assert!(
            !dispatcher.is_match(&name),
            "expected no probe helpers; found {name:?}"
        );
    }
}

/// The probe's vocabulary must exclude the disambiguator, or the probe
/// would run straight past the token it exists to find.
#[test]
fn probe_vocab_excludes_disambiguator() {
    let spec = abnf_convert(
        "top = [ X \"@\" ] Y\nX   = *( ALPHA / \"@\" )\nY   = *( ALPHA )",
        None,
    )
    .expect("compiles");
    let probe_rule = rule_names(&spec)
        .into_iter()
        .find(|name| name.ends_with("$probe"))
        .expect("a probe helper rule");
    let fixed = spec
        .options
        .get("fixed")
        .and_then(|fixed| fixed.get("token"))
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let rule = spec.rule[&probe_rule]
        .as_ref()
        .expect("the probe rule has a body");
    for alt in &rule.open {
        let Some(pattern) = alt.s() else { continue };
        let Some(first) = pattern.split_whitespace().next() else {
            continue;
        };
        assert_ne!(
            fixed.get(first).and_then(serde_json::Value::as_str),
            Some("@"),
            "probe vocab must not contain the disambiguator '@'"
        );
    }
}

// ---- RFC 3986 authority-style ambiguity ------------------------------

#[test]
fn probe_authority() {
    let parser = engine_for(AUTHORITY_GRAMMAR).expect("compiles");
    for input in [
        "example.com",
        "example.com:8080",
        "user@example.com",
        "user:pass@example.com",
        "user:pass@example.com:8080",
        "@example.com",
    ] {
        assert!(parser.parse(input).is_ok(), "expected accept {input:?}");
    }
}
