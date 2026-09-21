// Every divergence `../DIVERGENCE.md` records, pinned.
//
// Each case is asserted in BOTH directions, and both directions are
// MEASURED: what this port does, and what the canonical runtime does
// that differs. The canonical half is not a comment and not a constant
// copied off a screen -- it is the canonical implementation, run. A
// divergence that CLOSES from either side fails here as loudly as one
// that opens, so the recorded table cannot quietly go stale, and the
// file it records cannot describe a behaviour that neither runtime has
// any more.
//
// BOTH DIRECTIONS MEANS TWO: TypeScript and Rust. Nothing in this file
// runs Go, and nothing in it may be read as pinning Go. The Go column
// of every table in `../DIVERGENCE.md` is a hand measurement, dated
// there, which `go/`'s own suite does not pin either; a claim about Go
// that has to hold belongs in a test under `go/`, not in a comment
// here.
//
// There is no executable register under `test/spec` for these: most are
// invisible to the grammar-to-output comparison every file there makes,
// one is about the shape of the API rather than about any value, and
// one is about the tree a compiled grammar builds rather than about the
// grammar. `../DIVERGENCE.md` says so, and this file is what stands in
// for the register.
//
// THE CANONICAL HALF NEEDS THE CANONICAL BUILT. It runs `node` over
// `../ts/dist/abnf.js`, which `make build-ts` produces and which is not
// committed. Absent, this file FAILS and names the command, because a
// half-measured divergence table is the defect this file exists to
// prevent. `ABNF_CANONICAL=off` turns the canonical half off for a
// checkout that genuinely cannot build it; `ci/rust/run.sh` sets it
// when the canonical is missing and says so loudly in the gate output,
// which is the one place the absence is visible.

mod common;

use std::process::Command;
use std::sync::OnceLock;

use serde_json::{json, Value as JsonValue};
use tabnas_abnf::{
    abnf_compile, abnf_convert, parse_abnf, AbnfCompileOptions, AbnfConvertOptions, Kind,
};

use common::repo_root;

// ---- the canonical runtime, run ---------------------------------------

/// Every question this file asks of the canonical runtime, answered in
/// one process.
///
/// One process rather than one per case: the answers are pure functions
/// of the canonical build, nothing here mutates it, and a node start-up
/// per assertion would cost more than the whole Rust suite.
///
/// Strings come back as UTF-16 CODE UNITS rather than as text, because
/// that is the unit three of these entries are about. A JSON string
/// cannot carry a lone surrogate through `JSON.parse` on the Rust side
/// without being repaired into U+FFFD, which is precisely the value
/// under test, so the comparison would pass for the wrong reason.
const CANONICAL_SCRIPT: &str = r##"
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const ts = process.env.ABNF_TS_DIR
const req = createRequire(join(ts, 'package.json'))
const { Tabnas } = req('@tabnas/parser')
const { abnfConvert, abnfCompile, parseAbnf, abnf } = req(join(ts, 'dist', 'abnf.js'))

const units = (text) =>
  Array.from({ length: text.length }, (_, i) => text.charCodeAt(i))
const fixed = (src) => abnfConvert(src, { builtins: true }).options.fixed.token
const literal = (src) => units(parseAbnf(src).productions[0].alts[0][0].literal)
const caught = (run) => {
  try { return { threw: false, value: run() } }
  catch (error) {
    return { threw: true, name: error.constructor.name, message: String(error.message) }
  }
}
const nest = (depth) =>
  'top = ' + '( '.repeat(depth) + '"x"' + ' )'.repeat(depth)
