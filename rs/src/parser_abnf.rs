// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

//! The ABNF grammar itself, expressed as a tabnas grammar document and
//! installed on a bare engine. The Rust port of the TypeScript
//! `abnfRules` table plus `getAbnfParser` in `ts/src/converter.ts`, and
//! of `go/parser_abnf.go`.
//!
//! The converter eats its own dog food: ABNF source is read by a tabnas
//! instance whose grammar is the document below, and the parse AST is
//! assembled by the action closures in [`register_refs`].
//!
//! Token vocabulary (mirrors the TypeScript comment):
//!
//! | Token | Means |
//! |---|---|
//! | `#DEF` | `=`, the rule-definition operator |
//! | `#DEFA` | `=/`, the incremental-alternatives operator |
//! | `#ALT` | `/`, alternation |
//! | `#STAR` | `*`, the repetition separator |
//! | `#NUM` | a decimal repetition count |
//! | `#NV` | `%[xdb]NN[(-NN|(.NN)*)]`, a numeric value |
//! | `#SS` | `%s`, the case-sensitive string prefix |
//! | `#SI` | `%i`, the case-insensitive string prefix |
//! | `#LP` / `#RP` | `(` and `)` |
//! | `#OB` / `#CB` | `[` and `]` |
//! | `#PV` | `<prose>` |
//! | `#TX` | a bare identifier |
//! | `#ST` | a quoted string literal |
//! | `#ZZ` | end of source |

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;
use serde_json::Value as JsonValue;
use tabnas::{
    Context, MatchToken, MatchTokenMatcher, MatchTokenResult, Options, Rule, Tabnas, TabnasError,
    Token, Value,
};

use crate::numeric::parse_numeric_value;

/// How deep the rule stack may go while reading one ABNF source.
///
/// A grammar file is untrusted input, the parse AST nests once per
/// bracket, and both the walks over it and the engine's own value drop
/// are recursive: a Rust stack that runs out aborts the process rather
/// than unwinding. `((((…` a few tens of thousands deep reached that,
/// so the depth is refused here instead, before anything that deep is
/// built.
///
/// The limit counts RULE levels, which a bracket costs four of, so it
/// admits about 500 nested groups or options. The shared compiler
/// refuses element nesting past 128 in any case, so every grammar this
/// cap turns away was already going to be refused, only with a worse
/// failure. No ABNF an author writes comes close.
pub(crate) const MAX_GROUP_DEPTH: usize = 2048;

/// The diagnostic raised when [`MAX_GROUP_DEPTH`] is exceeded.
pub(crate) const DEPTH_MESSAGE: &str =
    "abnf: grammar nests too deeply (more than 2048 rule levels, about 500 nested \
     groups or options)";

/// The error code carried by the depth refusal, so the converter can
/// tell it apart from an ordinary engine rejection.
pub(crate) const DEPTH_CODE: &str = "abnf_group_depth";

thread_local! {
    /// The first numeric-value diagnostic of the parse running on this
    /// thread, or `None`.
    ///
    /// The canonical TypeScript throws out of the alt action that decodes
    /// `%x…`, so the fault is reported even when the element it was
    /// decoded into is later discarded (`bad = ( %x110000` drops the
    /// unclosed group and everything in it). An alt action here could
    /// return an error, but the engine would wrap it with a position the
    /// TypeScript diagnostic does not carry, and the exact bytes of that
    /// message are pinned by `test/spec/alignment-abnf-errors.tsv`.
    ///
    /// So the message is recorded here and read once the parse is
    /// structurally complete, which is what `go/converter.go` does with
    /// its per-element `NumErr` field. A thread local rather than the
    /// parse context's `u` bag because `Tabnas::parse` hands the context
    /// back to nobody: this is the only channel out.
    static NUM_ERR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Record a numeric-value diagnostic, keeping the first.
pub(crate) fn record_num_err(message: String) {
    NUM_ERR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(message);
        }
    });
}

/// Forget any numeric-value diagnostic left by an earlier parse.
fn clear_num_err() {
    NUM_ERR.with(|slot| *slot.borrow_mut() = None);
}

/// The numeric-value diagnostic recorded by the parse just finished.
fn taken_num_err() -> Option<String> {
    NUM_ERR.with(|slot| slot.borrow_mut().take())
}

