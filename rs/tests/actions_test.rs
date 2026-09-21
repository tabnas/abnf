// User semantic actions: attaching Rust callbacks to a compiled grammar
// by alt mark or rule phase. Mirrors `go/actions_test.go` and
// `ts/test/actions.test.js`.

mod common;

use std::sync::{Arc, Mutex};

use serde_json::Value as JsonValue;
use tabnas::{Context, Options, RewindOptions, Rule, Tabnas, Value};
use tabnas_abnf::{
    abnf, abnf_convert, attach_actions, mark_listing, AbnfConvertOptions, AbnfError, AbnfOptions,
    ActionFn, ActionsMap,
};

/// A shared, thread-safe log a user action can append to. Actions are
/// `Send + Sync` because the engine is, so a plain captured `Vec` will
/// not do.
type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn joined(log: &Log) -> String {
    log.lock().expect("the log is not poisoned").join(",")
}

/// Write a field onto the rule's AST node, through the shared cell.
fn set_delta(rule: &Rule, delta: f64) {
    if let Some(node) = rule.node.borrow_mut().as_object_mut() {
        node.insert("delta".to_string(), Value::Number(delta));
    }
}

/// One action, in the shape the actions map takes.
fn action(callback: impl Fn(&mut Rule, &mut Context) + Send + Sync + 'static) -> ActionFn {
    Arc::new(move |rule: &mut Rule, context: &mut Context| {
        callback(rule, context);
        Ok(())
    })
}

fn engine() -> Tabnas {
    let options = Options {
        rewind: RewindOptions {
            history: Some(4096),
        },
        ..Options::default()
    };
    Tabnas::with_options(options)
}

#[test]
fn actions_bind_by_mark() {
    let log = log();
    let (inc_log, dec_log) = (Arc::clone(&log), Arc::clone(&log));
    let mut parser = engine();
    abnf(
        &mut parser,
        r#"op = "inc" / "dec""#,
        Some(&AbnfOptions::default().with_actions(vec![
            (
                "@op:o:INC".to_string(),
                vec![action(move |rule, _context| {
                    inc_log.lock().expect("not poisoned").push("inc".into());
                    set_delta(rule, 1.0);
                })],
            ),
            (
                "@op:o:DEC".to_string(),
                vec![action(move |rule, _context| {
                    dec_log.lock().expect("not poisoned").push("dec".into());
                    set_delta(rule, -1.0);
                })],
            ),
        ])),
    )
    .expect("installs");

    let inc = parser.parse("inc").expect("parses").to_json();
    let dec = parser.parse("dec").expect("parses").to_json();
    assert_eq!(inc["delta"], JsonValue::from(1.0));
    assert_eq!(dec["delta"], JsonValue::from(-1.0));
    assert_eq!(
        inc["rule"], "op",
        "the compiler's tree action must run first"
    );
    assert_eq!(joined(&log), "inc,dec");
}

#[test]
fn actions_multiple_run_in_order() {
    let log = log();
    let (first, second) = (Arc::clone(&log), Arc::clone(&log));
    let mut parser = Tabnas::new();
    abnf(
        &mut parser,
        r#"op = "inc""#,
        Some(&AbnfOptions::default().with_actions(vec![(
            "@op:o:INC".to_string(),
            vec![
                action(move |_rule, _context| first.lock().expect("not poisoned").push("a".into())),
                action(move |_rule, _context| {
                    second.lock().expect("not poisoned").push("b".into())
                }),
            ],
        )])),
    )
    .expect("installs");
    parser.parse("inc").expect("parses");
    assert_eq!(joined(&log), "a,b");
}

#[test]
fn actions_rule_phase_hook() {
    let log = log();
    let entered = Arc::clone(&log);
    let mut parser = Tabnas::new();
    abnf(
        &mut parser,
        r#"g = "x""#,
        Some(&AbnfOptions::default().with_actions(vec![(
            "@g:bo".to_string(),
            vec![action(move |_rule, _context| {
                entered.lock().expect("not poisoned").push("enter".into())
            })],
        )])),
    )
    .expect("installs");
    parser.parse("x").expect("parses");
    assert_eq!(joined(&log), "enter");
}

#[test]
fn actions_reject_bad_refs() {
    for bad in ["@op:o:NOPE", "@nope:o:INC", "@op:zz"] {
        let options = AbnfConvertOptions {
            marks: true,
            ..AbnfConvertOptions::default()
        };
        let mut spec = abnf_convert(r#"op = "inc" / "dec""#, Some(&options)).expect("compiles");
        let actions: ActionsMap = vec![(bad.to_string(), vec![action(|_rule, _context| {})])];
        assert!(
            attach_actions(&mut spec, actions).is_err(),
            "should reject {bad:?}"
        );
    }
}

#[test]
fn mark_listing_names_every_mark() {
    let options = AbnfConvertOptions {
        marks: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(r#"op = "inc" / "dec""#, Some(&options)).expect("compiles");
    let listing = mark_listing(&spec);
    assert!(
        listing.contains("op") && listing.contains("o:INC"),
        "listing missing op o:INC:\n{listing}"
    );
    assert!(
        listing.contains("o:DEC"),
        "listing missing o:DEC:\n{listing}"
    );
}

/// Default conversion carries no marks at all.
#[test]
fn marks_opt_in() {
    let spec = abnf_convert(r#"op = "inc" / "dec""#, None).expect("compiles");
    for (name, rule) in &spec.rule {
        let Some(rule) = rule else { continue };
        let alts = rule.open.iter().chain(rule.close.iter().flatten());
        for alt in alts {
            let carries = alt
                .get("u")
                .and_then(JsonValue::as_object)
                .is_some_and(|bag| bag.contains_key("m$"));
            assert!(!carries, "rule {name:?} carries a mark without marks: true");
        }
    }
}

/// In builtins mode the user action runs AFTER the `@node$` tree
/// builtin, so the node it writes to is the one the tree built.
#[test]
fn actions_builtins_mode_composition() {
    let options = AbnfConvertOptions {
        builtins: true,
        marks: true,
        ..AbnfConvertOptions::default()
    };
    let mut spec = abnf_convert(r#"op = "inc" / "dec""#, Some(&options)).expect("compiles");
    attach_actions(
        &mut spec,
        vec![(
            "@op:o:INC".to_string(),
            vec![action(|rule, _context| set_delta(rule, 1.0))],
        )],
    )
    .expect("attaches");
    let mut parser = Tabnas::new();
    spec.install(&mut parser)
        .map_err(|error| AbnfError::Install(error.to_string()))
        .expect("installs");
    let out = parser.parse("inc").expect("parses").to_json();
    assert_eq!(out["rule"], "op", "the tree was not built by @node$");
    assert_eq!(
        out["delta"],
        JsonValue::from(1.0),
        "the user action did not run"
    );
}
