// Value annotations carried in ABNF comments. Mirrors
// `go/value_annotation_test.go` and `ts/test/value-annotation.test.js`.
//
// ```abnf
// ver = maj "." min "." pat    ; @object maj min pat
// ```
//
// RFC 5234 has nowhere else to put this. A comment is the only place in
// the notation that carries no meaning of its own, which is exactly why
// it can carry one here without changing what the grammar accepts:
// strip every annotation and the same language parses, just into a tree
// instead of a value. These tests assert that both halves hold.

mod common;

use serde_json::{json, Value as JsonValue};
use tabnas::Tabnas;
use tabnas_abnf::{abnf_convert, parse_abnf, AbnfConvertOptions, ValueAnnotation};

const VER_SRC: &str = concat!(
    "ver = maj \".\" min \".\" pat   ; @object maj min pat\n",
    "maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n"
);

fn options(start: &str) -> AbnfConvertOptions {
    AbnfConvertOptions {
        start: Some(start.to_string()),
        ..AbnfConvertOptions::default()
    }
}

/// Compile `src`, install it, parse `input`, and hand back what the
/// grammar BUILT.
fn annot_build(src: &str, input: &str, start: &str) -> JsonValue {
    let spec =
        abnf_convert(src, Some(&options(start))).unwrap_or_else(|error| panic!("convert: {error}"));
    let mut parser = Tabnas::new();
    spec.install(&mut parser)
        .unwrap_or_else(|error| panic!("install: {error}"));
    parser
        .parse(input)
        .unwrap_or_else(|error| panic!("parse {input:?}: {error}"))
        .to_json()
}

fn annot_accepts(src: &str, input: &str, start: &str) -> bool {
    let Ok(spec) = abnf_convert(src, Some(&options(start))) else {
        return false;
    };
    let mut parser = Tabnas::new();
    if spec.install(&mut parser).is_err() {
        return false;
    }
    parser.parse(input).is_ok()
}

fn annot_value_of(src: &str, name: &str) -> Option<ValueAnnotation> {
    let grammar = parse_abnf(src).unwrap_or_else(|error| panic!("parse: {error}"));
    grammar
        .productions
        .into_iter()
        .find(|production| name == production.name)
        .and_then(|production| production.value)
}

#[test]
fn annotation_builds_object() {
    assert_eq!(
        annot_build(VER_SRC, "1.2.30", "ver"),
        json!({ "maj": "1", "min": "2", "pat": "30" })
    );
}

#[test]
fn annotation_reads_into_the_ir() {
    assert_eq!(
        annot_value_of(VER_SRC, "ver"),
        Some(ValueAnnotation::object(["maj", "min", "pat"]))
    );
    assert_eq!(
        annot_value_of(VER_SRC, "maj"),
        None,
        "only the annotated rule carries one"
    );
}

/// The point of putting this in a comment: the same source minus the
/// annotation must still parse the same inputs. It just produces the
/// tree it always did.
#[test]
fn annotation_changes_what_is_built_not_what_is_accepted() {
    const PLAIN: &str = concat!(
        "ver = maj \".\" min \".\" pat\n",
        "maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n"
    );
    for input in ["1.2.30", "11.22.33", "1.2", "x", ""] {
        assert_eq!(
            annot_accepts(VER_SRC, input, "ver"),
            annot_accepts(PLAIN, input, "ver"),
            "the annotation changed whether {input:?} parses"
        );
    }
    let tree = annot_build(PLAIN, "1.2.30", "ver");
    assert_eq!(tree["rule"], "ver");
    assert_eq!(tree["src"], "1.2.30");
}

#[test]
fn annotation_nests_an_annotated_member() {
    const SRC: &str = concat!(
        "top = name \"=\" inner    ; @object name inner\n",
        "name = 1*ALPHA\n",
        "inner = maj \".\" min    ; @object maj min\n",
        "maj = 1*DIGIT\nmin = 1*DIGIT\n"
    );
    assert_eq!(
        annot_build(SRC, "ab=1.2", "top"),
        json!({ "name": "ab", "inner": { "maj": "1", "min": "2" } })
    );
}

#[test]
fn annotation_builds_array() {
    const SRC: &str = "top = a \",\" b   ; @array\na = 1*DIGIT\nb = 1*DIGIT\n";
    assert_eq!(annot_build(SRC, "1,2", "top"), json!(["1", "2"]));
}