/// The ABNF meta-grammar as a tabnas grammar document.
///
/// Every alternative is the one the canonical `abnfRules` table carries,
/// in the same order. The order is load-bearing twice over: the engine
/// tries alternatives in order, and the `s` patterns decide which token
/// matchers the lexer is offered at each position. See the comments in
/// `ts/src/converter.ts` for why a rulename followed by a repetition
/// count or by another atom needs an alternative of its own.
const GRAMMAR_TEXT: &str = r##"
{
  "rule": {
    "abnf": {
      "open": [
        { "s": "#ZZ", "g": "empty" },
        { "p": "prod" }
      ],
      "close": [
        { "s": "#ZZ" }
      ]
    },

    "prod": {
      "open": [
        { "s": "#TX #DEF", "a": "@prod-name", "p": "alts" },
        { "s": "#PV #DEF", "a": "@prod-name-prose", "p": "alts" },
        { "s": "#TX #DEFA", "a": "@prod-name-inc", "p": "alts" }
      ],
      "close": [
        { "s": "#TX #DEF", "b": 2, "r": "prod" },
        { "s": "#TX #DEFA", "b": 2, "r": "prod" },
        { "s": "#PV #DEF", "b": 2, "r": "prod" },
        { "b": 1 }
      ]
    },

    "alts": {
      "open": [
        { "p": "seq" }
      ],
      "close": [
        { "s": "#ALT", "p": "seq" },
        { "b": 1 }
      ]
    },

    "seq": {
      "open": [
        { "s": "#TX #DEF", "b": 2, "g": "end" },
        { "s": "#TX #DEFA", "b": 2, "g": "end" },
        { "s": "#TX #NUM", "b": 2, "p": "elem" },
        { "s": "#TX #ATOM", "b": 2, "p": "elem" },
        { "s": "#PV #DEF", "b": 2, "g": "end" },
        { "s": "#ALT", "b": 1, "g": "end" },
        { "s": "#ZZ", "b": 1, "g": "end" },
        { "s": "#RP", "b": 1, "g": "end" },
        { "s": "#CB", "b": 1, "g": "end" },
        { "s": "#ST", "b": 1, "p": "elem" },
        { "s": "#NV", "b": 1, "p": "elem" },
        { "s": "#SS", "b": 1, "p": "elem" },
        { "s": "#SI", "b": 1, "p": "elem" },
        { "s": "#PV", "b": 1, "p": "elem" },
        { "s": "#TX", "b": 1, "p": "elem" },
        { "s": "#LP", "b": 1, "p": "elem" },
        { "s": "#OB", "b": 1, "p": "elem" },
        { "s": "#STAR", "b": 1, "p": "elem" },
        { "s": "#NUM", "b": 1, "p": "elem" },
        { "p": "elem" }
      ],
      "close": [
        { "s": "#TX #DEF", "b": 2, "g": "end" },
        { "s": "#TX #DEFA", "b": 2, "g": "end" },
        { "s": "#PV #DEF", "b": 2, "g": "end" },
        { "s": "#TX #NUM", "b": 2, "p": "elem" },
        { "s": "#TX #ATOM", "b": 2, "p": "elem" },
        { "s": "#ALT", "b": 1, "g": "end" },
        { "s": "#ZZ", "b": 1, "g": "end" },
        { "s": "#RP", "b": 1, "g": "end" },
        { "s": "#CB", "b": 1, "g": "end" },
        { "s": "#ST", "b": 1, "p": "elem" },
        { "s": "#NV", "b": 1, "p": "elem" },
        { "s": "#SS", "b": 1, "p": "elem" },
        { "s": "#SI", "b": 1, "p": "elem" },
        { "s": "#PV", "b": 1, "p": "elem" },
        { "s": "#TX", "b": 1, "p": "elem" },
        { "s": "#LP", "b": 1, "p": "elem" },
        { "s": "#OB", "b": 1, "p": "elem" },
        { "s": "#STAR", "b": 1, "p": "elem" },
        { "s": "#NUM", "b": 1, "p": "elem" },
        { "b": 1 }
      ]
    },

    "elem": {
      "open": [
        { "s": "#NUM #STAR #NUM #ATOM", "b": 1, "a": "@elem-rep-bounded", "p": "atom" },
        { "s": "#NUM #STAR #ATOM", "b": 1, "a": "@elem-rep-atleast", "p": "atom" },
        { "s": "#STAR #NUM #ATOM", "b": 1, "a": "@elem-rep-atmost", "p": "atom" },
        { "s": "#STAR #ATOM", "b": 1, "a": "@elem-rep-star", "p": "atom" },
        { "s": "#NUM #ATOM", "b": 1, "a": "@elem-rep-exact", "p": "atom" },
        { "p": "atom" }
      ],
      "close": [
        { "a": "@elem-close" }
      ]
    },

    "atom": {
      "open": [
        { "s": "#SS #ST", "a": "@atom-ss" },
        { "s": "#SI #ST", "a": "@atom-si" },
        { "s": "#ST", "a": "@atom-st" },
        { "s": "#NV", "a": "@atom-nv" },
        { "s": "#PV", "a": "@atom-pv" },
        { "s": "#TX", "a": "@atom-tx" },
        { "s": "#LP", "a": "@atom-lp", "p": "alts" },
        { "s": "#OB", "a": "@atom-ob", "p": "alts" }
      ],
      "close": [
        { "s": "#RP", "c": "@atom-group-c", "a": "@atom-group-close" },
        { "s": "#CB", "c": "@atom-opt-c", "a": "@atom-opt-close" },
        { "s": "#TX", "b": 1 },
        { "s": "#ST", "b": 1 },
        { "s": "#NV", "b": 1 },
        { "s": "#SS", "b": 1 },
        { "s": "#SI", "b": 1 },
        { "s": "#PV", "b": 1 },
        { "s": "#NUM", "b": 1 },
        { "s": "#STAR", "b": 1 },
        { "s": "#LP", "b": 1 },
        { "s": "#OB", "b": 1 },
        { "s": "#RP", "b": 1 },
        { "s": "#CB", "b": 1 },
        { "s": "#ALT", "b": 1 },
        { "s": "#DEF", "b": 1 },
        { "s": "#ZZ", "b": 1 },
        { "b": 1 }
      ]
    }
  }
}
"##;

