// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

//! An ABNF (RFC 5234) grammar compiler for the
//! [`tabnas`](https://github.com/tabnas/parser) parsing engine.
//!
//! Where most tabnas grammar crates carry one fixed grammar, this one is
//! a META compiler: the grammar it installs is whatever ABNF text it is
//! handed at run time.
//!
//! ```text
//! ABNF text ──parse_abnf──▶ Grammar ──emit_grammar_spec──▶ GrammarSpec
//! ```
//!
//! [`parse_abnf`] is the RFC 5234 front-end and is what this crate adds;
//! everything downstream of the IR lives in
//! [`tabnas_bnf`](https://github.com/tabnas/bnf) and is shared with the
//! GBNF and EBNF front-ends.
//!
//! ```
//! let mut parser = tabnas::Tabnas::new();
//! tabnas_abnf::abnf(&mut parser, "greet = \"hi\" / \"hello\"", None).unwrap();
//! let tree = parser.parse("hello").unwrap();
//! assert_eq!(tree.to_json()["rule"], "greet");
//! ```
//!
//! This is the Rust port of the canonical TypeScript implementation in
//! `ts/src`; the TypeScript version is authoritative and this crate
//! tracks it.

mod compile;
mod converter;
mod numeric;
mod parser_abnf;

/// The README's Rust examples run as doctests, so a stale one fails the
/// gate rather than misleading the reader. Its `toml` and `bash` fences
/// are skipped; rustdoc runs only the `rust` ones.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_examples {}

use std::fmt;

use tabnas::{Plugin, PluginError, Tabnas, Value};

pub use compile::abnf_compile;
pub use converter::{parse_abnf, AbnfParseError};
pub use parser_abnf::abnf_rules;

pub use tabnas_bnf::{
    attach_action_slots, attach_actions, eliminate_left_recursion, mark_listing, to_jsonic,
    to_pure_spec, to_recognition_spec, ActionError as AbnfActionError, ActionFn, ActionsMap,
    AltSpec, CompileError as AbnfCompileError, ConvertOptions as AbnfConvertOptions, Element,
    EmitError, Grammar, GrammarSpec, JsonicOptions, Kind, NodeKind, Production, RefAction,
    RuleSpec, Sequence, SrcSpan, ValueAnnotation,
};

/// This crate's version. It MUST equal `ts/package.json` "version": the
/// release orchestrator rewrites both, and `tests/version_test.rs` fails
/// the build if they drift. Mirrors `VERSION` in `ts/src/abnf.ts` and
/// `const VERSION` in `go/abnf.go`.
pub const VERSION: &str = "0.4.13";

/// The group tag stamped on every emitted alt, and the prefix of every
/// diagnostic this crate raises through the shared compiler.
pub const TAG: &str = "abnf";

/// Anything that can go wrong turning ABNF source into a grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum AbnfError {
    /// The ABNF source itself could not be read.
    Parse(AbnfParseError),
    /// The shared compiler refused the grammar.
    Emit(EmitError),
    /// The emitted spec could not be lowered to pure data.
    Compile(AbnfCompileError),
    /// A user action could not be attached.
    Action(AbnfActionError),
    /// The engine refused the emitted grammar.
    Install(String),
}