/// A rule written across several lines, with the annotation on the last
/// of them, means the same thing.
#[test]
fn annotation_follows_the_rule_not_the_line() {
    const SRC: &str = concat!(
        "ver = maj \".\" min\n",
        "                        ; @object maj min\n",
        "maj = 1*DIGIT\nmin = 1*DIGIT\n"
    );
    assert_eq!(
        annot_value_of(SRC, "ver"),
        Some(ValueAnnotation::object(["maj", "min"]))
    );
}

/// `";@object x"` is a LITERAL semicolon, not a comment. Treating it as
/// one would attach an annotation the author never wrote.
#[test]
fn annotation_ignores_a_semicolon_inside_a_string() {
    const SRC: &str = "top = sep 1*DIGIT\nsep = \";@object x\"\n";
    assert_eq!(annot_value_of(SRC, "top"), None);
    assert_eq!(annot_value_of(SRC, "sep"), None);
    assert!(
        annot_accepts(SRC, ";@object x1", "top"),
        "and it should still parse"
    );
}

/// The notation has no directive namespace, so this must not assume one:
/// a reader's own `; @deprecated` has to keep meaning nothing.
#[test]
fn annotation_leaves_other_directives_alone() {
    assert_eq!(
        annot_value_of("top = 1*DIGIT   ; @deprecated use ver instead\n", "top"),
        None
    );
}

#[test]
fn annotation_refusals() {
    for (label, src, want) in [
        (
            "before any rule",
            "; @object a b\ntop = 1*DIGIT\n",
            "before any rule",
        ),
        (
            "two on one rule",
            concat!(
                "top = a \".\" b   ; @object a b\n                ; @object a b\n",
                "a = 1*DIGIT\nb = 1*DIGIT\n"
            ),
            "more than one value annotation",
        ),
        (
            "@array naming members",
            "top = a \",\" b   ; @array a b\na = 1*DIGIT\nb = 1*DIGIT\n",
            "'@array' names no members",
        ),
        (
            "a member that is not a rule name",
            "top = a \".\" b   ; @object a 9nope\na = 1*DIGIT\nb = 1*DIGIT\n",
            "is not a rule name",
        ),
    ] {
        match parse_abnf(src) {
            Ok(_) => panic!("{label}: expected a refusal, got none"),
            Err(error) => assert!(
                error.to_string().contains(want),
                "{label}: got {:?}, want it to mention {want:?}",
                error.to_string()
            ),
        }
    }
}

/// The refusals above are the front-end's own: they are about the
/// COMMENT, and parsing raises them. These come from the shared compiler
/// underneath, and are here because they are what an ABNF author
/// actually hits: the comment is well formed, the grammar is not. They
/// must reach the author in ABNF's own words, never as "bnf:".
#[test]
fn annotation_compiler_refusals() {
    for (label, src, want) in [
        // A rule's first reference is folded into it, which erases that
        // rule's builders: the member would hold an internal node.
        (
            "a leading member that builds a value",
            concat!(
                "top = inner \",\" x   ; @object inner x\n",
                "inner = a \".\" b     ; @object a b\n",
                "a = 1*DIGIT\nb = 1*DIGIT\nx = 1*DIGIT\n"
            ),
            "erases the value 'inner' is annotated to build",
        ),
        // The erasure needs a LEADING reference, not an annotated caller.
        (
            "an unannotated rule that inlines an annotated one",
            "top = leaf \",\"\nleaf = d   ; @object d\nd = 1*DIGIT\n",
            "erases the value 'leaf' is annotated to build",
        ),
        // A group produces a value, so it is a member and must be named,
        // but a member name has to be a rule name and a group has none.
        (
            "a group that cannot be named",
            concat!(
                "top = ( a / b ) c   ; @object c\n",
                "a = 1*DIGIT\nb = 1*ALPHA\nc = 1*DIGIT\n"
            ),
            "names 1 member but has 2 parts that produce a value",
        ),
    ] {
        match abnf_convert(src, None) {
            Ok(_) => panic!("{label}: expected a refusal, got none"),
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.starts_with("abnf: "),
                    "{label}: the diagnostic must name ABNF, got {message:?}"
                );
                assert!(
                    message.contains(want),
                    "{label}: got {message:?}, want it to mention {want:?}"
                );
            }
        }
    }
}

/// A repetition is COLLECTED into the array, one element per item, so
/// the ABNF list idiom builds a list.
#[test]
fn annotation_collects_a_repetition() {
    const SRC: &str = "list = item *( \",\" item )   ; @array\nitem = 1*DIGIT\n";
    assert_eq!(annot_build(SRC, "1,2,3", "list"), json!(["1", "2", "3"]));
    // An empty run contributes no element, not an empty one.
    assert_eq!(annot_build(SRC, "1", "list"), json!(["1"]));
}

