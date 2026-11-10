// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

//! An ABNF document read as the schema `grammar-rules`: its rules as the
//! text writes them, uncompiled and in source order (tabnas/bnf's
//! `docs/grammar-rules.md`, and section 13.1 of alchemy's
//! `docs/streaming-plan.md`). The Rust port of `ts/src/tree.ts`: the tree
//! a host translates, whose events the part, `alchemy/transduce.alc`,
//! makes from the rule passes of [`make`].
//!
//! The tree parser reads ABNF with the meta-grammar the IR parser reads it
//! with, its lexer and every alternate as they are, and actions of its own
//! under the names the IR parser's take, so the rule passes a part reads
//! are the meta-grammar's. A numeric value is read by its atom's action,
//! which gives its token the element it spells as its value.
//!
//! The tree holds what the schema allows, so the parser refuses what it
//! cannot hold, each with a code of its own: a numeric value past U+10FFFF
//! (`abnf_code_point`), a range that ends below where it starts
//! (`abnf_reversed_range`), a repetition count past 2^53 - 1
//! (`abnf_count`), a repetition whose most is below its least
//! (`abnf_reversed_repetition`), a group or an option left open
//! (`abnf_unclosed`), and a concatenation holding half of a surrogate pair
//! alone, which names no character (`abnf_surrogate`). The IR parser's
//! nesting cap holds here too (`abnf_group_depth`).

use std::rc::Rc;
use std::sync::OnceLock;

use indexmap::IndexMap;
use tabnas::{ActionError, Rule, Tabnas, Value};

use crate::converter::AbnfParseError;
use crate::parser_abnf::{
    build_parser, open_token, push_node, set_node, token_string, DEPTH_CODE, DEPTH_MESSAGE,
    MAX_GROUP_DEPTH,
};

/// The largest repetition count the tree holds exactly: 2^53 - 1.
const MAX_COUNT: u64 = 9_007_199_254_740_991;

/// The codes the tree parser refuses a source with, beside the engine's:
/// each with its message and its hint.
const TREE_ERRORS: [(&str, &str, &str); 6] = [
    (
        "abnf_code_point",
        "the numeric value {src} names no Unicode code point (the largest is %x10FFFF)",
        "A numeric value names a code point from %x0 to %x10FFFF.",
    ),
    (
        "abnf_reversed_range",
        "the numeric range {src} ends below where it starts",
        "Write the range from its lower end to its higher, as %x61-7A.",
    ),
    (
        "abnf_reversed_repetition",
        "the repetition allows fewer than it requires: its most is below its least",
        "In m*n the n is the most and the m the least, so n is not below m.",
    ),
    (
        "abnf_count",
        "the repetition count {src} is past 9007199254740991, the largest whole number the tree holds",
        "A repetition count is a whole number no larger than 9007199254740991.",
    ),
    (
        "abnf_unclosed",
        "a group or an option is not closed",
        "Close the group with ) and the option with ].",
    ),
    (
        "abnf_surrogate",
        "the numeric value {src} holds half of a surrogate pair standing alone, which names no \
         character",
        "A concatenation holds a surrogate as one half of a pair, as %xD83D.DE00; one value \
         alone, as %xD800, is a code point of its own.",
    ),
];

/// An object of the tree, its members in the order given.
fn object(fields: Vec<(&str, Value)>) -> Value {
    let mut map = IndexMap::new();
    for (key, value) in fields {
        map.insert(key.to_string(), value);
    }
    Value::object(map)
}

/// The error a code raises, with its message as the detail.
fn refuse(code: &str) -> ActionError {
    let message = TREE_ERRORS
        .iter()
        .find(|(name, _, _)| *name == code)
        .map_or(code, |(_, message, _)| message);
    ActionError::new(code, message.to_string())
}