impl fmt::Display for AbnfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(formatter),
            Self::Emit(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Action(error) => error.fmt(formatter),
            Self::Install(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AbnfError {}

impl From<AbnfParseError> for AbnfError {
    fn from(error: AbnfParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<EmitError> for AbnfError {
    fn from(error: EmitError) -> Self {
        Self::Emit(error)
    }
}

impl From<AbnfCompileError> for AbnfError {
    fn from(error: AbnfCompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<AbnfActionError> for AbnfError {
    fn from(error: AbnfActionError) -> Self {
        Self::Action(error)
    }
}

/// Options for [`abnf_compile`]: what to convert, and how to serialize
/// it.
///
/// The shared compiler's own `CompileOptions` carries only the
/// serialization half, because the conversion half is the front-end's;
/// this adds `start` and `tag`, exactly as `AbnfCompileOptions` does in
/// the Go port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbnfCompileOptions {
    /// Start rule name (default: the first production).
    pub start: Option<String>,
    /// Group tag stamped on every emitted alt (default `abnf`).
    pub tag: Option<String>,
    /// Emit a pure RECOGNITION grammar, with tree building dropped.
    /// `false` keeps the tree builtins, still as pure data.
    pub recognition: bool,
    /// Valid JSON rather than relaxed jsonic.
    pub strict: bool,
    /// Spaces per level (default 2).
    pub indent: Option<usize>,
}

impl Default for AbnfCompileOptions {
    fn default() -> Self {
        Self {
            start: None,
            tag: None,
            recognition: true,
            strict: false,
            indent: None,
        }
    }
}

/// Options for [`abnf`], the install path.
///
/// Mirrors the canonical `AbnfPluginOptions`: the conversion options,
/// plus the user actions to attach. Supplying actions forces closure
/// mode and requests marks, because a user action wraps the compiler's
/// own closures.
#[derive(Clone, Default)]
pub struct AbnfOptions {
    /// How to convert the source.
    pub convert: AbnfConvertOptions,
    /// User semantic actions, by `@<rule>:<phase>` or
    /// `@<rule>:o|c:<mark>` reference.
    pub actions: Option<ActionsMap>,
}

impl AbnfOptions {
    /// Conversion options alone.
    pub fn new(convert: AbnfConvertOptions) -> Self {
        Self {
            convert,
            actions: None,
        }
    }

    /// The same options carrying user actions.
    pub fn with_actions(mut self, actions: ActionsMap) -> Self {
        self.actions = Some(actions);
        self
    }
}

impl fmt::Debug for AbnfOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AbnfOptions")
            .field("convert", &self.convert)
            .field(
                "actions",
                &self.actions.as_ref().map(|actions| actions.len()),
            )
            .finish()
    }
}

/// Emit a spec from an already-parsed ABNF grammar.
///
/// Wraps the shared emitter to keep this crate's historical `abnf` tag
/// default, which consumers use to group and inspect the emitted alts.
/// An explicit tag still wins.
pub fn emit_grammar_spec(
    grammar: &Grammar,
    opts: Option<&AbnfConvertOptions>,
) -> Result<GrammarSpec, EmitError> {
    let mut options = opts.cloned().unwrap_or_default();
    if options.tag.is_none() {
        options.tag = Some(TAG.to_string());
    }
    tabnas_bnf::emit_grammar_spec(grammar, &options)
}

/// Convert ABNF source into a tabnas grammar spec, without installing
/// it. The Rust spelling of `tn.abnf.toSpec(src)`.
pub fn abnf_convert(
    src: &str,
    opts: Option<&AbnfConvertOptions>,
) -> Result<GrammarSpec, AbnfError> {
    let grammar = parse_abnf(src)?;
    Ok(emit_grammar_spec(&grammar, opts)?)
}

/// Convert ABNF source and install the resulting grammar on `parser`.
///
/// The Rust spelling of the callable `tn.abnf(src, opts)` the canonical
/// plugin decorates an instance with. With actions supplied, conversion
/// runs in closure mode with marks and the actions are attached before
/// the grammar is installed.
pub fn abnf(
    parser: &mut Tabnas,
    src: &str,
    opts: Option<&AbnfOptions>,
) -> Result<GrammarSpec, AbnfError> {
    let mut convert = opts.map(|opts| opts.convert.clone()).unwrap_or_default();
    let actions = opts.and_then(|opts| opts.actions.clone());
    if actions.is_some() {
        // User actions wrap the compiler's closures, so convert in
        // closure mode (never builtins) and request marks.
        convert.builtins = false;
        convert.marks = true;
    }
    let mut spec = abnf_convert(src, Some(&convert))?;
    if let Some(actions) = actions {
        attach_actions(&mut spec, actions)?;
    }
    spec.install(parser)
        .map_err(|error| AbnfError::Install(error.to_string()))?;
    Ok(spec)
}

/// The plugin descriptor, for [`Tabnas::use_plugin`].
///
/// A grammar this crate installs is whatever ABNF the caller hands over,
/// so the plugin has nothing of its own to install and records itself on
/// the instance instead, exactly as the Go `Plugin` does. Pass
/// `{"src": "<abnf text>"}` in the option bag to have it convert and
/// install that source; otherwise call [`abnf`] directly, which is the
/// typed way in.
pub fn plugin() -> Plugin {
    Plugin::new("Abnf", |parser, options| {
        let source = match options {
            Value::Object(entries) => match entries.get("src") {
                Some(Value::String(src)) => Some(src.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(source) = source else {
            return Ok(());
        };
        abnf(parser, &source, None)
            .map(|_| ())
            .map_err(|error| PluginError(error.to_string()))
    })
}
