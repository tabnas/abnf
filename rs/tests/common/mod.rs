// Shared test helpers. Cargo compiles this module into EVERY integration
// test binary, so an item only one binary uses is dead code in the
// others; the allow keeps that from being a warning rather than hiding
// anything real.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tabnas::{Options, RewindOptions, Tabnas, Value};
use tabnas_abnf::{abnf_convert, AbnfConvertOptions, AbnfError, GrammarSpec};
use tabnas_support::Failure;

/// The repository's shared fixture directory.
pub fn spec_dir() -> PathBuf {
    tabnas_support::find_spec_dir(Some(Path::new(env!("CARGO_MANIFEST_DIR"))))
        .expect("a test/spec directory above rs/")
}

/// The repository root, one level above this crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rs/ has a parent")
        .to_path_buf()
}

/// Convert ABNF source with the crate's defaults.
pub fn convert(src: &str) -> Result<GrammarSpec, AbnfError> {
    abnf_convert(src, None)
}

/// An engine carrying only the grammar `src` compiles to.
///
/// The rewind history is widened well past the default: a compiled ABNF
/// grammar backs up over whole alternatives, and the shared fixtures
/// include inputs that need more than the 64 tokens an engine retains by
/// default.
pub fn engine_for(src: &str) -> Result<Tabnas, AbnfError> {
    let spec = convert(src)?;
    install(&spec)
}

/// An engine carrying an already-converted spec.
pub fn install(spec: &GrammarSpec) -> Result<Tabnas, AbnfError> {
    let options = Options {
        rewind: RewindOptions {
            history: Some(4096),
        },
        ..Options::default()
    };
    let mut parser = Tabnas::with_options(options);
    spec.install(&mut parser)
        .map_err(|error| AbnfError::Install(error.to_string()))?;
    Ok(parser)
}

/// Compile `src`, install it, and parse `input`.
pub fn parse_with(src: &str, input: &str) -> Result<Value, String> {
    let parser = engine_for(src).map_err(|error| error.to_string())?;
    parser.parse(input).map_err(|error| error.to_string())
}

/// Convert an engine value to a fixture value, through JSON, the way the
/// Go runner's `jsonFlatten` does.
pub fn flatten(value: &Value) -> tabnas_support::Value {
    tabnas_support::Value::from(value.to_json())
}

/// The runner's failure shape for anything this crate refuses.
///
/// This crate declares no error codes of its own and the shared fixtures
/// pin the rendered MESSAGE, so the code channel stays empty and the
/// message is what a row compares.
pub fn to_failure(error: impl ToString) -> Failure {
    Failure::message(error.to_string())
}

/// The `{rule, src, kids}` tree as JSON, for a structural comparison.
pub fn tree(value: &Value) -> serde_json::Value {
    value.to_json()
}

/// Convert with explicit options.
pub fn convert_with(src: &str, opts: &AbnfConvertOptions) -> Result<GrammarSpec, AbnfError> {
    abnf_convert(src, Some(opts))
}