/// The ABNF meta-grammar's rule table, as data.
///
/// The Rust spelling of the TypeScript `abnfRules` export and of Go's
/// `AbnfRules()`: a fresh document each call, so a caller may take it
/// apart without disturbing the parser this crate builds from the same
/// text.
pub fn abnf_rules() -> JsonValue {
    let document: JsonValue =
        serde_json::from_str(GRAMMAR_TEXT).expect("the embedded ABNF meta-grammar is valid JSON");
    document
        .get("rule")
        .cloned()
        .expect("the embedded ABNF meta-grammar has a rule map")
}

// ---- node helpers ---------------------------------------------------

/// Replace a rule's node, rebinding the cell rather than writing through
/// it.
///
/// A pushed rule INHERITS its parent's node cell, so writing through the
/// cell would overwrite what the parent is accumulating. This is the
/// Rust spelling of TypeScript's `r.node = …`, which rebinds the
/// property and leaves the parent's own reference alone.
fn set_node(rule: &mut Rule, value: Value) {
    rule.node = Rc::new(RefCell::new(value));
}

/// Append to the array a rule's node holds, through the shared cell, so
/// a parent accumulating into the same array sees it. The TypeScript
/// `r.node.push(…)` on an inherited array.
fn push_node(rule: &Rule, value: Value) {
    if let Some(list) = rule.node.borrow_mut().as_array_mut() {
        list.push(value);
    }
}

/// One named field of an object value, or `None` when there is none.
fn field(value: &Value, key: &str) -> Option<Value> {
    match value {
        Value::Object(entries) => entries.get(key).cloned(),
        _ => None,
    }
}

