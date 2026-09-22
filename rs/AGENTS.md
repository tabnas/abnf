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
| `tests/abnf_test.rs` | `go/abnf_test.go` and `ts/test/abnf.test.js`, including a runner for every `.abnf` grammar under `ts/test/grammar/`, and the two assertions of `ts/test/lifting.test.js` that no shared fixture column reaches |
| `tests/compile_test.rs` | `go/compile_test.go` |
| `tests/leftrec_test.rs` | `go/leftrec_test.go` and the `left-recursion elimination` suite in `ts/test/abnf.test.js` |
| `tests/probe_test.rs` | `go/probe_test.go` |
| `tests/token_test.rs` | `go/token_test.go` and `ts/test/token.test.js` |
| `tests/rfc3986_test.rs` | `go/rfc3986_test.go` |
| `tests/spans_test.rs` | `go/spans_test.go` and the `source spans` suite in `ts/test/abnf.test.js` |
| `tests/class_overlap_test.rs` | `go/class_overlap_test.go` |
| `tests/actions_test.rs` | `go/actions_test.go` |
| `tests/value_annotation_test.rs` | `go/value_annotation_test.go` |
| `tests/conformance_test.rs` | `go/conformance_test.go` |
| `tests/untrusted_test.rs` | no twin: the boundaries a Rust port has to state, because a stack that runs out aborts rather than unwinding |
| `tests/perf_test.rs` | no twin: two ratio comparisons, both measured on one machine in one run |
| `tests/divergence_test.rs` | no twin: every entry of `../DIVERGENCE.md`, both halves, the canonical one by running `node` over `../ts/dist/abnf.js` |
| `tests/version_test.rs` | the five version sites must agree |
| `README.md` | the crate front page; its `rust` fences run as doctests |

Two rows above name a suite that the canonical side keeps INSIDE
`ts/test/abnf.test.js` rather than in a file of its own. A census taken
over file names reads that as a behaviour pinned in the ports alone, and
tabnas/abnf#73 did: `leftrec` and `spans` were listed as having no
TypeScript twin. They have one, and it is the larger of the two, so the
drift ran the other way. Name the canonical suite in this table when a
port splits one out, so the next census over file names does not have to
guess.

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
- **A LONE surrogate code point becomes U+FFFD.** No Rust `String` holds
  one. Recorded in `../DIVERGENCE.md` as entry 1.
- **An ADJACENT surrogate pair is the character it encodes, and that is
  not the same thing.** `%xD800.DC00` is a dotted concatenation, and the
  canonical runtime joins the parts into one JavaScript string before
  anything asks what characters it holds: the two halves are then the
  well-formed pair for U+10000, which every runtime can represent. So
  `parse_numeric_value` builds the whole sequence as UTF-16 code units
  and decodes it ONCE, with `push_utf16` and
  `String::from_utf16_lossy`, rather than converting a part at a time.
  Converting per part answered two U+FFFD, which REJECTED the character
  the grammar names and ACCEPTED a document carrying two replacements.
  A half the join leaves stranded is entry 1 again, one U+FFFD each.
  Go still converts per part; `../DIVERGENCE.md` entry 1 records that
  under "An ADJACENT pair is not this entry".
- **The meta-grammar instance is a `OnceLock`.** `Tabnas` parses through
  `&self` and is `Send + Sync`, so one instance serves every caller and
  every thread; per-parse state lives on the rules and the context.
- **The annotation comment is lexed with JAVASCRIPT's whitespace.**
  `str::trim`, `char::is_whitespace` and the `regex` crate's `\s` are
  the Unicode `White_Space` property, which holds U+0085 and lacks
  U+FEFF; ECMAScript's WhiteSpace plus LineTerminator is the other way
  round. A `;` comment takes any character, so `is_js_whitespace` in
  `src/converter.rs` spells the set out, `(?-u:\b)` keeps the keyword
  boundary ASCII, and the body class excludes every LineTerminator
  because JavaScript's `.` does and the crate's `.` excludes only `\n`.
- **A numeric value is read and printed as ECMAScript does.**
  `parse_int` rounds ONCE from the exact integer, because `parseInt`
  does and a repeated multiply-and-add in a double does not: the two
  part company above 2^53, where `%d12345678901234567890` read as
  `12345678901234570000` instead of `12345678901234567000`. That is
  visible in the out-of-range diagnostic and `tests/abnf_test.rs` pins
  it. `number_to_string` is the specification's `Number::toString`
  rather than Rust's shortest float form, and that one is NOT visible
  from outside this module: the only values it can be handed are the
  non-negative integral doubles, `Infinity` and `NaN` that `parse_int`
  answers, and on those the two renderers agree everywhere (449,995
  reachable values compared, none differed; about 7,000 of 200,000
  random doubles differ, and none of those is reachable). So it is
  pinned by a unit test inside `src/numeric.rs`, where the difference
  can be reached, rather than by a test under `tests/`. Do not delete
  that unit test on the grounds that an integration test covers it:
  none can.