/// The longest prefix of `text` that is digits in `radix`, as the
/// canonical `parseInt(text, radix)` reads it, while it is no larger than
/// `limit`; `None` where there is no digit or it is larger.
fn leading_int(text: &str, radix: u32, limit: u64) -> Option<u64> {
    let mut value: u64 = 0;
    let mut digits = 0;
    for character in text.chars() {
        let Some(digit) = character.to_digit(radix) else {
            break;
        };
        digits += 1;
        value = value
            .checked_mul(u64::from(radix))
            .and_then(|n| n.checked_add(u64::from(digit)))
            .filter(|n| *n <= limit)?;
    }
    (0 < digits).then_some(value)
}

/// The element a numeric value spells, or the code it is refused with.
/// The digits are read as the canonical runtime reads them, the longest
/// prefix valid in the stated base, and a concatenation's code points as
/// one UTF-16 sequence, so that a surrogate pair in it is the character
/// it encodes; a half standing alone names no character, and is refused.
pub(crate) fn numeric_element(src: &str) -> Result<Value, &'static str> {
    let radix = match src.as_bytes().get(1).map(u8::to_ascii_lowercase) {
        Some(b'd') => 10,
        Some(b'b') => 2,
        _ => 16,
    };
    let body = src.get(2..).unwrap_or_default();
    let point = |text: &str| leading_int(text, radix, 0x10FFFF);

    if body.contains('-') {
        let mut parts = body.split('-');
        let from = point(parts.next().unwrap_or_default());
        let to = point(parts.next().unwrap_or_default());
        let (Some(from), Some(to)) = (from, to) else {
            return Err("abnf_code_point");
        };
        if to < from {
            return Err("abnf_reversed_range");
        }
        return Ok(object(vec![
            ("type", Value::String("range".into())),
            ("from", Value::Number(from as f64)),
            ("to", Value::Number(to as f64)),
        ]));
    }

    let mut points = Vec::new();
    for part in body.split('.') {
        points.push(point(part).ok_or("abnf_code_point")?);
    }
    if 1 == points.len() {
        return Ok(object(vec![
            ("type", Value::String("char".into())),
            ("value", Value::Number(points[0] as f64)),
        ]));
    }
    let mut units: Vec<u16> = Vec::new();
    for n in points {
        match char::from_u32(n as u32) {
            Some(character) => {
                let mut buffer = [0u16; 2];
                units.extend_from_slice(character.encode_utf16(&mut buffer));
            }
            // A surrogate, which pairs with its neighbour or stands alone.
            None => units.push(n as u16),
        }
    }
    let text = String::from_utf16(&units).map_err(|_| "abnf_surrogate")?;
    // A concatenation that spells one character (a surrogate pair's two
    // halves) is that character.
    let mut chars = text.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        return Ok(object(vec![
            ("type", Value::String("char".into())),
            ("value", Value::Number(f64::from(u32::from(only)))),
        ]));
    }
    Ok(object(vec![
        ("type", Value::String("literal".into())),
        ("text", Value::String(text)),
        ("caseSensitive", Value::Bool(true)),
    ]))
}

/// A repetition count, from its `#NUM` token.
fn count(rule: &Rule, index: usize) -> Result<f64, ActionError> {
    let token = open_token(rule, index).ok_or_else(|| refuse("abnf_count"))?;
    leading_int(token.src.as_str(), 10, MAX_COUNT)
        .map(|n| n as f64)
        .ok_or_else(|| refuse("abnf_count"))
}

/// A repetition, `min` and, where it has one, `max`.
fn repetition(min: f64, max: Option<f64>) -> Value {
    let mut fields = vec![("min", Value::Number(min))];
    if let Some(max) = max {
        fields.push(("max", Value::Number(max)));
    }
    object(fields)
}

fn set_repeat(rule: &mut Rule, rep: Value) -> Result<(), ActionError> {
    rule.u_mut().insert("repeat".into(), rep);
    Ok(())
}

/// A literal element.
fn literal(text: String, sensitive: bool) -> Value {
    object(vec![
        ("type", Value::String("literal".into())),
        ("text", Value::String(text)),
        ("caseSensitive", Value::Bool(sensitive)),
    ])
}

