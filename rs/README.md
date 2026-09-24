# tabnas-abnf (Rust)

An ABNF (RFC 5234) grammar compiler for the
[`tabnas`](https://github.com/tabnas/parser) parsing engine, crate
`tabnas_abnf`.

Where most tabnas grammar crates carry one fixed grammar, this one is a
META compiler: the grammar it installs is whatever ABNF text it is
handed at run time. Feed it the collected ABNF an RFC publishes and the
engine parses that language.

```text
ABNF text ──parse_abnf──▶ Grammar ──emit_grammar_spec──▶ GrammarSpec
```

The first arrow is what this crate adds: the RFC 5234 front-end, the
Appendix B.1 core rules, incremental alternatives, numeric values,
case-insensitive literals, and the value annotations a trailing comment
can carry. Everything downstream of that IR lives in
[`tabnas-bnf`](https://github.com/tabnas/bnf) and is shared with the
GBNF and EBNF front-ends: desugaring, left-recursion elimination, probe
dispatch, literal lifting, token allocation, and chain emission.

This is the Rust port of the canonical TypeScript implementation in
[`../ts`](../ts); the TypeScript version is authoritative and this crate
tracks it. The Go port in [`../go`](../go) has the same shape.

## Use

Compile a grammar and install it on an engine:

```rust
use tabnas::Tabnas;
use tabnas_abnf::abnf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Tabnas::new();
    abnf(&mut parser, "greet = \"hi\" / \"hello\"", None)?;

    let tree = parser.parse("hello")?;
    assert_eq!(tree.to_json()["rule"], "greet");
    Ok(())
}
```

A grammar builds a parse tree by default: one `{rule, src, kids}` node
per rule the author wrote.

```rust
use tabnas::Tabnas;
use tabnas_abnf::abnf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "list = \"[\" item *( \",\" item ) \"]\"\nitem = 1*ALPHA";
    let mut parser = Tabnas::new();
    abnf(&mut parser, source, None)?;

    let tree = parser.parse("[a,b,c]")?.to_json();
    assert_eq!(tree["src"], "[a,b,c]");
    assert_eq!(tree["kids"].as_array().map(Vec::len), Some(3));
    assert_eq!(tree["kids"][1]["src"], "b");
    Ok(())
}
```

`ALPHA` was never declared there. All sixteen RFC 5234 Appendix B.1 core
rules are spliced in when referenced and not defined locally, and a
local definition always wins.

To build the grammar without installing it, use `abnf_convert`:

```rust
use tabnas_abnf::{abnf_convert, AbnfConvertOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = AbnfConvertOptions {
        start: Some("second".to_string()),
        ..AbnfConvertOptions::default()
    };
    let spec = abnf_convert("first = \"a\"\nsecond = \"b\"", Some(&options))?;

    assert!(spec.rule.contains_key("second"));
    let mut parser = tabnas::Tabnas::new();
    spec.install(&mut parser)?;
    assert_eq!(parser.parse("b")?.to_json()["rule"], "second");
    Ok(())
}
```

## Build a value instead of a tree

A trailing comment can say what a rule builds, and there are exactly two
words. `@object` names one member per part that produces a value;
`@array` names nothing and takes every such part as an element, in
order. A literal produces no value and is never a member.

```rust
use tabnas::Tabnas;
use tabnas_abnf::abnf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = concat!(
        "ver = maj \".\" min \".\" pat   ; @object maj min pat\n",
        "maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n"
    );
    let mut parser = Tabnas::new();
    abnf(&mut parser, source, None)?;

    let value = parser.parse("1.2.30")?.to_json();
    assert_eq!(value["maj"], "1");
    assert_eq!(value["pat"], "30");
    Ok(())
}
```

An annotation is about the OUTPUT, never about what the grammar accepts:
strip every annotation and the same inputs parse, into the tree they
always did. Values nest, so a part whose own rule is annotated is
assigned whole.

## Compile a grammar to pure data

`abnf_compile` serializes the compiled grammar as tabnas grammar text
with no functions in it at all, which any engine can load later:

```rust
use tabnas::Tabnas;
use tabnas_abnf::{abnf_compile, AbnfCompileOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = AbnfCompileOptions {
        recognition: false,
        strict: true,
        ..AbnfCompileOptions::default()
    };
    let text = abnf_compile("pair = \"a\" \"b\"", &options)?;
    assert!(text.contains("@node$"));

    let mut parser = Tabnas::new();
    parser.grammar_json(&text)?;
    assert_eq!(parser.parse("ab")?.to_json()["rule"], "pair");
    Ok(())
}
```

`recognition: true`, the default, drops the tree builders as well, for a
grammar that only has to say yes or no. `strict: true` emits valid JSON;
the relaxed default emits jsonic, which is smaller to read.

## Attach Rust callbacks

User semantic actions attach by rule phase or by alt mark, and
`mark_listing` prints the marks the compiler assigned:

```rust
use std::sync::Arc;

use tabnas::{Tabnas, Value};
use tabnas_abnf::{abnf, AbnfOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = Tabnas::new();
    abnf(
        &mut parser,
        "op = \"inc\" / \"dec\"",
        Some(&AbnfOptions::default().with_actions(vec![(
            "@op:o:INC".to_string(),
            vec![Arc::new(|rule: &mut tabnas::Rule, _ctx: &mut tabnas::Context| {
                if let Some(node) = rule.node.borrow_mut().as_object_mut() {
                    node.insert("delta".to_string(), Value::Number(1.0));
                }
                Ok(())
            })],
        )])),
    )?;

    assert_eq!(parser.parse("inc")?.to_json()["delta"], 1.0);
    Ok(())
}
```

## The dialect

Rules are RFC 5234 ABNF, not the `<x> ::= a | b` style.

| Construct | Spelling |
|---|---|
| definition | `name = elements`, and `name =/ more` to add alternatives |
| choice | `/` |
| literal | `"text"`, case-insensitive by default; `%s"Text"` forces case, `%i"text"` states the default |
| numeric value | `%x41`, `%d65`, `%b1000001`, the range `%x41-5A`, the concatenation `%x0D.0A` |
| repetition | `*A`, `1*A`, `m*nA`, `*nA`, `nA` |
| option and group | `[ A ]`, `( A / B )` |
| comment | `;` to end of line |
| prose | `<text>`, only as the whole body of a rule naming a built-in lexer token |

A literal has NO escape sequences: RFC 5234 `char-val` is
`DQUOTE *(%x20-21 / %x23-7E) DQUOTE`, so a backslash is just `%x5C` and
`"\"` is the one-character literal every RFC that defines `quoted-pair`
writes. `"\n"` is two characters, not a newline.

Rule names are not keyword-restricted, so `true`, `false` and `null` are
ordinary rule names, which JSON's own ABNF relies on.

Left recursion is rewritten automatically, `P = P a / b` becoming
`P = b *(a)`, and an optional prefix whose vocabulary overlaps what
follows it (`[ userinfo "@" ] host`) is resolved with a probe and a
retry pass.

## Options

| Option | Effect |
|---|---|
| `start` | Start rule name (default: the first production). |
| `tag` | Group tag stamped on every emitted alt, and the prefix of every diagnostic (default `abnf`). |
| `builtins` | Emit probe dispatch and tree building as engine `$`-builtin refs instead of closures, keeping the grammar function-free and serialisable. |
| `marks` | Emit a stable mark per user-rule alt, enabling `@<rule>:o\|c:<mark>` action references. |
| `word_keywords` | Treat word-like literals as whole-word keywords, so `"option"` does not match the prefix of `optional`. |
| `provenance` | Emit the map from each generated rule name back to the production it came from. On by default. |

## Install

The `tabnas` and `tabnas-bnf` crates are not published to a registry, so
both are consumed as **sibling checkouts**, the standard tabnas
development model. Clone `https://github.com/tabnas/parser` and
`https://github.com/tabnas/bnf` next to this repository and point at
them:

```toml
[dependencies]
tabnas-abnf = { path = "../abnf/rs" }
tabnas = { path = "../parser/rs" }
```

Both entries are needed. A crate's dependencies are not passed on to its
dependents, so `tabnas-abnf` alone does not put `tabnas` in the extern
prelude, and the examples above that name `tabnas::Tabnas` would not
resolve.

## Differences from the canonical TypeScript

Every grammar in the third-party corpus this suite can finish inside its
budget is accepted or rejected here exactly as the other two
implementations answer it, and that IS a test:
`tests/conformance_test.rs` measures it over the same 68 published ABNF
grammars the other two suites read. The dial this crate prints is one
rejection lower than theirs, because one grammar that all three do
reject takes about 161 seconds in the debug build this suite runs,
against about 13 and 16 seconds in the other two, and so runs out the
shared 60 second budget. `../AGENTS.md` names the file, the cause and
the owner.

The emitted grammar text was separately compared with the canonical
compiler's, byte for byte, over every grammar in that corpus either
compiler can finish, and agreed on all 66; that comparison needs a built
TypeScript checkout, so it is a procedure a maintainer repeats rather
than a committed test.

The differences are in the surface:

- **A failure is returned, never raised.** `parse_abnf`, `abnf_convert`,
  `abnf_compile` and `abnf` all answer a `Result`. The 35 diagnostics
  `test/spec/alignment-abnf-errors.tsv` names carry the same text in
  every runtime, which that fixture compares byte for byte across all
  three implementations. Diagnostics outside those rows are held to the
  canonical text by this crate's own tests, not by a cross-runtime one,
  and `../DIVERGENCE.md` records the class this crate does not word
  itself.
- **There is no instance decoration.** TypeScript adds a callable
  `tn.abnf` member to the engine; Rust has no such thing, so the install
  path is the free function `abnf(&mut parser, src, opts)` and the
  convert-only path is `abnf_convert(src, opts)`.
- **There is no command line tool.** TypeScript ships `tabnas-abnf` and
  the Go module ships `cmd/tabnas-abnf`; this crate is a library only,
  and ships no binary target. Everything either command does is a call
  to `abnf_convert`, `abnf` or `abnf_compile`, so the gap is a packaging
  decision rather than a capability one. `tests/compile_test.rs` covers
  the pure-data path a command would print.
- **Source spans count bytes.** A span's offsets are in the units the
  engine's own tokens use, and this engine counts bytes where TypeScript
  counts UTF-16 code units. Slicing the original source with a span
  gives the right text in either runtime.
- **A surrogate code point becomes the replacement character.**
  `%xD800` names half of a UTF-16 pair, which no Rust `String` can hold;
  TypeScript answers a lone surrogate. The Go port does the same as this
  one. No ABNF grammar in the conformance corpus writes one.
- **The `%s` and `%i` prefixes are recognised by a callback.** The
  canonical matcher is the lookahead `^%[sS](?=")`, which the engine's
  regular expression dialect has no lookaround for, so the two
  characters are consumed by a hand-written matcher that leaves the
  quote for the string lexer, as the Go port does.
- **Bracket nesting is refused past a documented cap.** A grammar
  arrives from outside the system, the parse tree nests once per
  bracket, and a Rust stack that runs out aborts the process rather than
  unwinding. About five hundred nested groups or options is the limit,
  which the shared compiler's own limit of 128 levels of element nesting
  reaches long before.
- **A reversed numeric range comes back in other words.** Every runtime
  refuses `%x5A-41`, and none of them writes that sentence: each hands
  the pattern to the regular expression engine it carries, and reports
  what comes back.

## Build and test

Both dependencies are path dependencies on sibling checkouts, so there
is nothing to fetch:

```bash
cargo test --all-targets
cargo test --doc
```

Or, from the repository root, `make test-rs`. For what CI would say,
including formatting and the `Cargo.lock` check, run `ci/rust/run.sh`.

The suite runs the four shared `test/spec/*.tsv` fixtures that keep all
three implementations in step with each other, ports the TypeScript and
Go unit suites, and runs the third-party conformance corpus of 68
published ABNF grammars. That corpus is fetched, never committed, and a
missing corpus FAILS the suite rather than skipping it.

## License

MIT.