/// Build an object value from named fields, dropping the absent ones.
fn object(fields: Vec<(&str, Option<Value>)>) -> Value {
    let mut map = IndexMap::new();
    for (key, value) in fields {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    Value::object(map)
}

/// The source span of a token, as the IR's `SrcSpan` shape.
///
/// Every field is copied straight off the token: the compiler stores
/// whatever units the front-end's own engine tokens use, precisely so
/// that no arithmetic, and so no off-by-one, happens at this boundary.
/// This engine counts bytes where the canonical TypeScript counts UTF-16
/// code units, the divergence the engine already records for token
/// positions.
pub(crate) fn span_of(token: Option<&Token>) -> Option<Value> {
    let token = token?;
    Some(object(vec![
        ("s", Some(Value::Number(token.site.si as f64))),
        ("e", Some(Value::Number((token.site.si + token.len) as f64))),
        ("r", Some(Value::Number(token.site.ri as f64))),
        ("c", Some(Value::Number(token.site.ci as f64))),
    ]))
}

/// One span covering two tokens: a group runs from its `(` to its `)`, a
/// bracketed optional from `[` to `]`. Falls back to whichever end is
/// known when the other is not.
fn span_to(from: Option<&Token>, to: Option<&Token>) -> Option<Value> {
    let a = span_of(from);
    let b = span_of(to);
    let (Some(a), Some(b)) = (a.clone(), b.clone()) else {
        return a.or(b);
    };
    Some(object(vec![
        ("s", field(&a, "s")),
        ("e", field(&b, "e")),
        ("r", field(&a, "r")),
        ("c", field(&a, "c")),
    ]))
}

/// The token a matched open slot holds, cloned so the rule stays
/// borrowable.
fn open_token(rule: &Rule, index: usize) -> Option<Token> {
    rule.o.get(index).cloned()
}

/// The first token matched in a rule's CLOSE phase: the `)` of a group,
/// the `]` of a bracketed optional. `None` when the rule closed without
/// matching one, so a span falls back to its opener.
fn close_token(rule: &Rule) -> Option<Token> {
    rule.c0().cloned()
}

/// A matched token's value as a string, resolving a lazy value function
/// the way the canonical `.val` getter does, and falling back to the
/// matched source.
fn token_string(token: &Token, rule: &mut Rule, context: &mut Context) -> String {
    let token = token.clone();
    match token.resolve_val(rule, context) {
        Value::String(text) => text,
        Value::Text(text) => text.string,
        _ => token.src.as_str().to_string(),
    }
}

/// Read a number off a rule's `u` bag.
fn u_number(rule: &Rule, key: &str) -> f64 {
    match rule.u.get(key) {
        Some(Value::Number(number)) => *number,
        _ => 0.0,
    }
}

// ---- the AST-assembly closures --------------------------------------

/// Register every action, condition and lifecycle hook the meta-grammar
/// names. The reserved `@<rule>-bo` / `@<rule>-bc` names are wired onto
/// their rule's phase by the grammar loader.
fn register_refs(parser: &mut Tabnas) {
    // --- abnf (top level) ---
    parser.state_action_ref("@abnf-bo", |rule, _context| {
        set_node(rule, Value::array(Vec::new()));
        Ok(())
    });

    // --- prod ---
    parser.action_with_context("@prod-name", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        let span = span_of(Some(&token));
        let bag = rule.u_mut();
        bag.insert("name".into(), Value::String(name));
        bag.insert("nameSp".into(), span.unwrap_or(Value::Undefined));
        bag.insert("incremental".into(), Value::Bool(false));
        Ok(())
    });
    // `<all> = …`: the production name is the prose token's raw source,
    // angle brackets and all, so it can never collide with a real
    // rulename.
    parser.action_with_context("@prod-name-prose", |rule, _context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token.src.as_str().to_string();
        let span = span_of(Some(&token));
        let bag = rule.u_mut();
        bag.insert("name".into(), Value::String(name));
        bag.insert("nameSp".into(), span.unwrap_or(Value::Undefined));
        bag.insert("incremental".into(), Value::Bool(false));
        Ok(())
    });
    parser.action_with_context("@prod-name-inc", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        let span = span_of(Some(&token));
        let bag = rule.u_mut();
        bag.insert("name".into(), Value::String(name));
        bag.insert("nameSp".into(), span.unwrap_or(Value::Undefined));
        bag.insert("incremental".into(), Value::Bool(true));
        Ok(())
    });
    parser.state_action_ref("@prod-bc", |rule, _context| {
        if rule.child_node.is_undefined() {
            return Ok(());
        }
        let alts = rule.child_node.clone();
        let name = match rule.u.get("name") {
            Some(Value::String(name)) => name.clone(),
            _ => String::new(),
        };
        // The name, not the body: that is what an outline entry,
        // go-to-definition and a whole-rule diagnostic want, and an ABNF
        // body can run over many folded lines.
        let span = match rule.u.get("nameSp") {
            Some(value) if !value.is_undefined() => Some(value.clone()),
            _ => None,
        };
        let incremental = matches!(rule.u.get("incremental"), Some(Value::Bool(true)));
        push_node(
            rule,
            object(vec![
                ("name", Some(Value::String(name))),
                ("alts", Some(alts)),
                ("sp", span),
                ("incremental", incremental.then_some(Value::Bool(true))),
            ]),
        );
        Ok(())
    });

    // --- alts ---
    parser.state_action_ref("@alts-bo", |rule, _context| {
        set_node(rule, Value::array(Vec::new()));
        Ok(())
    });
    parser.state_action_ref("@alts-bc", |rule, _context| {
        if rule.child_node.is_undefined() {
            return Ok(());
        }
        let sequence = rule.child_node.clone();
        push_node(rule, sequence);
        Ok(())
    });

    // --- seq ---
    parser.state_action_ref("@seq-bo", |rule, _context| {
        set_node(rule, Value::array(Vec::new()));
        Ok(())
    });

    // --- elem ---
    parser.state_action_ref("@elem-bo", |rule, _context| {
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(1.0));
        bag.insert("max".into(), Value::Number(1.0));
        Ok(())
    });
    parser.action_with_context("@elem-rep-bounded", |rule, _context| {
        let min = open_token(rule, 0).map_or(0.0, |token| count_of(&token));
        let max = open_token(rule, 2).map_or(0.0, |token| count_of(&token));
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(min));
        bag.insert("max".into(), Value::Number(max));
        Ok(())
    });
    parser.action_with_context("@elem-rep-atleast", |rule, _context| {
        let min = open_token(rule, 0).map_or(0.0, |token| count_of(&token));
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(min));
        bag.insert("max".into(), Value::Number(f64::INFINITY));
        Ok(())
    });
    parser.action_with_context("@elem-rep-atmost", |rule, _context| {
        let max = open_token(rule, 1).map_or(0.0, |token| count_of(&token));
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(0.0));
        bag.insert("max".into(), Value::Number(max));
        Ok(())
    });
    parser.action_with_context("@elem-rep-star", |rule, _context| {
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(0.0));
        bag.insert("max".into(), Value::Number(f64::INFINITY));
        Ok(())
    });
    parser.action_with_context("@elem-rep-exact", |rule, _context| {
        let count = open_token(rule, 0).map_or(0.0, |token| count_of(&token));
        let bag = rule.u_mut();
        bag.insert("min".into(), Value::Number(count));
        bag.insert("max".into(), Value::Number(count));
        Ok(())
    });
    parser.action_with_context("@elem-close", |rule, _context| {
        // A HOLE, not a no-op. `bad = ( "a"` runs the atom's bail
        // alternate at end of source, so the atom pops with no node and
        // there is nothing to wrap. The canonical TypeScript pushes the
        // `undefined` it found, and `reject_holes` refuses the rule by
        // name; dropping the element instead would let the malformed
        // grammar compile clean.
        let item = rule.child_node.clone();
        if item.is_undefined() {
            push_node(rule, Value::Undefined);
            return Ok(());
        }
        let min = u_number(rule, "min");
        let max = u_number(rule, "max");
        let wrapped = if min == 1.0 && max == 1.0 {
            item
        } else if min == 0.0 && max.is_infinite() {
            object(vec![
                ("kind", Some(Value::String("star".into()))),
                ("inner", Some(item)),
            ])
        } else if min == 1.0 && max.is_infinite() {
            object(vec![
                ("kind", Some(Value::String("plus".into()))),
                ("inner", Some(item)),
            ])
        } else if min == 0.0 && max == 1.0 {
            object(vec![
                ("kind", Some(Value::String("opt".into()))),
                ("inner", Some(item)),
            ])
        } else {
            object(vec![
                ("kind", Some(Value::String("rep".into()))),
                ("min", Some(Value::Number(min))),
                ("max", Some(Value::Number(max))),
                ("inner", Some(item)),
            ])
        };
        push_node(rule, wrapped);
        Ok(())
    });

    // --- atom ---
    parser.state_action_ref("@atom-bo", |rule, _context| {
        set_node(rule, Value::Undefined);
        Ok(())
    });
    // Case-sensitive string: %s"foo".
    parser.action_with_context("@atom-ss", |rule, context| {
        let (Some(prefix), Some(text)) = (open_token(rule, 0), open_token(rule, 1)) else {
            return Ok(());
        };
        let literal = token_string(&text, rule, context);
        let span = span_to(Some(&prefix), Some(&text));
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("term".into()))),
                ("literal", Some(Value::String(literal))),
                ("caseSensitive", Some(Value::Bool(true))),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    // Case-insensitive string: %i"foo", the ABNF default spelled out.
    parser.action_with_context("@atom-si", |rule, context| {
        let (Some(prefix), Some(text)) = (open_token(rule, 0), open_token(rule, 1)) else {
            return Ok(());
        };
        let literal = token_string(&text, rule, context);
        let span = span_to(Some(&prefix), Some(&text));
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("term".into()))),
                ("literal", Some(Value::String(literal))),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    // Bare quoted string, case-insensitive per the ABNF default.
    parser.action_with_context("@atom-st", |rule, context| {
        let Some(text) = open_token(rule, 0) else {
            return Ok(());
        };
        let literal = token_string(&text, rule, context);
        let span = span_of(Some(&text));
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("term".into()))),
                ("literal", Some(Value::String(literal))),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    parser.action_with_context("@atom-nv", |rule, _context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let element = parse_numeric_value(token.src.as_str(), Some(&token));
        set_node(rule, element);
        Ok(())
    });
    // Prose terminal `<free text>`, carried through as written; the
    // shared compiler's prose pass decides what it means.
    parser.action_with_context("@atom-pv", |rule, _context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let source = token.src.as_str();
        let text = source
            .strip_prefix('<')
            .and_then(|rest| rest.strip_suffix('>'))
            .unwrap_or(source)
            .to_string();
        let span = span_of(Some(&token));
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("prose".into()))),
                ("text", Some(Value::String(text))),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    parser.action_with_context("@atom-tx", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        let span = span_of(Some(&token));
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("ref".into()))),
                ("name", Some(Value::String(name))),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    parser.action_with_context("@atom-lp", |rule, _context| open_group(rule, "group"));
    parser.action_with_context("@atom-ob", |rule, _context| open_group(rule, "opt"));
    parser.alt_condition("@atom-group-c", |rule, _context| {
        matches!(rule.u.get("groupKind"), Some(Value::String(kind)) if "group" == kind)
    });
    parser.action_with_context("@atom-group-close", |rule, _context| {
        let alts = rule.child_node.clone();
        let open = rule
            .u
            .get("openTkn")
            .cloned()
            .filter(|value| !value.is_undefined());
        let span = group_span(rule, open.as_ref());
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("group".into()))),
                ("alts", Some(alts)),
                ("sp", span),
            ]),
        );
        Ok(())
    });
    parser.alt_condition("@atom-opt-c", |rule, _context| {
        matches!(rule.u.get("groupKind"), Some(Value::String(kind)) if "opt" == kind)
    });
    parser.action_with_context("@atom-opt-close", |rule, _context| {
        let alts = rule.child_node.clone();
        let open = rule
            .u
            .get("openTkn")
            .cloned()
            .filter(|value| !value.is_undefined());
        let bracket = group_span(rule, open.as_ref());
        set_node(
            rule,
            object(vec![
                ("kind", Some(Value::String("opt".into()))),
                (
                    "inner",
                    Some(object(vec![
                        ("kind", Some(Value::String("group".into()))),
                        ("alts", Some(alts)),
                        ("sp", bracket.clone()),
                    ])),
                ),
                ("sp", bracket),
            ]),
        );
        Ok(())
    });
}