/// Note which bracket opened this atom, and refuse a source that nests
/// them past the cap the IR parser holds.
fn open_group(rule: &mut Rule, kind: &str) -> Result<(), ActionError> {
    if MAX_GROUP_DEPTH < rule.d {
        return Err(ActionError::new(DEPTH_CODE, DEPTH_MESSAGE));
    }
    rule.u_mut()
        .insert("groupKind".into(), Value::String(kind.into()));
    Ok(())
}

/// Register the tree parser's actions under the names the meta-grammar's
/// document gives them.
fn register_tree_refs(parser: &mut Tabnas) {
    parser.state_action_ref("@abnf-bo", |rule, _context| {
        set_node(
            rule,
            object(vec![
                ("type", Value::String("grammar".into())),
                ("rules", Value::array(Vec::new())),
            ]),
        );
        Ok(())
    });

    parser.action_with_context("@prod-name", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        rule.u_mut().insert("name".into(), Value::String(name));
        Ok(())
    });
    // `<all> = <remove>`: the name keeps its angle brackets.
    parser.action_with_context("@prod-name-prose", |rule, _context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        rule.u_mut()
            .insert("name".into(), Value::String(token.src.as_str().to_string()));
        Ok(())
    });
    parser.action_with_context("@prod-name-inc", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        let bag = rule.u_mut();
        bag.insert("name".into(), Value::String(name));
        bag.insert("incremental".into(), Value::Bool(true));
        Ok(())
    });
    parser.state_action_ref("@prod-bc", |rule, _context| {
        if rule.child_node.is_undefined() {
            return Ok(());
        }
        let name = match rule.u.get("name") {
            Some(Value::String(name)) => name.clone(),
            _ => String::new(),
        };
        let mut fields = vec![
            ("type", Value::String("rule".into())),
            ("name", Value::String(name)),
        ];
        if matches!(rule.u.get("incremental"), Some(Value::Bool(true))) {
            fields.push(("incremental", Value::Bool(true)));
        }
        fields.push(("alternatives", rule.child_node.clone()));
        let entry = object(fields);
        if let Some(grammar) = rule.node.borrow_mut().as_object_mut() {
            if let Some(rules) = grammar.get_mut("rules").and_then(Value::as_array_mut) {
                rules.push(entry);
            }
        }
        Ok(())
    });

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

    parser.state_action_ref("@seq-bo", |rule, _context| {
        set_node(rule, Value::array(Vec::new()));
        Ok(())
    });

    parser.state_action_ref("@elem-bo", |rule, _context| {
        rule.u_mut().remove("repeat");
        Ok(())
    });
    parser.action_with_context("@elem-rep-bounded", |rule, _context| {
        let min = count(rule, 0)?;
        let max = count(rule, 2)?;
        if max < min {
            return Err(refuse("abnf_reversed_repetition"));
        }
        set_repeat(rule, repetition(min, Some(max)))
    });
    parser.action_with_context("@elem-rep-atleast", |rule, _context| {
        let min = count(rule, 0)?;
        set_repeat(rule, repetition(min, None))
    });
    parser.action_with_context("@elem-rep-atmost", |rule, _context| {
        let max = count(rule, 1)?;
        set_repeat(rule, repetition(0.0, Some(max)))
    });
    parser.action_with_context("@elem-rep-star", |rule, _context| {
        set_repeat(rule, repetition(0.0, None))
    });
    parser.action_with_context("@elem-rep-exact", |rule, _context| {
        let n = count(rule, 0)?;
        set_repeat(rule, repetition(n, Some(n)))
    });
    parser.action_with_context("@elem-close", |rule, _context| {
        let mut element = rule.child_node.clone();
        // An atom that built nothing is a group or an option the source
        // ended inside.
        if element.is_undefined() {
            return Err(refuse("abnf_unclosed"));
        }
        if let Some(rep) = rule.u.get("repeat").cloned() {
            if let Some(members) = element.as_object_mut() {
                match members.get_mut("repeat").and_then(Value::as_array_mut) {
                    Some(reps) => reps.push(rep),
                    None => {
                        members.insert("repeat".into(), Value::array(vec![rep]));
                    }
                }
            }
        }
        push_node(rule, element);
        Ok(())
    });

    parser.state_action_ref("@atom-bo", |rule, _context| {
        set_node(rule, Value::Undefined);
        Ok(())
    });
    parser.action_with_context("@atom-ss", |rule, context| {
        let Some(text) = open_token(rule, 1) else {
            return Ok(());
        };
        let text = token_string(&text, rule, context);
        set_node(rule, literal(text, true));
        Ok(())
    });
    parser.action_with_context("@atom-si", |rule, context| {
        let Some(text) = open_token(rule, 1) else {
            return Ok(());
        };
        let text = token_string(&text, rule, context);
        set_node(rule, literal(text, false));
        Ok(())
    });
    parser.action_with_context("@atom-st", |rule, context| {
        let Some(text) = open_token(rule, 0) else {
            return Ok(());
        };
        let text = token_string(&text, rule, context);
        set_node(rule, literal(text, false));
        Ok(())
    });
    // The element the value spells, which its token carries from here.
    parser.action_with_context("@atom-nv", |rule, _context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let element = numeric_element(token.src.as_str()).map_err(refuse)?;
        if let Some(first) = Rc::make_mut(&mut rule.o).first_mut() {
            first.val = element.clone();
        }
        set_node(rule, element);
        Ok(())
    });
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
        set_node(
            rule,
            object(vec![
                ("type", Value::String("prose".into())),
                ("text", Value::String(text)),
            ]),
        );
        Ok(())
    });
    parser.action_with_context("@atom-tx", |rule, context| {
        let Some(token) = open_token(rule, 0) else {
            return Ok(());
        };
        let name = token_string(&token, rule, context);
        set_node(
            rule,
            object(vec![
                ("type", Value::String("reference".into())),
                ("name", Value::String(name)),
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
        let alternatives = rule.child_node.clone();
        set_node(
            rule,
            object(vec![
                ("type", Value::String("group".into())),
                ("alternatives", alternatives),
            ]),
        );
        Ok(())
    });
    parser.alt_condition("@atom-opt-c", |rule, _context| {
        matches!(rule.u.get("groupKind"), Some(Value::String(kind)) if "opt" == kind)
    });
    parser.action_with_context("@atom-opt-close", |rule, _context| {
        let alternatives = rule.child_node.clone();
        set_node(
            rule,
            object(vec![
                ("type", Value::String("group".into())),
                ("alternatives", alternatives),
                ("repeat", Value::array(vec![repetition(0.0, Some(1.0))])),
            ]),
        );
        Ok(())
    });
}

/// A fresh parser that reads an ABNF document as the tree: the parser a
/// host streams a document through with the part, whose rule passes the
/// part reads, and whose value is the tree (`Undefined` for an empty
/// source, which makes no pass).
///
/// ```
/// let tree = tabnas_abnf::make().parse("a = b").unwrap();
/// assert_eq!(tree.to_json()["rules"][0]["name"], "a");
/// ```
#[must_use]
pub fn make() -> Tabnas {
    // The meta-grammar is this crate's own, so it always installs.
    build_parser(register_tree_refs, &TREE_ERRORS)
        .expect("abnf: the tree parser installs its own meta-grammar")
}

/// An ABNF document read whole as the tree. A source the meta-grammar
/// refuses, or the tree cannot hold, fails with an [`AbnfParseError`]
/// naming the line and column, its `code` the engine's.
///
/// ```
/// let tree = tabnas_abnf::parse_abnf_tree("a = *b\n").unwrap();
/// let rule = &tree.to_json()["rules"][0];
/// assert_eq!(rule["alternatives"][0][0]["repeat"][0]["min"], 0.0);
/// ```
pub fn parse_abnf_tree(src: &str) -> Result<Value, AbnfParseError> {
    static PARSER: OnceLock<Tabnas> = OnceLock::new();
    let parser = PARSER.get_or_init(make);
    match parser.parse(src) {
        Ok(Value::Undefined) | Ok(Value::Null) => Ok(object(vec![
            ("type", Value::String("grammar".into())),
            ("rules", Value::array(Vec::new())),
        ])),
        Ok(tree) => Ok(tree),
        Err(error) => Err(AbnfParseError::from_engine(&error)),
    }
}
