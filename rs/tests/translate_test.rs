// The translation parts: what the manifest says and what the crate
// embeds are the same files. The tree a host translates is the grammar's
// rules in source order, the schema grammar-rules, streamed by the part
// from the rule passes of `make()`; the compiled grammar spec is the route
// of `translate.spec`, with its own render.
//
// A packaged crate holds nothing outside `rs/`, so the crate embeds its
// own copies, `rs/translate/manifest.json` of `tabnas.plugin.json` and
// `rs/translate/*.alc` of the part and the renders the manifest names, as
// `manifest_text()`, `transduce_text()`, `render_text()` and
// `render_spec_text()`. The copies are the only texts a host sees, so they
// must be the files. Change the file at the root and run `npm run embed`
// in `ts/`, which copies it into `rs/translate/`; this fails until both
// are the same.

mod common;

use std::fs;

use serde_json::Value;
use tabnas_abnf::{abnf_compile, AbnfCompileOptions};

fn translate() -> Value {
    let manifest: Value =
        serde_json::from_str(tabnas_abnf::manifest_text()).expect("the manifest is JSON");
    manifest
        .get("translate")
        .cloned()
        .expect("the manifest carries a translate object")
}

#[test]
fn the_manifest_the_crate_embeds_is_the_repositorys() {
    let on_disk = fs::read_to_string(common::repo_root().join("tabnas.plugin.json"))
        .expect("the repository has its manifest");
    assert_eq!(
        on_disk,
        tabnas_abnf::manifest_text(),
        "rs/translate/manifest.json is not tabnas.plugin.json: run npm run embed in ts"
    );
}

#[test]
fn the_parts_the_manifest_names_are_the_ones_the_crate_embeds() {
    let translate = translate();
    for (path, embedded) in [
        (&translate["render"], tabnas_abnf::render_text()),
        (&translate["transduce"], tabnas_abnf::transduce_text()),
        (
            &translate["spec"]["render"],
            tabnas_abnf::render_spec_text(),
        ),
    ] {
        let path = path.as_str().expect("the manifest names a file");
        let on_disk = fs::read_to_string(common::repo_root().join(path))
            .unwrap_or_else(|e| panic!("the manifest names {path}, which cannot be read: {e}"));
        assert_eq!(
            on_disk, embedded,
            "the manifest names {path}, and the crate embeds another text: run npm run embed in ts"
        );
    }
}

/// An embed takes a plain tree into a format's own schema. A grammar spec
/// is no plain tree's shape, so the manifest names no embed and the crate
/// carries none; a manifest that named one would be held to its file
/// here, as the render is above.
#[test]
fn the_embed_the_manifest_names_is_the_one_the_crate_embeds() {
    let translate = translate();
    let parts = tabnas_abnf::translate().expect("abnf carries translation parts");
    let Some(path) = translate.get("embed").and_then(Value::as_str) else {
        assert_eq!(
            parts.embed, None,
            "the manifest names no embed, and the crate carries one"
        );
        return;
    };
    let on_disk = fs::read_to_string(common::repo_root().join(path))
        .unwrap_or_else(|e| panic!("translate.embed names {path}, which cannot be read: {e}"));
    let embed = parts
        .embed
        .unwrap_or_else(|| panic!("translate.embed names {path}, and the crate carries no embed"));
    assert_eq!(embed.entry, "abnf-embed");
    assert_eq!(
        embed.source,
        Some(on_disk.as_str()),
        "translate.embed names {path}, and the crate embeds another text: run npm run embed in ts"
    );
}

#[test]
fn the_structural_interface_names_the_entries() {
    let parts = tabnas_abnf::translate().expect("abnf carries translation parts");
    assert_eq!(parts.manifest, tabnas_abnf::manifest_text());
    assert_eq!(parts.lift, None);
    let render = parts.render.expect("abnf carries a render");
    assert_eq!(render.entry, "abnf-render");
    assert_eq!(render.source, Some(tabnas_abnf::render_text()));
    let transduce = parts.transduce.expect("abnf carries a part");
    assert_eq!(transduce.entry, "abnf-transduce");
    assert_eq!(transduce.source, Some(tabnas_abnf::transduce_text()));
    let spec = parts
        .spec
        .expect("abnf carries the grammar-spec route's render");
    assert_eq!(spec.entry, "abnf-render-spec");
    assert_eq!(spec.source, Some(tabnas_abnf::render_spec_text()));
}