/// The repetition count a `#NUM` token holds. RFC 5234 `repeat` is
/// `1*DIGIT`, and the matcher only ever hands over digits, so a count
/// too large for the engine's number is clamped rather than wrapped.
fn count_of(token: &Token) -> f64 {
    token.src.as_str().parse::<f64>().unwrap_or(0.0)
}

/// Note which bracket opened this atom, and refuse a source that nests
/// them past [`MAX_GROUP_DEPTH`].
fn open_group(rule: &mut Rule, kind: &str) -> Result<(), tabnas::ActionError> {
    if MAX_GROUP_DEPTH < rule.d {
        return Err(tabnas::ActionError::new(DEPTH_CODE, DEPTH_MESSAGE));
    }
    let span = open_token(rule, 0).and_then(|token| span_of(Some(&token)));
    let bag = rule.u_mut();
    bag.insert("groupKind".into(), Value::String(kind.into()));
    bag.insert("openTkn".into(), span.unwrap_or(Value::Undefined));
    Ok(())
}

/// The span from a remembered opening bracket to the closing one.
fn group_span(rule: &Rule, open: Option<&Value>) -> Option<Value> {
    let close = close_token(rule).and_then(|token| span_of(Some(&token)));
    match (open, close) {
        (Some(open), Some(close)) => Some(object(vec![
            ("s", field(open, "s")),
            ("e", field(&close, "e")),
            ("r", field(open, "r")),
            ("c", field(open, "c")),
        ])),
        (Some(open), None) => Some(open.clone()),
        (None, close) => close,
    }
}

