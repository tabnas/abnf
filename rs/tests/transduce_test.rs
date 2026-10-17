// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

//! The transducer part of an ABNF grammar: `abnf_transduce` converts the
//! source and hands the spec to tabnas-bnf's `transduce_alc`, as the
//! TypeScript front-end's `abnfTransduce` does (ts/test/transduce.test.js
//! holds the same text).

use tabnas_abnf::{abnf_convert, abnf_transduce, transduce_alc, AbnfConvertOptions, TransducePart};

/// /etc/hosts, as aless's grammar library writes it.
const HOSTS: &str = "hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array\n\
entry   = address names                      ; @object address names\n\
address = word\n\
names   = 1*word                             ; @array\n\
word    = ( TX )\n";

#[test]
fn writes_the_part_of_an_annotated_grammar() {
    let part = abnf_transduce(HOSTS, "hosts", None).expect("the grammar converts");
    let want = [
        "; The transducer part of the grammar hosts: the JSON events of the value",
        "; its @object and @array annotations build, from its rule passes, as",
        "; tabnas-bnf wrote them from the value actions those annotations attach.",
        "def hosts-transduce [input]",
        "  transduce-tree",
        "    record",
        "      entry :_gen6_plus_word (record (entry :text true))",
        "      entry :_gen6_star_word$alt0 (record (entry :text true))",
        "      entry :entry (record (entry :container :object) (entry :member \"address\") (entry :text true))",
        "      entry :entry$step1 (record (entry :container :object) (entry :member \"names\"))",
        "      entry :hosts (record (entry :container :array))",
        "      entry :hosts$step1 (record (entry :container :array))",
        "      entry :names (record (entry :container :array))",
        "    input",
        "",
    ]
    .join("\n");
    assert_eq!(
        part,
        TransducePart::Part {
            entry: "hosts-transduce".into(),
            source: want
        }
    );
}

#[test]
fn is_the_compilers_part_of_the_converted_spec() {
    let opts = AbnfConvertOptions {
        word_keywords: true,
        builtins: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(HOSTS, Some(&opts)).expect("the grammar converts");
    assert_eq!(
        abnf_transduce(HOSTS, "hosts", Some(&opts)).expect("the grammar converts"),
        transduce_alc(&spec, "hosts")
    );
}

#[test]
fn reads_a_grammar_whole_whose_start_rule_has_no_annotation() {
    let part = abnf_transduce("top = 1*DIGIT \".\" 1*DIGIT\n", "g", None).expect("converts");
    let TransducePart::Whole(why) = part else {
        panic!("an unannotated grammar was given a part");
    };
    assert!(
        why.starts_with(
            "the start rule top has no annotation, so the value is the {rule, src, kids} tree"
        ),
        "{why}"
    );
}

#[test]
fn refuses_a_source_that_does_not_convert() {
    assert!(abnf_transduce("top = = x\n", "g", None).is_err());
}