const span = (src, name) => {
  const production = parseAbnf(src).productions.find((p) => p.name === name)
  return [production.sp.s, production.sp.e, production.sp.r, production.sp.c]
}
const probe = (builtins, input) => {
  const engine = new Tabnas({ rewind: { history: 4096 } })
  engine.grammar(abnfConvert(PROBE, { builtins }))
  return engine.parse(input)
}
const PROBE = 'g = [ user "@" ] host\nuser = 1*ALPHA\nhost = 1*ALPHA'
// The same dispatch inside a grammar somebody published: RFC 3986's
// `authority = [ userinfo "@" ] host [ ":" port ]`, whose optional
// prefix draws on the same characters as the `host` after it.
const RFC = readFileSync(join(ts, 'test', 'grammar', 'rfc3986-uri.abnf'), 'utf8')
const authority = (input) => {
  const engine = new Tabnas({ rewind: { history: 4096 } })
  engine.grammar(abnfConvert(RFC, { start: 'authority' }))
  return engine.parse(input)
}

const decorated = new Tabnas()
decorated.use(abnf)
const decoration = typeof decorated.abnf
const toSpec = typeof decorated.abnf?.toSpec
decorated.abnf('greet = "hi"')

const untouched = new Tabnas()
const before = Object.keys(untouched.rule()).length
abnfConvert('greet = "hi"')

console.log(JSON.stringify({
  one_lone: units(fixed('g = %xD800')['#T']),
  one_d7ff: units(fixed('g = %xD7FF')['#T']),
  one_e000: units(fixed('g = %xE000')['#T']),
  one_astral: units(fixed('g = %x1F600')['#T']),
  one_pair: literal('g = %xD800.DC00'),
  one_pair_emoji: literal('g = %xD83D.DE00'),
  one_pair_mixed: literal('g = %x41.D800.DC00.42'),
  one_pair_reversed: literal('g = %xDC00.D800'),
  one_lone_then_ascii: literal('g = %xD800.0041'),
  one_ascii_then_lone: literal('g = %x0041.D800'),
  one_pair_across_elements: Object.values(fixed('g = %xD800 %xDC00')).map(units),
  one_surrogate_range: caught(() => { abnfConvert('g = %xD800-DFFF'); return true }),
  one_spanning_range: caught(() => { abnfConvert('g = %xD7FF-E000'); return true }),

  two_accented: span('a = "éé"\nb = "x"\n', 'b'),
  two_plain: span('a = "xx"\nb = "x"\n', 'b'),

  three_127: caught(() => { abnfConvert(nest(127)); return true }),
  three_128: caught(() => { abnfConvert(nest(128)); return true }),
  three_200: caught(() => { abnfConvert(nest(200)); return true }),
  three_5000: caught(() => { abnfConvert(nest(5000)); return true }),

  four_hole: caught(() => { parseAbnf('bad = ( "a"'); return true }),
  four_decoration: decoration,
  four_to_spec: toSpec,
  four_installed: Object.keys(decorated.rule()).includes('greet'),
  four_untouched: Object.keys(untouched.rule()).length === before,

  five_closures: probe(false, 'ab@cd'),
  five_builtins: probe(true, 'ab@cd'),
  five_closures_bare: probe(false, 'abc'),
  five_builtins_bare: probe(true, 'abc'),
  five_authority: authority('user@example.com'),
  five_authority_bare: authority('example.com'),
  five_compiled: abnfCompile(PROBE, {}),

  six_reversed: caught(() => { abnfConvert('g = %x5A-41'); return true }),
  six_ascending: caught(() => { abnfConvert('g = %x41-5A'); return true }),

  seven_120000: caught(() => { abnfConvert('g = "' + 'a'.repeat(120000) + '"'); return true }),
  seven_150000: caught(() => { abnfConvert('g = "' + 'a'.repeat(150000) + '"'); return true }),
}))
"##;

/// What the canonical runtime answered, or `None` when the canonical
/// half is switched off.
fn canonical() -> Option<&'static JsonValue> {
    static ANSWERS: OnceLock<Option<JsonValue>> = OnceLock::new();
    ANSWERS
        .get_or_init(|| {
            if matches!(std::env::var("ABNF_CANONICAL").as_deref(), Ok("off")) {
                return None;
            }
            Some(run_canonical())
        })
        .as_ref()
}

