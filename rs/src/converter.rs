// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

//! The RFC 5234 front-end: ABNF text in, the notation-neutral grammar IR
//! that `tabnas-bnf` compiles out.
//!
//! ```text
//! ABNF text ──parse_abnf──▶ Grammar ──bnf::emit_grammar_spec──▶ GrammarSpec
//! ```
//!
//! Everything downstream of that IR (desugaring, left-recursion
//! elimination, tail repeats, probe dispatch, literal lifting, token
//! allocation, first-set analysis, chain emission) lives in
//! `tabnas-bnf` and is shared with the GBNF and EBNF front-ends. What
//! stays here is what is genuinely ABNF: the meta-grammar in
//! [`crate::parser_abnf`], the Appendix B.1 core rules, incremental
//! alternatives, numeric values, case-insensitive quoted strings and the
//! value annotations a trailing comment can carry.

use std::fmt;

use indexmap::IndexSet;
use regex::Regex;
use serde_json::Value as JsonValue;
use tabnas::Value;
use tabnas_bnf::{refs_in, Grammar, NodeKind, Production};

use crate::parser_abnf::{parse_abnf_raw, RawError};

/// A failure to read ABNF source.
///
/// Carries the line and column the engine reported, where it reported
/// any, so a caller can point at the offending text directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbnfParseError {
    /// The rendered diagnostic, always prefixed `abnf: `.
    pub message: String,
    /// The 1-based line, when the underlying failure named one.
    pub line: Option<usize>,
    /// The 1-based column, when the underlying failure named one.
    pub column: Option<usize>,
    /// The engine's own error code, when the failure came from the
    /// engine rather than from this front-end.
    pub code: Option<String>,
}

impl AbnfParseError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
            code: None,
        }
    }
}

impl fmt::Display for AbnfParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AbnfParseError {}

// ---- parse ----------------------------------------------------------

/// Parse ABNF source into the grammar IR.
///
/// The order of the checks below is itself part of the contract, and
/// `test/spec/alignment-abnf-errors.tsv` pins it in every runtime: the
/// numeric diagnostic first, then the incremental-merge refusal, then
/// the malformed element. A rejection naming a different cause in each
/// runtime is the divergence that order exists to close.
pub fn parse_abnf(src: &str) -> Result<Grammar, AbnfParseError> {
    let (productions, num_err) = match parse_abnf_raw(src) {
        Ok(parsed) => parsed,
        Err(RawError::Message(message)) => return Err(AbnfParseError::new(message)),
        Err(RawError::Engine(error)) => {
            let line = error.row;
            let column = error.col;
            let location = if 0 != line && 0 != column {
                format!(" at line {line}, column {column}")
            } else {
                String::new()
            };
            let rendered = error.to_string();
            let raw = rendered.lines().next().unwrap_or_default();
            return Err(AbnfParseError {
                message: format!("abnf: parse error{location}: {raw}"),
                line: (0 != line).then_some(line),
                column: (0 != column).then_some(column),
                code: Some(error.code.clone()),
            });
        }
    };

    if productions.is_empty() {
        return Err(AbnfParseError::new("abnf: no productions found"));
    }

    // Surface the numeric-value diagnostic the parse recorded. The
    // canonical runtime raises it from inside the decoding action, so it
    // precedes every check below even when the element it was decoded
    // into was dropped with an unclosed group.
    if let Some(message) = num_err {
        return Err(AbnfParseError::new(format!("abnf: parse error: {message}")));
    }

    // BEFORE merging, not after. `merge_incrementals` drops each `=/`
    // production, keeping only the base's span, so an annotation on an
    // incremental line would be resolved against a production list that
    // no longer contained the line it followed and would attach to
    // whatever rule happened to be declared before it instead.
    let mut productions = productions;
    attach_value_annotations(src, &mut productions)?;
    let merged = merge_incrementals(productions)?;
    reject_holes(&merged)?;

    let mut typed = Vec::with_capacity(merged.len());
    for production in &merged {
        typed.push(production_from_value(production)?);
    }
    Ok(Grammar::new(with_core_rules(typed)))
}

// ---- value annotations ----------------------------------------------

