// Source spans: the front-end records where each element and production
// came from, so a compile failure carries a range and a tool can
// underline the offending text. Mirrors `go/spans_test.go` and the
// `source spans` suite in `ts/test/abnf.test.js`.
//
// Every assertion SLICES THE ORIGINAL SOURCE with the span and compares
// the text. That is the only check worth making: an offset pair that is
// self-consistent but points at the wrong characters would satisfy any
// assertion about the numbers themselves.

mod common;

use tabnas_abnf::{
    abnf_convert, parse_abnf, AbnfError, Grammar, Kind, NodeKind, Production, SrcSpan,
};

const SPAN_SRC: &str = concat!(
    "doc  = item\n",
    "item = \"hi\" / %s\"Yo\" / ref / (alt / two) / [opt] / %x41-5A / <free>\n",
    "ref  = ALPHA\n",
    "alt  = \"a\"\n",
    "two  = \"b\"\n",
    "opt  = \"c\""
);

fn span_grammar() -> Grammar {
    parse_abnf(SPAN_SRC).expect("the span fixture parses")
}

fn span_text(span: Option<SrcSpan>) -> String {
    let span = span.expect("expected a span, got none");
    SPAN_SRC[span.s..span.e].to_string()
}

fn span_prod(name: &str) -> Production {
    span_grammar()
        .productions
        .into_iter()
        .find(|production| name == production.name)
        .unwrap_or_else(|| panic!("no production {name:?}"))
}

#[test]
fn production_spans_its_name() {
    let production = span_prod("item");
    assert_eq!(span_text(production.sp), "item");
    let span = production.sp.expect("a span");
    assert_eq!(
        (span.r, span.c),
        (Some(2), Some(1)),
        "row/col are 1-based, as the engine reports them"
    );
}

#[test]
fn string_terminal_spans_include_any_prefix() {
    let alts = span_prod("item").alts;
    assert_eq!(span_text(alts[0][0].sp), "\"hi\"");
    assert_eq!(span_text(alts[1][0].sp), "%s\"Yo\"");
}

#[test]
fn ref_numeric_and_prose_spans() {
    let alts = span_prod("item").alts;
    assert_eq!(span_text(alts[2][0].sp), "ref");
    assert_eq!(span_text(alts[5][0].sp), "%x41-5A");
    // Prose too. This assertion is why the fixture carries a `<free>`
    // alternative at all, and it is the one the name of this test has
    // always claimed: a prose element carrying no span leaves the shared
    // compiler nothing to underline, and that failure is one the
    // front-end is supposed to place.
    assert_eq!(span_text(alts[6][0].sp), "<free>");
}

#[test]
fn group_and_optional_span_their_delimiters() {
    let alts = span_prod("item").alts;

    let group = &alts[3][0];
    let Kind::Group { alts: inner } = &group.kind else {
        panic!("expected a group, got {:?}", group.kind);
    };
    assert_eq!(span_text(group.sp), "(alt / two)");
    assert_eq!(span_text(inner[0][0].sp), "alt");
    assert_eq!(span_text(inner[1][0].sp), "two");

    let optional = &alts[4][0];
    let Kind::Opt { inner } = &optional.kind else {
        panic!("expected an opt, got {:?}", optional.kind);
    };
    assert_eq!(span_text(optional.sp), "[opt]");
    assert_eq!(span_text(inner.sp), "[opt]");
}

/// The RFC 5234 core rules are parsed from a string inside the
/// converter, not from the user's grammar, so any offset would index a
/// document the user never wrote. A missing span means "nowhere to
/// point", which is right; a wrong one is worse than none.
#[test]
fn core_rules_carry_no_span() {
    let alpha = span_prod("ALPHA");
    assert_eq!(alpha.node_kind, NodeKind::Core);
    assert!(
        alpha.sp.is_none(),
        "a core rule must carry no span, got {:?}",
        alpha.sp
    );
    for alt in &alpha.alts {
        for element in alt {
            assert!(
                element.sp.is_none(),
                "a core rule element must carry no span, got {:?}",
                element.sp
            );
        }
    }
    // ...but the user's REFERENCE to it does: that reference is in their
    // source, and it is what a diagnostic points at.
    assert_eq!(span_text(span_prod("ref").alts[0][0].sp), "ALPHA");
}

