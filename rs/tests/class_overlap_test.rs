// Overlapping character classes. Mirrors `go/class_overlap_test.go` and
// `ts/test/class-overlap.test.js`.
//
// The lexer produces ONE token per position and picks it by running the
// matchers the rule expects in allocation order, first match wins. So
// when two class tokens both cover a character, whichever was allocated
// first always won, and every alternative keyed on the other one was
// unreachable. Which alternative died depended only on the order the
// classes happened to be allocated in, which depends on the order the
// productions are visited, so the SAME language written two ways gave
// two different parsers.
//
// The shared compiler lays overlapping classes over a shared partition
// of disjoint atoms and expresses each class as a token set over them,
// so there is nothing left for allocation order to decide.

mod common;

use regex::Regex;
use serde_json::Value as JsonValue;
use tabnas_abnf::abnf_convert;

use common::install;

const DIGIT_FIRST: &str = "top = c\nc = DIGIT / %x31-39 DIGIT\n";
const RANGE_FIRST: &str = "top = c\nc = %x31-39 DIGIT / DIGIT\n";

fn class_accepts(grammar: &str, input: &str) -> bool {
    let Ok(spec) = abnf_convert(grammar, None) else {
        return false;
    };
    let Ok(parser) = install(&spec) else {
        return false;
    };
    parser.parse(input).is_ok()
}

#[test]
fn class_overlap_both_widths() {
    for (label, grammar) in [("DIGIT first", DIGIT_FIRST), ("%x31-39 first", RANGE_FIRST)] {
        assert!(class_accepts(grammar, "8"), "{label}: rejected one digit");
        assert!(class_accepts(grammar, "18"), "{label}: rejected two digits");
        assert!(
            !class_accepts(grammar, "08"),
            "{label}: accepted 08, but the two-digit alt needs 1-9"
        );
        assert!(
            !class_accepts(grammar, "x"),
            "{label}: accepted a non-digit"
        );
    }
}

#[test]
fn class_overlap_order_independent() {
    for input in ["0", "5", "9", "10", "42", "99", "08", "x", ""] {
        assert_eq!(
            class_accepts(DIGIT_FIRST, input),
            class_accepts(RANGE_FIRST, input),
            "alternative order changed the verdict for {input:?}"
        );
    }
}

/// Verbatim from RFC 3986 Appendix A. Standing alone, outside the
/// `"." dec-octet` context that gave its alternatives distinguishing
/// two-token prefixes, every multi-digit octet used to be rejected.
#[test]
fn class_overlap_dec_octet() {
    const GRAMMAR: &str = concat!(
        "top = dec-octet\n",
        "dec-octet = DIGIT\n",
        "          / %x31-39 DIGIT\n",
        "          / \"1\" 2DIGIT\n",
        "          / \"2\" %x30-34 DIGIT\n",
        "          / \"25\" %x30-35\n"
    );
    for value in ["0", "9", "10", "42", "99"] {
        assert!(
            class_accepts(GRAMMAR, value),
            "dec-octet should accept {value:?}"
        );
    }
    for value in ["a", "1a"] {
        assert!(
            !class_accepts(GRAMMAR, value),
            "dec-octet should reject {value:?}"
        );
    }
}

/// The single character span an emitted class matcher covers, or `None`
/// when the pattern is not one.
fn class_span(pattern: &str) -> Option<(u32, u32)> {
    let span = Regex::new(r"^@~/\^?\[\\u([0-9a-fA-F]{4})-\\u([0-9a-fA-F]{4})\]/$")
        .expect("the span pattern is valid");
    let captures = span.captures(pattern)?;
    let read = |index: usize| u32::from_str_radix(captures.get(index)?.as_str(), 16).ok();
    Some((read(1)?, read(2)?))
}

/// `%x30-39` and `%x31-39` overlap, so the atoms are `[0-0]` and `[1-9]`
/// and both classes become sets over them, the second a one-member set,
/// so that its own token name (and every mark derived from it) stays put
/// whatever the partition does underneath. `ALPHA` overlaps nothing and
/// keeps the tokens it has always had.
///
/// This asserts the emitted PARTITION, not just that a set exists. The
/// three ports are separate implementations of the same algorithm, so a
/// regression in one could leave a set in place while its atoms were
/// wrong or overlapping, and a test that only counted sets would stay
/// green through it.
#[test]
fn class_overlap_emits_sets() {
    let spec =
        abnf_convert("top = c\nc = DIGIT / %x31-39 DIGIT / ALPHA\n", None).expect("compiles");
    let sets = spec
        .options
        .get("tokenSet")
        .and_then(JsonValue::as_object)
        .expect("token sets are emitted");
    let tokens = spec
        .options
        .get("match")
        .and_then(|matches| matches.get("token"))
        .and_then(JsonValue::as_object)
        .expect("match tokens are emitted");

    let mut names: Vec<&String> = sets.keys().collect();
    names.sort();
    for name in &names {
        // Keyed WITHOUT the leading `#`: that is the only form every
        // engine resolves.
        assert!(
            !name.starts_with('#'),
            "set key {name:?} must not carry a '#'"
        );
    }
    assert_eq!(
        names.len(),
        2,
        "expected two sets, one per overlapping class, got {names:?}"
    );

    // Each set covers exactly the atoms of the class it was minted for:
    // DIGIT spans both, %x31-39 spans the second alone.
    let spans_of = |set: &str| -> Vec<(u32, u32)> {
        let members = sets[set].as_array().expect("a set is a list");
        let mut spans: Vec<(u32, u32)> = members
            .iter()
            .map(|member| {
                let name = member.as_str().expect("a member is a token name");
                let pattern = tokens
                    .get(name)
                    .and_then(JsonValue::as_str)
                    .unwrap_or_else(|| panic!("set member {name:?} is not a match token"));
                class_span(pattern)
                    .unwrap_or_else(|| panic!("set member {name:?} is not a single-span class"))
            })
            .collect();
        spans.sort();
        spans
    };
    assert_eq!(
        spans_of(names[0]),
        vec![('0' as u32, '0' as u32), ('1' as u32, '9' as u32)]
    );
    assert_eq!(spans_of(names[1]), vec![('1' as u32, '9' as u32)]);

    // Every ATOM in the grammar, not only one set's members: the
    // partition is only a partition if nothing overlaps. Atoms carry the
    // `#RXA` prefix; a class left out of the partition keeps its own
    // single-span token, and two of those may legitimately overlap.
    let mut atoms: Vec<(u32, u32)> = tokens
        .iter()
        .filter(|(name, _)| name.starts_with("#RXA"))
        .filter_map(|(_, pattern)| class_span(pattern.as_str()?))
        .collect();
    atoms.sort();
    for pair in atoms.windows(2) {
        assert!(pair[0].1 < pair[1].0, "atom spans overlap: {atoms:?}");
    }
}

/// `#HELLO` and `#HI` share an `h` but are distinct TOKENS, so the
/// dispatch was never in doubt and needs no lookahead help. Asking the
/// character question here doubled this rule's alternates.
#[test]
fn class_overlap_leaves_distinct_heads_alone() {
    let spec = abnf_convert("greeting = \"hello\" name / \"hi\" name\nname = TX\n", None)
        .expect("compiles");
    let greeting = spec.rule["greeting"]
        .as_ref()
        .expect("the greeting rule has a body");
    assert_eq!(greeting.open.len(), 2, "greeting should have two open alts");
}