/// A trailing comment claiming a value annotation:
///
/// ```abnf
/// ver = maj "." min "." pat    ; @object maj min pat
/// tags = tag *("," tag)        ; @array
/// ```
///
/// RFC 5234 has nowhere else to put this. A comment is the only place in
/// the notation that carries no meaning of its own, which is exactly why
/// it can carry one here without changing what the grammar accepts:
/// strip every annotation and the same language parses, just into a tree
/// instead of a value.
///
/// ONLY `@object` and `@array` are claimed. Any other `; @…` comment is
/// left alone: the notation has no directive namespace, so this must not
/// assume one, and a reader's own `; @deprecated` has to keep meaning
/// nothing.
fn annotation_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^@(object|array)\b\s*(.*)$").expect("the annotation pattern is valid")
    })
}

fn member_name_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^[A-Za-z][A-Za-z0-9-]*$").expect("the member-name pattern is valid")
    })
}

/// Every `;` comment in the source that claims an annotation, with the
/// byte offset it starts at.
///
/// Quoted strings and prose are skipped: a `;` inside `"a;b"` or `<a;b>`
/// is CONTENT, not a comment, and treating it as one would silently
/// attach an annotation the author did not write.
fn annotation_comments(src: &str) -> Vec<(usize, String)> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            quote @ (b'"' | b'<') => {
                // RFC 5234 char-val and prose-val have no escapes, so the
                // next closing mark ends them.
                let closer = if b'<' == quote { b'>' } else { b'"' };
                match bytes[index + 1..].iter().position(|byte| closer == *byte) {
                    Some(offset) => index += offset + 2,
                    None => return out,
                }
            }
            b';' => {
                let end = bytes[index..]
                    .iter()
                    .position(|byte| b'\n' == *byte)
                    .map_or(bytes.len(), |offset| index + offset);
                let body = src[index + 1..end].trim().to_string();
                if body.starts_with('@') {
                    out.push((index, body));
                }
                index = end + 1;
            }
            _ => index += 1,
        }
    }
    out
}

/// Attach each annotation to the production it FOLLOWS: the last one
/// that begins before it.
///
/// Not "the production on the same line". A rule may be written across
/// several lines, and an author putting the annotation on the last of
/// them means the same thing. Following the definition is the rule that
/// reads the same either way.
fn attach_value_annotations(src: &str, prods: &mut [Value]) -> Result<(), AbnfParseError> {
    let mut ordered: Vec<usize> = (0..prods.len())
        .filter(|index| span_start(&prods[*index]).is_some())
        .collect();
    if ordered.is_empty() {
        return Ok(());
    }
    ordered.sort_by_key(|index| span_start(&prods[*index]).unwrap_or(0));

    // Both `ordered` and the comments are in source order, so the search
    // only ever moves FORWARD: `cursor` is not reset per comment.
    // Restarting it made attachment quadratic in the number of annotated
    // rules, which a generated grammar can make expensive for nothing.
    let mut cursor = 0;
    for (at, body) in annotation_comments(src) {
        let Some(captures) = annotation_pattern().captures(&body) else {
            continue;
        };

        while cursor < ordered.len() && span_start(&prods[ordered[cursor]]).unwrap_or(0) < at {
            cursor += 1;
        }
        if 0 == cursor {
            return Err(AbnfParseError::new(format!(
                "abnf: '; {body}' appears before any rule, so there is nothing for it to \
                 annotate. A value annotation goes after the rule it describes."
            )));
        }
        let owner = ordered[cursor - 1];
        let name = production_name(&prods[owner]);

        let kind = captures.get(1).map_or("", |group| group.as_str());
        let members: Vec<String> = captures
            .get(2)
            .map_or("", |group| group.as_str())
            .split([' ', '\t', ','])
            .filter(|word| !word.is_empty())
            .map(str::to_string)
            .collect();

        if "array" == kind {
            if !members.is_empty() {
                return Err(AbnfParseError::new(format!(
                    "abnf: rule '{name}': '@array' names no members \u{2014} every part that \
                     produces a value becomes an element, in order. Got '{}'.",
                    members.join(" ")
                )));
            }
        } else {
            let mut seen = IndexSet::new();
            for member in &members {
                if !member_name_pattern().is_match(member) {
                    return Err(AbnfParseError::new(format!(
                        "abnf: rule '{name}': '{member}' is not a rule name, so it cannot name a \
                         member of '@object'."
                    )));
                }
                // Each member is a separate KEY. Two parts named the same
                // thing both write to it, so the second silently
                // overwrites the first and that much of the input is gone
                // from the result.
                if !seen.insert(member.clone()) {
                    return Err(AbnfParseError::new(format!(
                        "abnf: rule '{name}': '@object' names '{member}' twice. Each member is a \
                         separate key, so the second part would overwrite the first. Give them \
                         different names."
                    )));
                }
            }
        }

        if has_field(&prods[owner], "value") {
            return Err(AbnfParseError::new(format!(
                "abnf: rule '{name}' has more than one value annotation. A rule builds one thing."
            )));
        }
        let annotation = if "array" == kind {
            annotation_value("array", &[])
        } else {
            annotation_value("object", &members)
        };
        set_field(&mut prods[owner], "value", annotation);
    }
    Ok(())
}

