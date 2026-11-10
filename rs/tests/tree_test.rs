// The tree a host translates, `parse_abnf_tree`: an ABNF document read as
// the schema grammar-rules. `test/spec/tree.tsv` holds each grammar to its
// tree's JSON text, member order included, as `ts/test/tree.test.js` and
// `go/tree_test.go` hold theirs; the order is part of the contract, since
// a host streams the members in that order and the part makes the same
// events from the rule passes of `make()`.
//
// Repetition is replacement (AGENTS.md): the rules of a grammar, the
// alternatives of a rule or a group and the elements of an alternative
// add no rule depth however many there are.

mod common;

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tabnas_support::{Failure, Runner, Value};

use common::spec_dir;

/// A tree's JSON text as JavaScript writes it: its members in order, and a
/// whole number without a fraction.
fn text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(number) => match number.as_f64() {
            Some(float) if float.fract() == 0.0 && float.abs() < 9.007_199_254_740_992e15 => {
                format!("{}", float as i64)
            }
            _ => number.to_string(),
        },
        serde_json::Value::Array(items) => {
            let items: Vec<String> = items.iter().map(text).collect();
            format!("[{}]", items.join(","))
        }
        serde_json::Value::Object(members) => {
            let members: Vec<String> = members
                .iter()
                .map(|(key, value)| {
                    format!("{}:{}", serde_json::Value::String(key.clone()), text(value))
                })
                .collect();
            format!("{{{}}}", members.join(","))
        }
        other => other.to_string(),
    }
}

#[test]
fn tree_spec() {
    Runner::new(|grammar| {
        let tree = tabnas_abnf::parse_abnf_tree(grammar).map_err(|error| {
            Failure::new(error.code.clone().unwrap_or_default()).with_message(error.message)
        })?;
        let json = tree.to_json();
        tabnas_bnf::check_rules(&json)
            .map_err(|error| Failure::message(format!("not a grammar-rules tree: {error}")))?;
        Ok(Value::from(serde_json::Value::String(text(&json))))
    })
    .input("grammar")
    .expected("expected")
    // The cell is the tree's JSON text, compared as text.
    .parse_expected(|expected, _row| {
        Ok(Value::from(serde_json::Value::String(expected.to_string())))
    })
    .file(spec_dir().join("tree.tsv"));
}

/// Every grammar of the fixtures and of the TypeScript suite's grammars
/// that the tree parser reads is a tree of the schema.
#[test]
fn every_tree_the_reader_builds_is_a_grammar_rules_tree() {
    let mut sources = Vec::new();
    for entry in fs::read_dir(spec_dir()).expect("test/spec") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|ext| ext != "tsv") {
            continue;
        }
        let raw = fs::read_to_string(&path).expect("a fixture");
        for line in raw.lines().skip(1) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cell = line.split('\t').next().unwrap_or_default();
            sources.push(tabnas_support::unescape(cell));
        }
    }
    let grammars = spec_dir()
        .join("..")
        .join("..")
        .join("ts")
        .join("test")
        .join("grammar");
    for entry in fs::read_dir(grammars).expect("ts/test/grammar") {
        sources.push(fs::read_to_string(entry.expect("an entry").path()).expect("a grammar"));
    }
    let mut read = 0;
    for src in &sources {
        let Ok(tree) = tabnas_abnf::parse_abnf_tree(src) else {
            continue;
        };
        if let Err(error) = tabnas_bnf::check_rules(&tree.to_json()) {
            panic!("{src:?}: {error}");
        }
        read += 1;
    }
    assert!(100 < read, "{read} trees read");
}

#[test]
fn make_builds_a_fresh_parser_whose_value_is_the_tree() {
    let tree = tabnas_abnf::make().parse("x = \"y\"\n").expect("a grammar");
    let want = tabnas_abnf::parse_abnf_tree("x = \"y\"\n").expect("a grammar");
    assert_eq!(text(&tree.to_json()), text(&want.to_json()));
}

/// The deepest rule a parse of `src` reaches.
fn depth(src: &str) -> usize {
    let mut parser = tabnas_abnf::make();
    let deepest = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&deepest);
    parser.subscribe_rule_done(move |rule, _context, done| {
        if !done.probe {
            seen.fetch_max(rule.d, Ordering::Relaxed);
        }
    });
    parser.parse(src).expect("the document parses");
    deepest.load(Ordering::Relaxed)
}

#[test]
fn repetition_adds_no_rule_depth() {
    const N: usize = 10_000;
    type Document = fn(usize) -> String;
    let repeated: [(&str, Document); 5] = [
        ("rules of a grammar", |n| {
            let rules: Vec<String> = (0..n).map(|i| format!("r{i} = \"x\"\n")).collect();
            rules.concat()
        }),
        ("alternatives of a rule", |n| {
            let items: Vec<String> = (0..n).map(|i| format!("r{i}")).collect();
            format!("a = {}\n", items.join(" / "))
        }),
        ("elements of an alternative", |n| {
            let items: Vec<String> = (0..n).map(|i| format!("*r{i}")).collect();
            format!("a = {}\n", items.join(" "))
        }),
        ("alternatives of a group", |n| {
            let items: Vec<String> = (0..n).map(|i| format!("%x{i:x}")).collect();
            format!("a = ( {} )\n", items.join(" / "))
        }),
        ("elements of an option", |n| {
            format!("a = [ {}]\n", "\"x\" ".repeat(n))
        }),
    ];
    for (what, document) in repeated {
        // Two items, since a list's first item can sit apart from its
        // repeated ones: the loop's own item is the second.
        assert_eq!(depth(&document(N)), depth(&document(2)), "{what}");
    }
}