/// Every spelling of a variable-length list collects. ONE digit per
/// item, since a greedy `1*DIGIT` would swallow the run into a single
/// item and prove nothing.
#[test]
fn annotation_collects_every_spelling_of_a_list() {
    const ONE: &str = "\nitem = DIGIT\n";
    let cases: Vec<(&str, String, &str)> = vec![
        // `item` leads here, and left-recursion elimination folds it in:
        // a rule whose body is a bare terminal stops being a part at all
        // there, so this spelling keeps `1*DIGIT`.
        (
            "separator after",
            "list = item *( \",\" item )   ; @array\nitem = 1*DIGIT\n".to_string(),
            "1,2,3",
        ),
        (
            "separator before",
            format!("list = *( item \",\" ) item   ; @array{ONE}"),
            "1,2,3",
        ),
        ("bare star", format!("list = *item   ; @array{ONE}"), "123"),
        ("bare plus", format!("list = 1*item   ; @array{ONE}"), "123"),
        (
            "group then star",
            format!("list = ( item ) *( item )   ; @array{ONE}"),
            "123",
        ),
    ];
    for (name, src, input) in cases {
        assert_eq!(
            annot_build(&src, input, "list"),
            json!(["1", "2", "3"]),
            "{name}"
        );
    }
}

/// `; @array` takes every part that produces a value, in order. An
/// iteration is not a special case: two parts in it are two elements.
#[test]
fn annotation_flattens_a_multi_part_iteration() {
    const SRC: &str = "list = *( a b )   ; @array\na = ALPHA\nb = DIGIT\n";
    assert_eq!(
        annot_build(SRC, "x1y2", "list"),
        json!(["x", "1", "y", "2"])
    );
}

/// A bare group is how ONE element is written out of several pieces, so
/// it resolves to its matched text, literals included. Only a repetition
/// collects; a group does so only as the item of one.
#[test]
fn annotation_keeps_a_bare_group_as_one_element() {
    const SRC: &str = "top = \"<\" ( \"[\" p \"]\" ) \">\"   ; @array\np = 1*DIGIT\n";
    assert_eq!(annot_build(SRC, "<[7]>", "top"), json!(["[7]"]));
}

/// Arrays name nothing, which is what made a run's text indefensible
/// there. An object NAMES the part, so the member stays that run's text.
#[test]
fn annotation_leaves_an_object_member_as_text() {
    const SRC: &str = "top = a *( \",\" a )   ; @object a rest\na = 1*DIGIT\n";
    assert_eq!(
        annot_build(SRC, "1,2,3", "top"),
        json!({ "a": "1", "rest": ",2,3" })
    );
}

/// Not a refusal, the opposite. A pure alias is the one caller
/// left-recursion elimination does not substitute into, so an annotated
/// alias of an annotated rule works and must not be caught by the
/// leading-reference rule above.
#[test]
fn annotation_nests_through_a_pure_alias() {
    const SRC: &str = "top = child   ; @object child\nchild = d   ; @object d\nd = 1*DIGIT\n";
    assert_eq!(
        annot_build(SRC, "7", "top"),
        json!({ "child": { "d": "7" } })
    );
}

/// A rule that builds a value gives no text to what contains it, so a
/// group or an intermediate rule wrapping one has nothing to resolve to.
/// And the fix the refusal points at has to work: a part that IS an
/// annotated rule nests.
#[test]
fn annotation_part_must_be_the_annotated_rule_itself() {
    for (label, src, want) in [
        (
            "a group wrapping an annotated rule",
            concat!(
                "top = \"<\" ( inner ) \">\"   ; @array\n",
                "inner = d   ; @object d\nd = 1*DIGIT\n"
            ),
            "builds a value of its own",
        ),
        (
            "a rule that recurses into itself",
            "top = 1*DIGIT [ \"+\" top ]   ; @array\n",
            "reaches 'top' itself",
        ),
    ] {
        match abnf_convert(src, None) {
            Ok(_) => panic!("{label}: expected a refusal, got none"),
            Err(error) => assert!(
                error.to_string().contains(want),
                "{label}: got {error}, want it to mention {want:?}"
            ),
        }
    }

    const SRC: &str = "top = \"<\" inner \">\"   ; @array\ninner = d   ; @object d\nd = 1*DIGIT\n";
    assert_eq!(annot_build(SRC, "<7>", "top"), json!([{ "d": "7" }]));
}