/// The IR shape of a value annotation.
fn annotation_value(kind: &str, members: &[String]) -> Value {
    let mut map = indexmap::IndexMap::new();
    map.insert("kind".to_string(), Value::String(kind.to_string()));
    if !members.is_empty() {
        map.insert(
            "members".to_string(),
            Value::array(
                members
                    .iter()
                    .map(|member| Value::String(member.clone()))
                    .collect(),
            ),
        );
    }
    Value::object(map)
}

// ---- incremental alternatives ---------------------------------------

/// Fold every `name =/ alt` production into the earlier production with
/// the same name by appending its alternatives. Refuses an incremental
/// that references a name not yet defined: RFC 5234 requires the base
/// production to appear first.
fn merge_incrementals(prods: Vec<Value>) -> Result<Vec<Value>, AbnfParseError> {
    let mut out: Vec<Value> = Vec::with_capacity(prods.len());
    let mut by_name: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    for production in prods {
        let name = production_name(&production);
        if matches!(field(&production, "incremental"), Some(Value::Bool(true))) {
            let Some(base) = by_name.get(&name).copied() else {
                return Err(AbnfParseError::new(format!(
                    "abnf: '{name} =/ \u{2026}' has no earlier '{name} = \u{2026}' to extend"
                )));
            };
            let extra = match field(&production, "alts") {
                Some(Value::Array(alts)) => alts.as_ref().clone(),
                _ => Vec::new(),
            };
            if let Some(Value::Array(existing)) = field(&out[base], "alts") {
                let mut merged = existing.as_ref().clone();
                merged.extend(extra);
                set_field(&mut out[base], "alts", Value::array(merged));
            }
            // This production is about to be dropped, and annotations are
            // attached before that happens, so an annotation on the `=/`
            // line has to move to the base, which IS the rule it
            // describes.
            if let Some(value) = field(&production, "value") {
                if has_field(&out[base], "value") {
                    return Err(AbnfParseError::new(format!(
                        "abnf: rule '{name}' has more than one value annotation. A rule builds \
                         one thing."
                    )));
                }
                set_field(&mut out[base], "value", value);
            }
            continue;
        }
        // Rebuilt field by field, so every field carried on a production
        // has to be listed here or it is silently dropped, `sp` included.
        let mut clean = indexmap::IndexMap::new();
        clean.insert("name".to_string(), Value::String(name.clone()));
        clean.insert(
            "alts".to_string(),
            field(&production, "alts").unwrap_or_else(|| Value::array(Vec::new())),
        );
        if let Some(span) = field(&production, "sp") {
            clean.insert("sp".to_string(), span);
        }
        if let Some(node_kind) = field(&production, "nodeKind") {
            clean.insert("nodeKind".to_string(), node_kind);
        }
        // Annotations are attached BEFORE this runs, so this carry is
        // live: without it every annotation in the grammar would vanish
        // here without a word.
        if let Some(value) = field(&production, "value") {
            clean.insert("value".to_string(), value);
        }
        by_name.insert(name, out.len());
        out.push(Value::object(clean));
    }
    Ok(out)
}

// ---- malformed elements ---------------------------------------------