// ---- the parser instance --------------------------------------------

/// The engine options the ABNF meta-grammar needs.
fn abnf_parser_options() -> Options {
    let mut options = Options::default();

    // Clear the JSON-oriented defaults this notation does not use, so
    // `:`, `,` and `{` have no special meaning inside ABNF source, and
    // re-map `#OB` / `#CB` from `{` / `}` to ABNF's `[` / `]`.
    for retired in ["#OS", "#CS", "#CL", "#CA"] {
        options.fixed.tokens.shift_remove(retired);
    }

    // RFC 5234 rulename is `ALPHA *(ALPHA / DIGIT / "-")`: nothing is
    // reserved, so `true`, `false` and `null` are ordinary rule names,
    // which JSON's own ABNF relies on. With the engine's default
    // keyword-value lexing they arrive as `#VL` value tokens instead of
    // `#TX` barewords, and no ABNF rendering of JSON compiles. The
    // meta-grammar has no use for `#VL` at all; this switch affects the
    // parser that READS ABNF, never the grammars it emits, where `VL`
    // remains a built-in token name.
    options.value.lex = false;

    // RFC 5234 char-val has NO escape sequences at all:
    //   char-val = DQUOTE *(%x20-21 / %x23-7E) DQUOTE
    // A backslash is just %x5C, an ordinary member of that range, so
    // `"\"` is the one-character literal every RFC that defines
    // `quoted-pair` writes. With the engine's default JSON-style
    // escaping the backslash swallows the closing quote. There is no
    // "escaping off" switch the three runtimes share, so point the
    // escape character at DEL (%x7F), which is outside char-val's body
    // and so unreachable in any legal ABNF literal.
    //
    // Only the escape character is changed. The set of quote characters
    // the lexer recognises is left at the engine's default, exactly as
    // the canonical runtime leaves it: narrowing it to `"` would be a
    // silent difference in what this parser accepts, and `char-val` is
    // the only place a quote can appear in legal ABNF anyway.
    options.string.escape_char = '\u{7F}';

    // ABNF uses `;` to start a line comment. Override the default `hash`
    // definition and drop the other styles, so `//` and `/* */` are not
    // confused with the alternation operator.
    options.comment.definitions.shift_remove("slash");
    options.comment.definitions.shift_remove("multi");
    if let Some(hash) = options.comment.definitions.get_mut("hash") {
        hash.line = true;
        hash.start = ";".to_string();
        hash.end = String::new();
        hash.lex = true;
        hash.eat_line = false;
    }

    options.rule.start = "abnf".to_string();
    options
}