- **A probe and retry keeps the node it built**, where the canonical
  runtime answers an empty one. That is the ENGINE's answer, not this
  crate's: the emitted `GrammarSpec` is byte identical in all three
  runtimes. Recorded in `../DIVERGENCE.md` as entry 5.

## What this crate does not have

No binary target. The canonical package ships the `tabnas-abnf` command
and the Go module ships `cmd/tabnas-abnf`; this crate is a library only,
and `../AGENTS.md` records that in its repository map. Everything either
command does is a call to `abnf_convert`, `abnf` or `abnf_compile`, so
adding one is packaging rather than capability, and nothing in the
shared fixtures or the conformance instrument reaches a command line.

No error codes either, in any runtime: `../AGENTS.md` under "Error
codes" states that this package declares none, `tabnas.plugin.json`
carries an empty `errorCodes`, and no fixture row pins an
`ERROR:<code>`. What the shared errors fixture pins is the rendered
message, byte for byte, this crate included.

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

Two things about that sweep are load bearing, and both look like
plumbing:

- **Only the child's own watchdog is budget exhaustion.** The child
  exits 3 when its 256MB or 60s cap fires, and the parent scores that,
  and a kill at the parent's own deadline, as a failure to finish.
  EVERY other ending -- a panic, an abort, a stack that ran out, a
  loader failure -- fails the parent by name. It cannot be folded into
  the budget, because the invalid half scores on `!ok`: a crash counted
  as budget exhaustion is a crash counted as the compiler correctly
  REJECTING an invalid grammar, and a regression that aborts on bad
  input would leave the suite green.
- **The children run a PINNED COPY of the test binary.** The sweep takes
  minutes and the parent re-executes itself ~1500 times; a rebuild in
  another terminal replaces or unlinks the path `current_exe` answers,
  so later cases would measure a different artifact, or fail to spawn
  (`Os { code: 2, kind: NotFound }`, seen in practice). The copy is
  taken before the first child and removed when the sweep ends.

The divergence suite runs the CANONICAL implementation. It starts `node`
on `../ts/dist/abnf.js` once per run and asserts the TypeScript column of
every table in `../DIVERGENCE.md`, so an entry that closes from the
canonical side fails as loudly as one that closes here. That build is not
committed: without it `tests/divergence_test.rs` FAILS rather than
skipping, and `make build-ts` is what produces it. `ABNF_CANONICAL=off`
turns the canonical half off where it genuinely cannot be built;
`ci/rust/run.sh` sets that when the canonical is missing and prints a
warning saying the canonical half went unmeasured, which is the only
state in which a green Rust gate has not checked the other side.

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

## A long single rule is quadratic, and the engine owns it

Parsing scales linearly in the NUMBER of rules and quadratically in the
number of elements inside ONE rule. Measured on the release profile: a
rule concatenating 2500, 5000, 10000 and 20000 literals takes 0.19s,
0.75s, 3.67s and 12.75s, where the canonical runtime takes about 0.12s
at every size.

The cause is not in this crate. `Rule::accept_child_node` in the engine
(`../../parser/rs/src/rule.rs`) does `self.child_node =
child.node.borrow().clone()`. A `Value` container sits behind an `Arc`,
so that clone is a second reference to the very array the next
`push_node` mutates, and `Arc::make_mut` then copies the whole array
once per element. The engine already computes `child_node_is_self` on
the line above, which is the condition under which the clone is the
parent's own accumulator and can be skipped.

The same curve shows with a compiled grammar and no ABNF in sight:
`list = item *( "," item )` over 1000, 2000, 4000 and 8000 items takes
0.21s, 0.64s, 2.40s and 13.85s here against 0.09s, 0.18s, 0.55s and
1.78s in the canonical runtime. Every answer is the same; only the cost
differs, so this is not in `../DIVERGENCE.md`.

`tests/untrusted_test.rs` measures the rule-count dimension, which is
linear. Adding a ratio assertion for the element-count dimension would
pin the quadratic as acceptable, so the number is recorded here instead
and belongs to whoever fixes the engine.

## Checking against the canonical compiler

The strongest check this port has is a whole-corpus diff. Build the
TypeScript side (`npm install && npm run build` in `../../parser/ts`,
then `../../bnf/ts`, then `../ts`), emit
`toJsonic(toPureSpec(abnfConvert(src, {builtins: true, marks: true})),
{strict: true, indent: 2})` for every `.abnf` file under
`test/abnf-corpus`, and compare it byte for byte with the same pipeline
here. All 66 grammars that either compiler can finish agree exactly; the
other two exceed the budget in both.