/// Refuse a production holding an element the parser could not build.
///
/// A malformed element reaches here as a HOLE in an alternative: an
/// unclosed group (`( "a" / "b"` with no `)`) pops without ever building
/// its node, leaving nothing in the sequence. The walk mirrors the
/// shared compiler's `refs_in` exactly, because "where does a walk
/// dereference?" is the definition of where a hole is fatal. A top-level
/// scan of the sequence is not enough: when a repetition wraps the
/// unclosed group (`bad = *( "a"`) the close builds a perfectly real
/// `star` whose `inner` is the hole, so the sequence entry is present
/// and the walk has to go one level down.
fn reject_holes(prods: &[Value]) -> Result<(), AbnfParseError> {
    fn hole_in(alt: &Value) -> bool {
        let Value::Array(elements) = alt else {
            return true;
        };
        for element in elements.iter() {
            if element.is_undefined() || element.is_null() {
                return true;
            }
            match field(element, "kind") {
                Some(Value::String(kind))
                    if matches!(kind.as_str(), "opt" | "star" | "plus" | "rep") =>
                {
                    let inner = field(element, "inner").unwrap_or(Value::Undefined);
                    if hole_in(&Value::array(vec![inner])) {
                        return true;
                    }
                }
                Some(Value::String(kind)) if "group" == kind => {
                    let Some(Value::Array(alts)) = field(element, "alts") else {
                        return true;
                    };
                    for inner in alts.iter() {
                        if hole_in(inner) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    for production in prods {
        let name = production_name(production);
        let Some(Value::Array(alts)) = field(production, "alts") else {
            return Err(AbnfParseError::new(format!(
                "abnf: rule '{name}' is malformed \u{2014} no alternatives were built."
            )));
        };
        for alt in alts.iter() {
            if hole_in(alt) {
                return Err(AbnfParseError::new(format!(
                    "abnf: rule '{name}' is malformed \u{2014} an element could not be built. The \
                     usual cause is an unclosed group or option."
                )));
            }
        }
    }
    Ok(())
}

// ---- core rules -----------------------------------------------------

/// RFC 5234 Appendix B.1 core rules, parsed on first use and spliced
/// into any user grammar that references them but does not define them.
const CORE_RULES_ABNF: &str = r#"
ALPHA  = %x41-5A / %x61-7A
BIT    = "0" / "1"
CHAR   = %x01-7F
CR     = %x0D
LF     = %x0A
CRLF   = CR LF
CTL    = %x00-1F / %x7F
DIGIT  = %x30-39
DQUOTE = %x22
HEXDIG = DIGIT / "A" / "B" / "C" / "D" / "E" / "F"
HTAB   = %x09
OCTET  = %x00-FF
SP     = %x20
VCHAR  = %x21-7E
WSP    = SP / HTAB
LWSP   = *( WSP / CRLF WSP )
"#;

/// The core rules, in declaration order.
///
/// Parsed once and cloned per grammar. They flatten to `src` in the
/// output AST: they are character-class bricks, not structural nodes a
/// reader wants one of per matched character.
///
/// Their source spans are stripped. They are parsed from a string in
/// THIS FILE, so their offsets index a document the user never wrote,
/// and an editor asked to reveal one would jump to a position in the
/// user's grammar that has nothing to do with `ALPHA` or `DIGIT`. A
/// missing span means "nowhere to point", which is exactly right for a
/// rule the library supplied; a wrong one is worse than none. A
/// reference TO a core rule still carries a span, because that reference
/// is in the user's source and is what a diagnostic points at.
fn core_rules() -> &'static Vec<Production> {
    static CORE: std::sync::OnceLock<Vec<Production>> = std::sync::OnceLock::new();
    CORE.get_or_init(|| {
        let (raw, _) = parse_abnf_raw(CORE_RULES_ABNF)
            .unwrap_or_else(|_| panic!("abnf: internal: the core rules failed to parse"));
        raw.iter()
            .map(|value| {
                let mut production = production_from_value(value)
                    .unwrap_or_else(|error| panic!("abnf: internal: core rule: {error}"));
                production.node_kind = NodeKind::Core;
                strip_spans(&mut production);
                production
            })
            .collect()
    })
}

/// Remove every span from a production and everything under it.
fn strip_spans(production: &mut Production) {
    fn walk(element: &mut tabnas_bnf::Element) {
        element.sp = None;
        match &mut element.kind {
            tabnas_bnf::Kind::Opt { inner }
            | tabnas_bnf::Kind::Star { inner, .. }
            | tabnas_bnf::Kind::Plus { inner }
            | tabnas_bnf::Kind::Rep { inner, .. } => walk(inner),
            tabnas_bnf::Kind::Group { alts } => {
                for alt in alts {
                    for inner in alt {
                        walk(inner);
                    }
                }
            }
            _ => {}
        }
    }
    production.sp = None;
    for alt in &mut production.alts {
        for element in alt {
            walk(element);
        }
    }
}

/// Add each RFC 5234 core rule the user's grammar references but does
/// not define locally. Resolution is transitive: mention `HEXDIG` and
/// `DIGIT` comes too. User definitions always win, so a local
/// `DIGIT = …` is left untouched.
fn with_core_rules(user: Vec<Production>) -> Vec<Production> {
    let core = core_rules();
    let mut defined: IndexSet<String> = user
        .iter()
        .map(|production| production.name.clone())
        .collect();
    let mut needed: IndexSet<String> = IndexSet::new();
    for production in &user {
        for alt in &production.alts {
            refs_in(alt, &mut needed);
        }
    }

    let mut out: Vec<Production> = Vec::new();
    let mut added = true;
    while added {
        added = false;
        for production in core {
            if defined.contains(&production.name) || !needed.contains(&production.name) {
                continue;
            }
            defined.insert(production.name.clone());
            // A COPY, never the cached production: the cache is handed
            // out to every grammar compiled in this process, and a
            // consumer that annotated an `ALPHA` node would otherwise see
            // that annotation on unrelated documents.
            let copy = production.clone();
            for alt in &copy.alts {
                refs_in(alt, &mut needed);
            }
            out.push(copy);
            added = true;
        }
    }

    let mut all = user;
    all.extend(out);
    all
}

// ---- value helpers --------------------------------------------------

/// One named field of an object value.
fn field(value: &Value, key: &str) -> Option<Value> {
    match value {
        Value::Object(entries) => entries.get(key).cloned(),
        _ => None,
    }
}

fn has_field(value: &Value, key: &str) -> bool {
    field(value, key).is_some()
}

fn set_field(value: &mut Value, key: &str, field: Value) {
    if let Some(entries) = value.as_object_mut() {
        entries.insert(key.to_string(), field);
    }
}

fn production_name(value: &Value) -> String {
    match field(value, "name") {
        Some(Value::String(name)) => name,
        _ => String::new(),
    }
}

fn span_start(value: &Value) -> Option<usize> {
    match field(&field(value, "sp")?, "s") {
        Some(Value::Number(start)) => Some(start as usize),
        _ => None,
    }
}

/// Read one parsed production into the typed IR.
///
/// The parse AST is built as engine values in exactly the shape the IR
/// serializes to, so this is a deserialization rather than a
/// translation, and the two representations cannot drift apart.
fn production_from_value(value: &Value) -> Result<Production, AbnfParseError> {
    let mut json: JsonValue = value.to_json();
    integral(&mut json);
    let name = production_name(value);
    serde_json::from_value(json).map_err(|error| {
        AbnfParseError::new(format!(
            "abnf: rule '{name}' is malformed \u{2014} {error}."
        ))
    })
}

/// Rewrite every whole number in a JSON tree as an integer.
///
/// The engine's number is a double, so a span offset and a repetition
/// count both arrive as `8.0`, which serde will not read into a `usize`.
/// Nothing in the IR is a fractional number, so the conversion is total
/// rather than a heuristic.
fn integral(value: &mut JsonValue) {
    match value {
        JsonValue::Number(number) => {
            if let Some(float) = number.as_f64() {
                if 0.0 == float.fract() && float.is_finite() && 0.0 <= float {
                    *value = JsonValue::Number((float as u64).into());
                }
            }
        }
        JsonValue::Array(items) => items.iter_mut().for_each(integral),
        JsonValue::Object(entries) => entries.values_mut().for_each(integral),
        _ => {}
    }
}