/// A `%s` / `%i` prefix matcher.
///
/// The canonical TypeScript spells these `^%[sS](?=")`, a lookahead the
/// engine's `regex` dialect has none of, so the prefix is recognised by
/// a callback that consumes the two characters and leaves the `"` for
/// the string lexer, exactly as the Go port does.
fn prefix_matcher(
    letters: &'static str,
) -> impl Fn(&str) -> Option<MatchTokenResult> + Send + Sync + 'static {
    move |rest: &str| {
        let bytes = rest.as_bytes();
        if bytes.len() < 3 || b'%' != bytes[0] || b'"' != bytes[2] {
            return None;
        }
        if !letters.as_bytes().contains(&bytes[1]) {
            return None;
        }
        Some(MatchTokenResult::new(
            &rest[..2],
            Value::String(rest[..2].to_string()),
        ))
    }
}

/// Build the ABNF parser instance: a bare engine carrying only the
/// meta-grammar above.
fn build_abnf_parser() -> Result<Tabnas, String> {
    let mut parser = Tabnas::with_options(abnf_parser_options());

    // Token identities, minted before the grammar names them.
    let num = parser.token("#NUM");
    let numeric = parser.token("#NV");
    let sensitive = parser.token("#SS");
    let insensitive = parser.token("#SI");
    let prose = parser.token("#PV");
    let define = parser.token_with_source("#DEF", "=");
    // `=/` is longer than `=`, so the engine's longest-match-wins fixed
    // matcher tries it first.
    let define_add = parser.token_with_source("#DEFA", "=/");
    let alternation = parser.token_with_source("#ALT", "/");
    let star = parser.token_with_source("#STAR", "*");
    let open_paren = parser.token_with_source("#LP", "(");
    let close_paren = parser.token_with_source("#RP", ")");
    let open_bracket = parser.token_with_source("#OB", "[");
    let close_bracket = parser.token_with_source("#CB", "]");
    let _ = (
        define,
        define_add,
        alternation,
        star,
        open_paren,
        close_paren,
        close_bracket,
    );

    let patterns: [(&str, tabnas::Tin, &str); 2] = [
        // ABNF repetition counts: decimal integers.
        ("#NUM", num, r"^[0-9]+"),
        // ABNF numeric value notation: `%xNN`, `%dNN`, `%bNN`, the
        // `%xNN-NN` range and the `%xNN.NN.NN` concatenation. Digits are
        // permissive (hex covers the decimal and binary subsets);
        // `parse_numeric_value` reads them against the stated base.
        (
            "#NV",
            numeric,
            r"^%[xdbXDB][0-9a-fA-F]+(?:[-.][0-9a-fA-F]+)*",
        ),
    ];
    for (name, tin, pattern) in patterns {
        let regex = Regex::new(pattern).map_err(|error| format!("abnf: {name}: {error}"))?;
        options_insert_match(&mut parser, name, tin, MatchTokenMatcher::Regex(regex));
    }
    // RFC 5234 prose-val: `<` free text `>`. The body is every printable
    // character except `>` itself (%x20-3D / %x3F-7E).
    let prose_pattern = Regex::new("^<[\\x20-\\x3D\\x3F-\\x7E]*>")
        .map_err(|error| format!("abnf: #PV: {error}"))?;
    options_insert_match(
        &mut parser,
        "#PV",
        prose,
        MatchTokenMatcher::Regex(prose_pattern),
    );
    options_insert_match(
        &mut parser,
        "#SS",
        sensitive,
        MatchTokenMatcher::Callback(std::sync::Arc::new(prefix_matcher("sS"))),
    );
    options_insert_match(
        &mut parser,
        "#SI",
        insensitive,
        MatchTokenMatcher::Callback(std::sync::Arc::new(prefix_matcher("iI"))),
    );

    // Tokens that can open an atom. Declaring the set lets `elem.open`
    // and `seq` name `#ATOM` inside an `s` pattern, so the token list at
    // the atom-starter position includes every matcher identity and the
    // lexer does not fall through to `#TX` when the atom is `%xNN`.
    let string = parser.token("#ST");
    let text = parser.token("#TX");
    parser.options.token_set.insert(
        "ATOM".to_string(),
        vec![
            string,
            numeric,
            text,
            open_paren,
            open_bracket,
            sensitive,
            insensitive,
            prose,
        ],
    );

    // Drop the default rules: they would compete with the meta-grammar
    // for the starting token set.
    for name in parser.rule_names() {
        parser.remove_rule(&name);
    }

    register_refs(&mut parser);
    parser
        .grammar_json(GRAMMAR_TEXT)
        .map_err(|error| error.to_string())?;
    Ok(parser)
}