/// An ABNF document is read as a tree and written from one, the grammar's
/// rules, which the manifest names as its schema, streamed by the part,
/// so nothing is read whole and there is no lift; the compiled spec's
/// route is `spec`, of the schema grammar-spec.
#[test]
fn abnf_reads_and_writes_its_grammar_rules_through_its_part() {
    let manifest: Value =
        serde_json::from_str(tabnas_abnf::manifest_text()).expect("the manifest is JSON");
    assert_eq!(manifest["languageId"], "abnf");
    let translate = translate();
    assert_eq!(translate["reads"], "tree");
    assert_eq!(translate["writes"], "tree");
    assert_eq!(translate["root"], "object");
    assert_eq!(translate["schema"], "grammar-rules");
    assert_eq!(translate["transduce"], "alchemy/transduce.alc");
    assert_eq!(translate.get("lift"), None);
    assert_eq!(translate.get("whole"), None);
    assert_eq!(translate["spec"]["schema"], "grammar-spec");
    assert_eq!(translate["spec"]["root"], "object");
}

/// The host prints the loss lines verbatim, so each is a sentence.
#[test]
fn the_loss_is_a_list_of_sentences() {
    let translate = translate();
    for loss in [&translate["loss"], &translate["spec"]["loss"]] {
        let loss = loss.as_array().expect("a loss is a list");
        assert!(!loss.is_empty());
        for line in loss {
            let line = line.as_str().expect("each loss line is a string");
            assert!(
                line.starts_with(char::is_uppercase) && line.ends_with('.'),
                "{line:?} is not a sentence"
            );
        }
    }
}

/// A host links the part and the renders with its own program and other
/// formats' parts, so every definition is named for ABNF, each defines its
/// entry point, and the two renders, which a host may link together,
/// share no name.
#[test]
fn the_part_and_the_renders_are_libraries_named_for_abnf() {
    let defs = |text: &'static str| -> Vec<&'static str> {
        text.lines()
            .filter_map(|line| line.strip_prefix("def "))
            .filter_map(|rest| rest.split_whitespace().next())
            .collect()
    };
    for (text, entry) in [
        (tabnas_abnf::transduce_text(), "abnf-transduce"),
        (tabnas_abnf::render_text(), "abnf-render"),
        (tabnas_abnf::render_spec_text(), "abnf-render-spec"),
    ] {
        let names = defs(text);
        assert!(names.contains(&entry), "{names:?}");
        for name in &names {
            assert!(name.starts_with("abnf-"), "{name} is not named for abnf");
        }
    }
    let render = defs(tabnas_abnf::render_text());
    for name in defs(tabnas_abnf::render_spec_text()) {
        assert!(!render.contains(&name), "{name} is defined in both renders");
    }
}

/// What a host that takes the grammar-spec route reads an ABNF document
/// as: the pure-data grammar spec `abnf_compile` writes with `recognition`
/// false and `strict` true, valid JSON whose root is an object holding the
/// rules and the options.
#[test]
fn a_host_reads_a_document_as_the_strict_pure_data_grammar_spec() {
    let text = abnf_compile(
        "greet = \"hi\" / \"hello\"",
        &AbnfCompileOptions {
            recognition: false,
            strict: true,
            ..AbnfCompileOptions::default()
        },
    )
    .expect("the grammar compiles");
    let tree: Value = serde_json::from_str(&text).expect("the grammar spec is JSON");
    let rules: Vec<&String> = tree["rule"]
        .as_object()
        .expect("the spec holds its rules")
        .keys()
        .collect();
    assert_eq!(rules, ["greet", "__start__"]);
    assert_eq!(tree["options"]["rule"]["start"], "__start__");
    assert_eq!(tree["meta"]["provenance"]["__start__"], "greet");
}
