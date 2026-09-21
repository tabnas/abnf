// End-to-end over RFC 3986 Appendix A, the collected ABNF grammar for
// URI. Mirrors `go/rfc3986_test.go` and `ts/test/rfc3986.test.js`, and
// exercises hyphenated rule names, `=/`, alternation, options, groups,
// prefix repetition, `%x` numeric values, case-insensitive literals,
// transitive core-rule inclusion, `;` comments, and the probe plus
// phase-retry dispatcher for the authority ambiguity.

mod common;

use std::fs;

use regex::Regex;
use tabnas_abnf::{abnf_convert, AbnfConvertOptions};

use common::{install, repo_root};

fn rfc_grammar() -> String {
    let path = repo_root()
        .join("ts")
        .join("test")
        .join("grammar")
        .join("rfc3986-uri.abnf");
    fs::read_to_string(&path).expect("the rfc3986-uri.abnf fixture")
}

fn options() -> AbnfConvertOptions {
    AbnfConvertOptions {
        start: Some("URI".to_string()),
        ..AbnfConvertOptions::default()
    }
}

#[test]
fn rfc_compiles() {
    abnf_convert(&rfc_grammar(), Some(&options())).expect("compile RFC 3986");
}

#[test]
fn rfc_every_production_survives() {
    let spec = abnf_convert(&rfc_grammar(), Some(&options())).expect("compiles");
    for name in [
        "URI",
        "hier-part",
        "scheme",
        "authority",
        "userinfo",
        "host",
        "port",
        "IP-literal",
        "IPvFuture",
        "IPv6address",
        "h16",
        "ls32",
        "IPv4address",
        "dec-octet",
        "reg-name",
        "path-abempty",
        "path-absolute",
        "path-rootless",
        "path-empty",
        "segment",
        "segment-nz",
        "pchar",
        "query",
        "fragment",
        "pct-encoded",
        "unreserved",
        "sub-delims",
        "ALPHA",
        "DIGIT",
        "HEXDIG",
    ] {
        assert!(
            spec.rule.contains_key(name),
            "missing rule {name:?} in the emitted spec"
        );
    }
}

#[test]
fn rfc_authority_ambiguity_rewritten() {
    let spec = abnf_convert(&rfc_grammar(), Some(&options())).expect("compiles");
    let names: Vec<String> = spec.rule.keys().cloned().collect();
    for pattern in [
        r"^authority\$pd\d+\$probe$",
        r"^authority\$pd\d+\$with$",
        r"^authority\$pd\d+\$no$",
    ] {
        let regex = Regex::new(pattern).expect("the pattern is valid");
        assert!(
            names.iter().any(|name| regex.is_match(name)),
            "expected a rule matching {pattern:?} (the authority probe rewrite)"
        );
    }
}

#[test]
fn rfc_uri_acceptance() {
    let spec = abnf_convert(&rfc_grammar(), Some(&options())).expect("compiles");
    let parser = install(&spec).expect("installs");
    for uri in [
        "urn:isbn:0451450523",
        "mailto:alice@example.com",
        "tag:yaml.org,2002:int",
        "http://[::1]/",
        "http://example.com",
        "http://example.com:8080",
        "ftp://user@host",
        "http://user@example.com:8080",
        "http://user:pass@example.com:8080/some/path",
        "https://www.example.org/path/to/resource?name=value&other=thing#section",
    ] {
        assert!(parser.parse(uri).is_ok(), "expected accept {uri:?}");
    }
    for uri in ["not a uri", ":foo"] {
        assert!(parser.parse(uri).is_err(), "expected reject {uri:?}");
    }
}
