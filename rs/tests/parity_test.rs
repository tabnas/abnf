// Cross-runtime conformance, driven by the shared `test/spec/*.tsv`
// fixtures at the repository root (see ../../test/AGENTS.md).
//
// The fixture loader, the escape codec, the `ERROR:` contract and the
// row loop all come from tabnas_support, whose TypeScript half
// `ts/test/parity.test.js` and Go half `go/parity_test.go` run the SAME
// files, so the three implementations cannot drift without one of them
// going red, and neither can the loaders.
//
// What is left here is only what is specific to abnf: four fixtures,
// each asserting a different thing about the same `grammar` column.

mod common;

use tabnas_support::{Runner, Value};

use common::{flatten, install, spec_dir, to_failure};

/// The grammar parses the input into the expected AST.
#[test]
fn spec_abnf_ast() {
    Runner::new_with_row(|input, row| {
        let grammar = row.unesc_named("grammar");
        let spec = tabnas_abnf::abnf_convert(&grammar, None).map_err(to_failure)?;
        let parser = install(&spec).map_err(to_failure)?;
        parser
            .parse(input)
            .map(|value| flatten(&value))
            .map_err(to_failure)
    })
    .input("input")
    .expected("expected")
    .file(spec_dir().join("alignment-abnf-ast.tsv"));
}

/// The grammar declares the expected fixed tokens.
#[test]
fn spec_abnf_tokens() {
    Runner::new(|grammar| {
        let spec = tabnas_abnf::abnf_convert(grammar, None).map_err(to_failure)?;
        let fixed = spec
            .options
            .get("fixed")
            .and_then(|fixed| fixed.get("token"))
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let kept: serde_json::Map<String, serde_json::Value> = fixed
            .into_iter()
            .filter(|(_, source)| !source.is_null())
            .collect();
        Ok(Value::from(serde_json::Value::Object(kept)))
    })
    .input("grammar")
    .expected("expected")
    .file(spec_dir().join("alignment-abnf-tokens.tsv"));
}

/// The grammar declares the expected rules, by name.
#[test]
fn spec_abnf_rules() {
    Runner::new(|grammar| {
        let spec = tabnas_abnf::abnf_convert(grammar, None).map_err(to_failure)?;
        let mut names: Vec<String> = spec.rule.keys().cloned().collect();
        names.sort();
        Ok(Value::from(serde_json::json!(names)))
    })
    .input("grammar")
    .expected("expected")
    .file(spec_dir().join("alignment-abnf-rules.tsv"));
}

/// The grammar is rejected, with exactly this message.
#[test]
fn spec_abnf_errors() {
    Runner::new(|grammar| {
        tabnas_abnf::abnf_convert(grammar, None)
            .map(|_| Value::Null)
            .map_err(to_failure)
    })
    .input("grammar")
    .expected("expected")
    // abnf's ERROR: cells hold the whole MESSAGE, compared EXACTLY, not
    // a code and not a substring. These rejections are the converter's
    // own diagnostics, several of them paragraphs that name the
    // offending rule and say what to write instead, and the wording is
    // the thing under test: a diagnostic that stops explaining itself is
    // the regression worth catching.
    .match_error(|failure, want, _row| failure.message == want)
    .file(spec_dir().join("alignment-abnf-errors.tsv"));
}