/// Install one match-token matcher.
fn options_insert_match(
    parser: &mut Tabnas,
    name: &str,
    tin: tabnas::Tin,
    matcher: MatchTokenMatcher,
) {
    parser.options.match_tokens.insert(
        name.to_string(),
        MatchToken {
            name: name.to_string(),
            tin,
            matcher,
            eager: false,
        },
    );
}

/// The cached ABNF parser instance, built once.
///
/// `Tabnas` parses through `&self` and is `Send + Sync`, so one instance
/// serves every caller and every thread; per-parse state lives on the
/// rules and the context.
fn abnf_parser() -> Result<&'static Tabnas, String> {
    static PARSER: OnceLock<Result<Tabnas, String>> = OnceLock::new();
    PARSER
        .get_or_init(build_abnf_parser)
        .as_ref()
        .map_err(String::clone)
}

/// How a raw ABNF parse failed.
pub(crate) enum RawError {
    /// The engine rejected the source.
    Engine(Box<TabnasError>),
    /// The parser itself could not be built, or the source nests past
    /// [`MAX_GROUP_DEPTH`].
    Message(String),
}

/// Run the meta-grammar over `src` and return the raw production list,
/// together with the numeric-value diagnostic the parse recorded.
pub(crate) fn parse_abnf_raw(src: &str) -> Result<(Vec<Value>, Option<String>), RawError> {
    let parser = abnf_parser().map_err(RawError::Message)?;
    clear_num_err();
    let parsed = parser.parse(src);
    let num_err = taken_num_err();
    match parsed {
        Ok(value) => {
            let productions = match value {
                Value::Array(items) => items.as_ref().clone(),
                Value::Undefined | Value::Null => Vec::new(),
                other => vec![other],
            };
            Ok((productions, num_err))
        }
        Err(error) if DEPTH_CODE == error.code => Err(RawError::Message(DEPTH_MESSAGE.to_string())),
        Err(error) => Err(RawError::Engine(Box::new(error))),
    }
}