/// The core-rule list is handed to EVERY grammar parsed in this process,
/// so each parse has to get its own copy. Without one, a consumer that
/// annotated an `ALPHA` node would see the annotation on unrelated
/// documents, and the "core rules carry no span" guarantee above would
/// hold only until someone broke it for everyone.
///
/// Here the copy is enforced by ownership rather than by care:
/// `core_rules` hands back a `&'static Vec<Production>` and
/// `with_core_rules` clones out of it, so sharing would not compile.
/// That is the mechanism, not the contract. What is pinned is the
/// ANSWER, the same one `ts/test/abnf.test.js` pins under "hands out a
/// fresh copy of each core rule", where the cache is a module-level
/// object a returned reference really can reach.
#[test]
fn hands_out_a_fresh_copy_of_each_core_rule() {
    let alpha_at = |grammar: &Grammar| {
        grammar
            .productions
            .iter()
            .position(|production| "ALPHA" == production.name)
            .expect("ALPHA should have been pulled in")
    };

    let mut first = parse_abnf("doc = ALPHA").expect("the first parse succeeds");
    let at = alpha_at(&first);
    let planted = SrcSpan {
        s: 999,
        e: 1000,
        r: Some(42),
        c: Some(1),
    };
    first.productions[at].sp = Some(planted);
    first.productions[at].alts[0][0].sp = Some(planted);

    let second = parse_abnf("doc = ALPHA").expect("the second parse succeeds");
    let at = alpha_at(&second);
    assert_eq!(
        second.productions[at].sp, None,
        "a mutation leaked between parses"
    );
    assert_eq!(
        second.productions[at].alts[0][0].sp, None,
        "an element mutation leaked between parses"
    );
}

/// A span whose offset and row/column disagree is worse than no span: a
/// consumer picking either one gets a different answer.
#[test]
fn span_row_and_column_agree_with_the_offset() {
    for production in span_grammar().productions {
        // Core rules carry none, deliberately.
        let Some(span) = production.sp else { continue };
        let before = &SPAN_SRC[..span.s];
        let row = before.matches('\n').count() + 1;
        let column = span.s - before.rfind('\n').map_or(0, |at| at + 1) + 1;
        assert_eq!(
            span.r,
            Some(row),
            "{}: row disagrees with the offset",
            production.name
        );
        assert_eq!(
            span.c,
            Some(column),
            "{}: column disagrees with the offset",
            production.name
        );
    }
}

#[test]
fn compile_error_carries_a_range() {
    let src = "doc = item\nitem = missing";
    let error = abnf_convert(src, None).expect_err("an unknown-rule failure");
    let AbnfError::Emit(emit) = &error else {
        panic!("expected an emit error, got {error}");
    };
    let span = emit.sp.expect("the compile error carries a range");
    assert_eq!(&src[span.s..span.e], "missing");
    assert_eq!(span.r, Some(2));
}

/// The fixture's `<free>` alternative is itself a compile failure, since
/// prose may only stand alone as the whole definition of a built-in
/// lexer token. It is one of the failures the front-end gives a range,
/// and the range has to cover the prose rather than the whole rule: the
/// shared compiler falls back to the production span when the element
/// carries none, so a front-end that spanned productions only would
/// still answer a range here, just a useless one. Mirrors the TS
/// "ranges a prose-in-expression failure too".
#[test]
fn prose_in_expression_failure_carries_a_range() {
    let error = abnf_convert(SPAN_SRC, None).expect_err("a prose-in-expression failure");
    let AbnfError::Emit(emit) = &error else {
        panic!("expected an emit error, got {error}");
    };
    let span = emit
        .sp
        .expect("the prose-in-expression failure carries a range");
    assert_eq!(&SPAN_SRC[span.s..span.e], "<free>");
}

#[test]
fn spans_do_not_reach_the_emitted_grammar() {
    let spec = abnf_convert(
        "doc = item\nitem = \"hi\" / (a / b)\na = \"x\"\nb = ALPHA",
        None,
    )
    .expect("compiles");
    let raw = serde_json::to_string(&spec.to_value()).expect("serialises");
    assert!(
        !raw.contains("\"sp\"") && !raw.contains("\"Sp\""),
        "a span reached the emitted grammar:\n{raw}"
    );
}
