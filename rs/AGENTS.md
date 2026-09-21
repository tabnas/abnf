# Agents Guide — rs/

The Rust port of the canonical TypeScript in [`../ts`](../ts). Read
[`../AGENTS.md`](../AGENTS.md) first: it holds the cross-runtime rules
(TypeScript wins, what the dialect is, what a value annotation means,
the shared fixtures, the conformance instrument, the version sites), and
this file only covers what is specific to this crate.

## What is here, and what is not

This crate is the RFC 5234 FRONT-END and nothing else. It parses ABNF
text into the notation-neutral grammar IR that
[`tabnas-bnf`](../../bnf/rs) compiles, exactly as `ts/src/converter.ts`
and `go/converter.go` do:

```text
ABNF text ──parse_abnf──▶ tabnas_bnf::Grammar ──emit_grammar_spec──▶ GrammarSpec
```

Everything downstream of that arrow (desugaring, left-recursion
elimination, tail repeats, probe dispatch, literal lifting, token
allocation, first-set analysis, chain emission, the value-annotation
planner) lives in `tabnas-bnf`. A defect in the emitted grammar is
almost always a defect there, not here. What belongs here is the
meta-grammar, the Appendix B.1 core rules, incremental alternatives,
numeric values, case-insensitive literals, and reading a value
annotation out of a trailing comment.

## Layout

| Path | Mirrors |
|---|---|
| `src/lib.rs` | `ts/src/abnf.ts` and `go/facade.go`: `VERSION`, `abnf`, `abnf_convert`, `emit_grammar_spec`, `plugin`, `AbnfError`, and the re-exports of the shared compiler under this package's historical names |
| `src/parser_abnf.rs` | the `abnfRules` table plus `getAbnfParser` in `ts/src/converter.ts`, and `go/parser_abnf.go`: the meta-grammar as a tabnas grammar document, its engine options, and the AST-assembly closures |
| `src/converter.rs` | the rest of `ts/src/converter.ts`: `parse_abnf`, the annotation reader, `merge_incrementals`, `reject_holes`, the core rules, `AbnfParseError` |
| `src/numeric.rs` | `parseNumericValue` |
| `src/compile.rs` | `ts/src/compile.ts` and `go/compile.go`: `abnf_compile` |
| `tests/parity_test.rs` | `go/parity_test.go`: the four shared `test/spec/*.tsv` fixtures |
| `tests/abnf_test.rs` | `go/abnf_test.go` and `ts/test/abnf.test.js` |
| `tests/compile_test.rs` | `go/compile_test.go` |
| `tests/leftrec_test.rs` | `go/leftrec_test.go` |
| `tests/probe_test.rs` | `go/probe_test.go` |
| `tests/rfc3986_test.rs` | `go/rfc3986_test.go` |
| `tests/spans_test.rs` | `go/spans_test.go` |
| `tests/class_overlap_test.rs` | `go/class_overlap_test.go` |
| `tests/actions_test.rs` | `go/actions_test.go` |
| `tests/value_annotation_test.rs` | `go/value_annotation_test.go` |
| `tests/conformance_test.rs` | `go/conformance_test.go` |
| `tests/untrusted_test.rs` | no twin: the boundaries a Rust port has to state, because a stack that runs out aborts rather than unwinding |
| `tests/perf_test.rs` | no twin: two ratio comparisons, both measured on one machine in one run |
| `tests/version_test.rs` | the five version sites must agree |
| `README.md` | the crate front page; its `rust` fences run as doctests |

## The parse AST is engine values

`Rule::node` is a `tabnas::Value`, so the AST the meta-grammar's actions
build is a tree of `Value::Object`s in exactly the shape the IR
serializes to. `production_from_value` then deserializes it into
`tabnas_bnf::Production` with serde. That is deliberate, and it is the
closest mirror of the canonical runtime there is: TypeScript builds
plain objects with those same field names, so the two representations
cannot drift apart without serde saying so.

Three consequences.

**A rule's node is a shared cell.** A pushed rule INHERITS its parent's
`Rc<RefCell<Value>>`, so `set_node` rebinds the cell (TypeScript's
`r.node = []`) while `push_node` writes through it (TypeScript's
`r.node.push(...)`). Getting that backwards silently overwrites whatever
the parent was accumulating. Every `bo` hook that starts a fresh
collection uses `set_node`; `@elem-close` and `@prod-bc`, which append
to a collection their parent owns, use `push_node`.

**Numbers arrive as doubles.** The engine's number is an `f64`, so a
span offset and a repetition count both reach serde as `8.0`, which no
`usize` field will take. `integral` rewrites every whole number in the
JSON tree before deserialization. Nothing in the IR is fractional, so
that conversion is total rather than a heuristic.