/// Run the canonical implementation and read its answers.
///
/// Every failure here is fatal rather than a skip. The claim this file
/// makes is that both halves of every row in `../DIVERGENCE.md` are
/// measured; a canonical half that quietly did not run turns that claim
/// into the stale prose the file exists to replace.
fn run_canonical() -> JsonValue {
    let ts = repo_root().join("ts");
    let entry = ts.join("dist").join("abnf.js");
    assert!(
        entry.is_file(),
        "the canonical TypeScript is not built at {}\n  \
         Build it with:  (cd ts && npm install && npm run build)   (or: make build-ts)\n  \
         This file measures the canonical half of every DIVERGENCE.md entry against it \
         and FAILS rather than skipping.\n  \
         Set ABNF_CANONICAL=off only where it genuinely cannot be built; \
         ci/rust/run.sh does that and says so.",
        entry.display()
    );
    let output = Command::new("node")
        .args(["--input-type=module", "-e", CANONICAL_SCRIPT])
        .env("ABNF_TS_DIR", &ts)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "could not run node for the canonical half: {error}\n  \
                 node >=24 is the canonical runtime; see DIVERGENCE.md.\n  \
                 Set ABNF_CANONICAL=off only where it genuinely cannot be run."
            )
        });
    assert!(
        output.status.success(),
        "the canonical runtime failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "the canonical runtime answered something that is not JSON: {error}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// The canonical answer to one question, or nothing when the canonical
/// half is off.
///
/// The `else` arm prints rather than failing, and that print is the only
/// trace an unmeasured canonical half leaves in a green run. The gate
/// script is where the absence is actually announced.
macro_rules! canonical {
    ($key:expr) => {
        match canonical() {
            Some(answers) => Some(answers[$key].clone()),
            None => {
                println!(
                    "ABNF_CANONICAL=off: the canonical half of {} is NOT measured",
                    $key
                );
                None
            }
        }
    };
}

/// The fixed-token map a grammar emits.
fn fixed_tokens(src: &str) -> JsonValue {
    let options = AbnfConvertOptions {
        builtins: true,
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(src, Some(&options)).expect("compiles");
    spec.options
        .get("fixed")
        .and_then(|fixed| fixed.get("token"))
        .cloned()
        .unwrap_or(JsonValue::Null)
}

/// A Rust string as UTF-16 code units, which is how the canonical half
/// reports one, so the two are compared in the same unit.
fn units(text: &str) -> JsonValue {
    JsonValue::from(text.encode_utf16().map(u32::from).collect::<Vec<u32>>())
}

/// The literal a one-element production carries.
fn literal(src: &str) -> String {
    let grammar = parse_abnf(src).expect("parses");
    let element = &grammar.productions[0].alts[0][0];
    match &element.kind {
        Kind::Term { literal, .. } => literal.clone(),
        other => panic!("{src}: expected a term, got {other:?}"),
    }
}

/// DIVERGENCE 1. `%xD800` names one half of a UTF-16 surrogate pair.
/// TypeScript answers a lone surrogate; no Rust `String` can hold one,
/// so the replacement character stands in, as it does in Go.
///
/// A surrogate PAIR is not this entry. `%xD800.DC00` is the UTF-16
/// encoding of U+10000, which every runtime can represent, so the two
/// runtimes agree there and the test says so.
#[test]
fn a_lone_surrogate_becomes_the_replacement_character() {
    let tokens = fixed_tokens("g = %xD800");
    assert_eq!(
        tokens["#T"], "\u{FFFD}",
        "the canonical runtime emits U+D800 here; see DIVERGENCE.md entry 1"
    );
    if let Some(canon) = canonical!("one_lone") {
        assert_eq!(
            canon,
            json!([0xD800]),
            "the canonical runtime no longer answers a lone surrogate, so \
             DIVERGENCE.md entry 1 has closed and must be deleted"
        );
    }

    // The other direction: a code point that IS a scalar value still
    // arrives unchanged, so this is about surrogates and nothing wider.
    assert_eq!(fixed_tokens("g = %xD7FF")["#T"], "\u{D7FF}");
    assert_eq!(fixed_tokens("g = %xE000")["#T"], "\u{E000}");
    assert_eq!(fixed_tokens("g = %x1F600")["#T"], "\u{1F600}");
    if let Some(canon) = canonical!("one_d7ff") {
        assert_eq!(canon, json!([0xD7FF]));
    }
    if let Some(canon) = canonical!("one_e000") {
        assert_eq!(canon, json!([0xE000]));
    }
    if let Some(canon) = canonical!("one_astral") {
        assert_eq!(canon, json!([0xD83D, 0xDE00]), "U+1F600 as its own pair");
    }

    // A pair written as one dotted concatenation is a CHARACTER, in
    // both runtimes: the canonical string holds the two halves next to
    // each other, which is U+10000, and this port decodes the sequence
    // as a whole rather than one part at a time.
    assert_eq!(literal("g = %xD800.DC00"), "\u{10000}");
    assert_eq!(literal("g = %xD83D.DE00"), "\u{1F600}");
    if let Some(canon) = canonical!("one_pair") {
        assert_eq!(canon, units("\u{10000}"), "the pair is U+10000 there too");
    }
    if let Some(canon) = canonical!("one_pair_emoji") {
        assert_eq!(canon, units("\u{1F600}"));
    }

    // And a pair still pairs INSIDE a longer concatenation, which is the
    // third row of the table in `../DIVERGENCE.md` under "An ADJACENT
    // pair is not this entry".
    assert_eq!(literal("g = %x41.D800.DC00.42"), "A\u{10000}B");
    if let Some(canon) = canonical!("one_pair_mixed") {
        assert_eq!(canon, units("A\u{10000}B"));
    }

    // What the join leaves unpaired is this entry again, one U+FFFD per
    // stranded half, which is what a lossy UTF-16 decode gives.
    assert_eq!(literal("g = %xDC00.D800"), "\u{FFFD}\u{FFFD}");
    assert_eq!(literal("g = %xD800.0041"), "\u{FFFD}A");
    assert_eq!(literal("g = %x0041.D800"), "A\u{FFFD}");
    if let Some(canon) = canonical!("one_pair_reversed") {
        assert_eq!(
            canon,
            json!([0xDC00, 0xD800]),
            "a low half first pairs with nothing"
        );
    }
    if let Some(canon) = canonical!("one_lone_then_ascii") {
        assert_eq!(canon, json!([0xD800, 0x41]));
    }
    if let Some(canon) = canonical!("one_ascii_then_lone") {
        assert_eq!(canon, json!([0x41, 0xD800]));
    }

    // A pair split ACROSS two elements is two terms, never one
    // character: the canonical runtime builds one string per numeric
    // value and never joins them, so both halves stay lone surrogates
    // there and both become U+FFFD here. Here they then collapse into
    // ONE token, because the two halves are now the same string, which
    // is the same entry 1 read one step further downstream.
    let split = fixed_tokens("g = %xD800 %xDC00");
    assert_eq!(split["#T"], "\u{FFFD}");
    assert_eq!(
        split.as_object().map(serde_json::Map::len),
        Some(1),
        "the canonical runtime emits two distinct tokens here"
    );
    if let Some(canon) = canonical!("one_pair_across_elements") {
        assert_eq!(
            canon,
            json!([[0xD800], [0xDC00]]),
            "the canonical runtime joined a pair across elements, which would \
             make this a defect of this port rather than DIVERGENCE.md entry 1"
        );
    }

    // And the same fact in a character class: a range with no scalar
    // value anywhere in it is a class the `regex` crate cannot build,
    // where the canonical runtime compiles it over code units.
    let error =
        abnf_convert("g = %xD800-DFFF", None).expect_err("a surrogate-only range is refused");
    assert!(
        error.to_string().contains("is not a Unicode scalar value"),
        "got {error}; see DIVERGENCE.md entry 1. A refusal for any OTHER \
         reason would pass a looser assertion here and pin nothing"
    );
    if let Some(canon) = canonical!("one_surrogate_range") {
        assert_eq!(
            canon["threw"],
            json!(false),
            "the canonical runtime now refuses a surrogate-only range too, so \
             that row of DIVERGENCE.md entry 1 has closed"
        );
    }

    // A range that merely SPANS the surrogate block still compiles, in
    // both runtimes, so this is about a class with nothing in it and not
    // about ranges near the block.
    abnf_convert("g = %xD7FF-E000", None).expect("a range spanning the block compiles");
    if let Some(canon) = canonical!("one_spanning_range") {
        assert_eq!(canon["threw"], json!(false));
    }
}

/// DIVERGENCE 2. A span's offsets count BYTES, where the canonical
/// runtime counts UTF-16 code units.
#[test]
fn span_offsets_count_bytes() {
    // `é` is two bytes and one UTF-16 code unit, so a rule following a
    // literal holding two of them starts at 11 here and at 9 there.
    let src = "a = \"\u{e9}\u{e9}\"\nb = \"x\"\n";
    let grammar = parse_abnf(src).expect("parses");
    let production = grammar
        .productions
        .iter()
        .find(|production| "b" == production.name)
        .expect("the second production");
    let span = production.sp.expect("a span");
    assert_eq!(
        (span.s, span.e),
        (11, 12),
        "the canonical runtime reports 9..10 here; see DIVERGENCE.md entry 2"
    );
    assert_eq!((span.r, span.c), (Some(2), Some(1)));
    if let Some(canon) = canonical!("two_accented") {
        assert_eq!(
            canon,
            json!([9, 10, 2, 1]),
            "the canonical runtime no longer counts UTF-16 code units, so \
             DIVERGENCE.md entry 2 has closed"
        );
    }

    // What a consumer actually wants holds in every runtime: slicing the
    // source with the span gives the same TEXT.
    assert_eq!(&src[span.s..span.e], "b");

    // And with nothing non-ASCII in front, the two units agree, so this
    // is about the encoding and not about an off-by-one.
    let plain = "a = \"xx\"\nb = \"x\"\n";
    let grammar = parse_abnf(plain).expect("parses");
    let span = grammar
        .productions
        .iter()
        .find(|production| "b" == production.name)
        .and_then(|production| production.sp)
        .expect("a span");
    assert_eq!((span.s, span.e), (9, 10));
    if let Some(canon) = canonical!("two_plain") {
        assert_eq!(canon, json!([9, 10, 2, 1]));
    }
}

/// DIVERGENCE 3. Nested groups are refused sooner than in the canonical
/// runtime, at two caps: the shared compiler's 128 levels of element
/// nesting, and this crate's own limit on the rule stack.
#[test]
fn nested_groups_are_refused_sooner() {
    let nest = |depth: usize| -> String {
        format!("top = {}\"x\"{}", "( ".repeat(depth), " )".repeat(depth))
    };

    // 127 compiles, so the cap is a cap and not a ceiling anyone meets
    // by accident.
    abnf_convert(&nest(127), None).expect("127 nested groups compile");
    if let Some(canon) = canonical!("three_127") {
        assert_eq!(canon["threw"], json!(false), "127 compiles there too");
    }

    // 128 is refused by the shared compiler, which the canonical
    // runtime accepts.
    let error = abnf_convert(&nest(128), None).expect_err("128 is refused");
    assert!(
        error
            .to_string()
            .contains("nests elements more than 128 deep"),
        "got {error}; see DIVERGENCE.md entry 3"
    );
    if let Some(canon) = canonical!("three_128") {
        assert_eq!(
            canon["threw"],
            json!(false),
            "the canonical runtime now refuses 128 too, so the first row of \
             DIVERGENCE.md entry 3 has closed"
        );
    }
    if let Some(canon) = canonical!("three_200") {
        assert_eq!(canon["threw"], json!(false), "200 is accepted there");
    }

    // The front-end's own cap is further out, and it is what a source
    // deep enough to threaten the stack meets. The canonical runtime
    // raises a catchable range error somewhere past this; Go accepts it.
    let error = parse_abnf(&nest(5000)).expect_err("5000 is refused by the front-end");
    assert!(
        error.to_string().contains("nests too deeply"),
        "got {error}; see DIVERGENCE.md entry 3"
    );
    if let Some(canon) = canonical!("three_5000") {
        assert_eq!(canon["threw"], json!(true), "5000 is refused there as well");
        assert_eq!(
            canon["name"],
            json!("RangeError"),
            "the canonical refusal is the stack running out, not a diagnostic; \
             if it is now a diagnostic, DIVERGENCE.md entry 3 has changed"
        );
        assert!(
            canon["message"]
                .as_str()
                .is_some_and(|message| message.contains("call stack")),
            "got {canon}"
        );
    }
}

/// DIVERGENCE 4. A failure is RETURNED, and there is no instance
/// decoration, so the install path is a free function.
#[test]
fn failures_are_returned_and_there_is_no_decoration() {
    // Returned, not raised, and with the same text the canonical
    // runtime throws.
    let error = parse_abnf("bad = ( \"a\"").expect_err("a malformed grammar is refused");
    assert!(error.to_string().starts_with("abnf: "));
    if let Some(canon) = canonical!("four_hole") {
        assert_eq!(
            canon["threw"],
            json!(true),
            "the canonical runtime now RETURNS this failure, so the first half \
             of DIVERGENCE.md entry 4 has closed"
        );
        assert_eq!(canon["name"], json!("AbnfParseError"));
        // The text is the contract; only the channel differs.
        assert_eq!(canon["message"], json!(error.to_string()));
    }

    // The install path takes the engine as an argument, and leaves it
    // carrying the grammar.
    let mut parser = tabnas::Tabnas::new();
    tabnas_abnf::abnf(&mut parser, "greet = \"hi\"", None).expect("installs");
    assert!(parser.rule_names().iter().any(|name| "greet" == name));
    if let Some(canon) = canonical!("four_decoration") {
        assert_eq!(
            canon,
            json!("function"),
            "the canonical runtime no longer decorates the instance, so the \
             second half of DIVERGENCE.md entry 4 has closed"
        );
    }
    if let Some(canon) = canonical!("four_to_spec") {
        assert_eq!(canon, json!("function"), "`tn.abnf.toSpec` is still there");
    }
    if let Some(canon) = canonical!("four_installed") {
        assert_eq!(canon, json!(true), "and calling it still installs");
    }

    // And the convert-only path leaves an engine alone, which is what
    // the canonical `tn.abnf.toSpec` does.
    let untouched = tabnas::Tabnas::new();
    let before = untouched.rule_names().len();
    abnf_convert("greet = \"hi\"", None).expect("converts");
    assert_eq!(untouched.rule_names().len(), before);
    if let Some(canon) = canonical!("four_untouched") {
        assert_eq!(canon, json!(true));
    }
}

/// DIVERGENCE 5. A probe and retry keeps the node it built, where the
/// canonical runtime answers an empty one.
///
/// The compiled grammar text is BYTE IDENTICAL in TypeScript and in
/// this port, which is what makes this the engine's answer and not this
/// crate's, and which this test measures rather than asserts in prose.
/// Go is not in that comparison: `abnf_compile` and `AbnfCompile`
/// answered different text for both of these grammars when they were
/// compared on 2026-09-21, so the Go column of entry 5 rests on the
/// hand measurement recorded in `../DIVERGENCE.md` and on nothing here.
///
/// It is asserted with `builtins` both off, where the retry hooks are
/// closures the shared compiler registers, and on, where they are the
/// engine's own `$` builtins, because a difference in only one of those
/// would say which side owns it.
#[test]
fn a_probe_and_retry_keeps_the_node_it_built() {
    const SRC: &str = "g = [ user \"@\" ] host\nuser = 1*ALPHA\nhost = 1*ALPHA";

    for builtins in [false, true] {
        let options = AbnfConvertOptions {
            builtins,
            ..AbnfConvertOptions::default()
        };
        let spec = abnf_convert(SRC, Some(&options)).expect("compiles");
        let parser = {
            let mut parser = tabnas::Tabnas::with_options(tabnas::Options {
                rewind: tabnas::RewindOptions {
                    history: Some(4096),
                },
                ..tabnas::Options::default()
            });
            spec.install(&mut parser).expect("installs");
            parser
        };

        let tree = parser.parse("ab@cd").expect("parses").to_json();
        assert_eq!(
            tree["src"], "ab@cd",
            "builtins={builtins}: the canonical runtime answers an empty node here; \
             see DIVERGENCE.md entry 5"
        );
        assert_eq!(tree["kids"].as_array().map(Vec::len), Some(2));
        assert_eq!(tree["kids"][0]["rule"], "user");
        assert_eq!(tree["kids"][1]["rule"], "host");

        // The optional absent, which takes the other branch of the same
        // dispatch.
        let tree = parser.parse("abc").expect("parses").to_json();
        assert_eq!(tree["src"], "abc");
        assert_eq!(tree["kids"][0]["rule"], "host");
    }

    let empty = json!({ "rule": "g", "src": "", "kids": [] });
    let empty_authority = json!({ "rule": "authority", "src": "", "kids": [] });
    for key in [
        "five_closures",
        "five_builtins",
        "five_closures_bare",
        "five_builtins_bare",
    ] {
        if let Some(canon) = canonical!(key) {
            assert_eq!(
                canon, empty,
                "{key}: the canonical runtime now keeps the node the retry built, \
                 so DIVERGENCE.md entry 5 has closed"
            );
        }
    }

    // The third row of that table: the same dispatch inside a grammar
    // somebody published. RFC 3986's `authority` has the same shape,
    // `[ userinfo "@" ] host [ ":" port ]`, and the same overlap, so the
    // entry is not an artefact of the two-line grammar above.
    let rfc = std::fs::read_to_string(
        repo_root()
            .join("ts")
            .join("test")
            .join("grammar")
            .join("rfc3986-uri.abnf"),
    )
    .expect("the rfc3986-uri.abnf fixture");
    let options = AbnfConvertOptions {
        start: Some("authority".to_string()),
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert(&rfc, Some(&options)).expect("RFC 3986 compiles");
    let parser = common::install(&spec).expect("installs");
    let tree = parser.parse("user@example.com").expect("parses").to_json();
    assert_eq!(
        tree["src"], "user@example.com",
        "the canonical runtime answers an empty node here; see DIVERGENCE.md entry 5"
    );
    let kids: Vec<String> = tree["kids"]
        .as_array()
        .map(|kids| {
            kids.iter()
                .filter_map(|kid| kid["rule"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        kids.iter().any(|rule| "userinfo" == rule) && kids.iter().any(|rule| "host" == rule),
        "expected userinfo and host under authority, got {kids:?}"
    );
    for key in ["five_authority", "five_authority_bare"] {
        if let Some(canon) = canonical!(key) {
            assert_eq!(
                canon, empty_authority,
                "{key}: the canonical runtime now keeps the node the retry built, \
                 so DIVERGENCE.md entry 5 has closed"
            );
        }
    }

    // The load-bearing half of this entry, measured rather than
    // asserted in prose: the grammar the two runtimes RUN is the same
    // one, byte for byte, so the different trees above are the engine
    // reading one grammar two ways and not two compilers emitting two
    // grammars. If this ever differs, entry 5 stops being an engine
    // difference and becomes a defect of this crate.
    //
    // Only the two-line grammar, because it is the cheap one: compiling
    // the whole of RFC 3986 with marks on emits about nine megabytes and
    // takes minutes in each runtime, which is a price every `cargo test`
    // would pay. That grammar's two compilers were compared by hand on
    // 2026-09-21 and agreed byte for byte; `../DIVERGENCE.md` says so,
    // and says it is a hand measurement.
    let compiled = abnf_compile(SRC, &AbnfCompileOptions::default()).expect("compiles");
    if let Some(canon) = canonical!("five_compiled") {
        assert_eq!(
            canon,
            JsonValue::String(compiled),
            "the compiled grammar text no longer matches the canonical runtime, \
             so the difference entry 5 records is no longer the engine's alone"
        );
    }

    // The other direction: a grammar whose optional prefix does NOT
    // overlap what follows needs no probe, and every runtime builds the
    // same tree for it. So this is about the retry and nothing wider.
    let parser = common::engine_for("g = [ num \"@\" ] host\nnum = 1*DIGIT\nhost = 1*ALPHA")
        .expect("compiles");
    let tree = parser.parse("12@cd").expect("parses").to_json();
    assert_eq!(tree["src"], "12@cd");
    assert_eq!(tree["kids"].as_array().map(Vec::len), Some(2));
}

/// DIVERGENCE 6. A reversed numeric range is refused in the words of
/// whichever regular expression engine the runtime carries.
#[test]
fn a_reversed_numeric_range_is_refused_in_the_regex_engines_words() {
    let error = abnf_convert("g = %x5A-41", None).expect_err("a reversed range is refused");
    let message = error.to_string();
    assert!(
        message.contains("invalid character class range"),
        "got {message}; see DIVERGENCE.md entry 6"
    );
    if let Some(canon) = canonical!("six_reversed") {
        assert_eq!(
            canon["threw"],
            json!(true),
            "the canonical runtime now accepts a reversed range"
        );
        assert_eq!(canon["name"], json!("SyntaxError"));
        assert!(
            canon["message"]
                .as_str()
                .is_some_and(|text| text.contains("Range out of order in character class")),
            "got {canon}; that is no longer the canonical wording, so \
             DIVERGENCE.md entry 6 must be re-measured"
        );
        assert_ne!(
            canon["message"],
            json!(message),
            "both runtimes now say the same thing, so entry 6 has closed"
        );
    }

    // A range the right way round still compiles, so this is about the
    // order and not about ranges.
    abnf_convert("g = %x41-5A", None).expect("an ascending range compiles");
    if let Some(canon) = canonical!("six_ascending") {
        assert_eq!(canon["threw"], json!(false));
    }
}

/// DIVERGENCE 7. A case-insensitive literal becomes one regular
/// expression, and the `regex` crate caps a compiled pattern at ten
/// megabytes where the canonical runtime has no cap at all.
#[test]
fn a_very_long_literal_exceeds_the_regex_size_limit() {
    let literal = |count: usize| format!("g = \"{}\"", "a".repeat(count));

    // Well inside the budget, so the cap is a cap and not a ceiling any
    // author meets: the corpus's longest literal is shorter than this
    // by two orders of magnitude.
    abnf_convert(&literal(120_000), None).expect("120000 characters compile");
    if let Some(canon) = canonical!("seven_120000") {
        assert_eq!(canon["threw"], json!(false));
    }

    let error = abnf_convert(&literal(150_000), None).expect_err("150000 characters are refused");
    assert!(
        error.to_string().contains("exceeds size limit"),
        "got {error}; see DIVERGENCE.md entry 7"
    );
    if let Some(canon) = canonical!("seven_150000") {
        assert_eq!(
            canon["threw"],
            json!(false),
            "the canonical runtime now refuses this literal too, so \
             DIVERGENCE.md entry 7 has closed"
        );
    }
}
