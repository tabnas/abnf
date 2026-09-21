// The left-recursion elimination pass, mirroring `go/leftrec_test.go`
// and the "left-recursion elimination" suite in `ts/test/abnf.test.js`.
// The fixture-driven positive and equivalence tests live in
// `abnf_test.rs`; these pin the rewrite structure and the refusal.

mod common;

use tabnas_abnf::{abnf_convert, eliminate_left_recursion, parse_abnf, Kind};

use common::engine_for;

/// `P = P alpha / beta` becomes `P = beta (alpha)*`: one alternative
/// whose first element is the seed and whose second is a star of the
/// recursive tail.
#[test]
fn eliminate_left_recursion_rewrite() {
    let grammar = parse_abnf("e = e \"+\" t / t\nt = \"1\"").expect("parses");
    let rewritten = eliminate_left_recursion(&grammar).expect("eliminates");
    let production = rewritten
        .productions
        .iter()
        .find(|production| "e" == production.name)
        .expect("production 'e' survives elimination");
    assert_eq!(production.alts.len(), 1, "e has one alternative");
    let alt = &production.alts[0];
    assert_eq!(alt.len(), 2, "the alternative is seed plus tail");
    // The seed is t's body ("1") inlined: Paull's topologically orders t
    // before e.
    assert!(
        matches!(alt[0].kind, Kind::Term { .. }),
        "seed kind = {:?}, want a term",
        alt[0].kind
    );
    assert!(
        matches!(alt[1].kind, Kind::Star { .. }),
        "tail kind = {:?}, want a star",
        alt[1].kind
    );
}

/// Several recursive and several seed alternatives group correctly: the
/// seed becomes a group of the non-recursive alternatives, and the
/// star's inner a group of the recursive tails.
#[test]
fn eliminate_left_recursion_multiple_alts() {
    let grammar =
        parse_abnf("e = e \"+\" t / e \"-\" t / t / \"(\" e \")\"\nt = \"1\"").expect("parses");
    let rewritten = eliminate_left_recursion(&grammar).expect("eliminates");
    let production = rewritten
        .productions
        .iter()
        .find(|production| "e" == production.name)
        .expect("production 'e' survives elimination");
    assert_eq!(production.alts.len(), 1, "e has one alternative");
    let seed = &production.alts[0][0];
    let star = &production.alts[0][1];
    match &seed.kind {
        Kind::Group { alts } => {
            assert_eq!(alts.len(), 2, "the seed groups both non-recursive alts")
        }
        other => panic!("seed kind = {other:?}, want a group of 2 alts"),
    }
    match &star.kind {
        Kind::Star { inner, .. } => match &inner.kind {
            Kind::Group { alts } => {
                assert_eq!(alts.len(), 2, "the star groups both recursive tails")
            }
            other => panic!("star.inner = {other:?}, want a group of 2 alts"),
        },
        other => panic!("tail kind = {other:?}, want a star"),
    }
}

/// A rule with no seed (non-recursive) alternative cannot be eliminated
/// and is rejected.
///
/// Each port signals failure its own way, and that is not a divergence:
/// the canonical runtime throws and this one returns. The MESSAGE is
/// what both assert, because a rule with no seed and a rule that merely
/// fails to compile are different things and only one of them is this
/// test's subject.
#[test]
fn rejects_purely_left_recursive() {
    match abnf_convert("a = a \"x\"", None) {
        Ok(_) => panic!("a purely left-recursive rule was not rejected"),
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains("purely left-recursive"),
                "rejection = {message:?}, want it to mention 'purely left-recursive'"
            );
        }
    }
}

/// A trivial `P = P` alternative adds nothing and is dropped, leaving
/// the rule's real language intact.
#[test]
fn drops_trivial_self_ref() {
    let parser = engine_for("a = a / \"x\"").expect("compiles");
    assert!(parser.parse("x").is_ok(), "expected accept \"x\"");
    assert!(parser.parse("y").is_err(), "expected reject \"y\"");
}