**A hole is `Value::Undefined`.** `bad = ( "a"` runs the atom's bail
alternate at end of source, so the atom pops with no node and there is
nothing to wrap. The canonical runtime pushes the `undefined` it found
and rejects the rule by name in `rejectHoles`; this port does the same
rather than dropping the element, because dropping it lets the
malformed grammar compile clean. `reject_holes` walks the same shape
the shared compiler's `refs_in` walks, one level into every repetition
and group, because a repetition wrapping an unclosed group leaves a
perfectly real `star` whose `inner` is the hole.

## The numeric diagnostic travels in a thread local

The canonical runtime throws out of the alt action that decodes `%x…`,
so `bad = ( %x110000` reports the numeric fault even though the
unclosed group it sat in was discarded. An alt action here CAN return an
error, but the engine would wrap it with a position the canonical
diagnostic does not carry, and `test/spec/alignment-abnf-errors.tsv`
pins those bytes. So the message is recorded in a thread local and read
once the parse is structurally complete, which is the same shape the Go
port reaches with its per-element `NumErr` field.

The order of the checks in `parse_abnf` is itself part of the contract,
and the shared errors fixture pins it: the numeric diagnostic first,
then the incremental-merge refusal, then the malformed element. A
rejection naming a different cause in each runtime is the divergence
that order exists to close.

## Things that look like bugs and are not

- **`MAX_GROUP_DEPTH` counts RULE levels, not brackets.** A bracket
  costs four rule pushes (`atom`, `alts`, `seq`, `elem`), so 2048 admits
  about five hundred nested groups. The shared compiler refuses element
  nesting past 128 anyway, so every grammar this cap turns away was
  going to be refused regardless, only with a worse failure.
- **`%s` and `%i` are callback matchers.** The canonical pattern is the
  lookahead `^%[sS](?=")`, and the engine's `regex` dialect has no
  lookaround. The callback consumes the two characters and leaves the
  quote for the string lexer, as `go/parser_abnf.go` does.
- **The `#NV` and `#PV` matchers are NOT eager.** The Go port marks them
  eager because its engine gates non-eager match tokens by alt position
  zero; this engine takes the expected token list per POSITION, as the
  TypeScript engine does, so the meta-grammar's `#TX #ATOM` lookahead
  alternatives are enough on their own. That is why those alternatives
  are carried verbatim from the canonical table rather than merged.
- **Spans carry `site.si` and `token.len`, both bytes.** Slicing the
  original source with a span gives the right text; `tests/spans_test.rs`
  asserts exactly that rather than asserting the numbers.
- **A surrogate code point becomes U+FFFD.** No Rust `String` holds one.
  Recorded in `../DIVERGENCE.md`.
- **The meta-grammar instance is a `OnceLock`.** `Tabnas` parses through
  `&self` and is `Send + Sync`, so one instance serves every caller and
  every thread; per-parse state lives on the rules and the context.

## Running it

`make test-rs` from the repository root is the fast loop, and it depends
on `make abnf-corpus` because the conformance suite FAILS rather than
skips without the third-party corpus. `ci/rust/run.sh` is the full gate
and is what CI would run: it adds `cargo fmt --check`, a build,
doctests, clippy with `-D warnings` and the lockfile check.

The conformance sweep compiles every corpus grammar in its own budgeted
process and every mutant in this one, so on the unoptimised profile it
takes minutes. `cargo test --release --test conformance_test` is the
same measurement in about ninety seconds. Re-measure the pinned rows
with `ABNF_CONFORMANCE_RECORD=1`, which prints the `rust` rows of
`test/corpus/known-gaps.tsv` and asserts nothing.

`tabnas`, `tabnas-bnf` and `tabnas-support` are sibling checkouts at
`../../parser`, `../../bnf` and `../../support`.

## The README is doctested and gated

`src/lib.rs` includes `README.md` as crate documentation under
`#[cfg(doctest)]`, so `cargo test --doc` compiles and runs every `rust`
fence in the README exactly as it appears on the page. Keep each fence a
complete `fn main` example (no top-level `?` outside `main`, no hidden
`# ` lines), and expect `readme_examples (line N)` entries in the
doctest output, one per fence.

The README is in the published set and is gated by
[`../docs/STYLE-GUIDE.md`](../docs/STYLE-GUIDE.md): no em dashes in
prose, no first person singular, no link from it to any `AGENTS.md`, no
project history. Adding or editing it changes the alert totals recorded
in `.vale.ini` and `docs/STYLE-GUIDE.md`; re-measure with `node
ts/scripts/vale-counts.cjs --write` and re-wrap any comment whose
numbers changed length.

## Checking against the canonical compiler

The strongest check this port has is a whole-corpus diff. Build the
TypeScript side (`npm install && npm run build` in `../../parser/ts`,
then `../../bnf/ts`, then `../ts`), emit
`toJsonic(toPureSpec(abnfConvert(src, {builtins: true, marks: true})),
{strict: true, indent: 2})` for every `.abnf` file under
`test/abnf-corpus`, and compare it byte for byte with the same pipeline
here. All 66 grammars that either compiler can finish agree exactly; the
other two exceed the budget in both.
