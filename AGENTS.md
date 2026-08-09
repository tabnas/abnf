# Agents Guide — abnf

## What this project is

`@tabnas/abnf` is a **grammar compiler**: it reads ABNF source and emits
a [`@tabnas/parser`](https://github.com/tabnas/parser) `GrammarSpec`,
optionally installing it on a tabnas instance. Where most tabnas grammar
packages hand-write one fixed grammar, this one is a *meta* plugin — the
grammar it installs is whatever ABNF text you feed it at runtime.

It is a **plugin** for the tabnas engine. Once installed it decorates the
instance with a callable `abnf` member:

- `tn.abnf(src)` compiles `src` and installs the resulting grammar on `tn`.
- `tn.abnf.toSpec(src)` compiles and returns the spec **without** installing.
- Bare exports `abnfConvert(src, opts)` / `parseAbnf` / `emitGrammarSpec`
  let you convert without an instance.

The compiler synthesises a `__start__` wrapper rule that pushes the real
start production and consumes end-of-source (`#ZZ`); `abnfConvert` sets
`spec.options.rule.start = '__start__'`. The start production defaults to
the first one declared and can be overridden (`opts.start` / CLI `--start`).

## The dialect is ABNF, not classic BNF

Rules are RFC 5234 ABNF, **not** the `<x> ::= a | b` style:

- `name = element ...` — `=` defines a rule, not `::=`.
- `/` is choice (`greet = "hi" / "hello"`), not `|`.
- Literals are **double-quoted** and **case-insensitive** by default;
  `%s"…"` forces case-sensitivity, `%i"…"` states the default.
- A literal has **no escape sequences**: RFC 5234's `char-val` is
  `DQUOTE *(%x20-21 / %x23-7E) DQUOTE`, so a backslash is just `%x5C`
  and `"\"` is the one-character literal every RFC spells `quoted-pair`
  with. `"\n"` is two characters, not a newline. (The parser sets
  `string.escapeChar` to DEL, which no legal `char-val` can contain, to
  turn the engine's JSON-style escaping off.)
- `;` starts a line comment.
- Repetition / option / group: `*A`, `1*A`, `m*nA`, `*nA`, `nA`,
  `[ A ]`, `( A / B )`. Every form works after any element, including
  after a bare rulename (`a 1*b`, `simple-key 1*( dot-sep simple-key )`).
- `name =/ alt` incrementally adds alternatives to an existing rule.
- **Numeric values are fully supported**, in all three bases and all
  three forms: single (`%x41`, `%d65`, `%b1000001`), range
  (`%x41-5A`), and concatenation (`%x0D.0A`). Code points above the BMP
  work (`%x1F600`, `%xE000-10FFFF`); anything above `%x10FFFF` is
  rejected with a diagnostic naming the value.
- **Rulenames are not keyword-restricted.** RFC 5234 `rulename` is
  `ALPHA *(ALPHA / DIGIT / "-")`, so `true`, `false` and `null` are
  ordinary rule names — which JSON's own ABNF relies on. The parser sets
  `value.lex: false` so the engine's keyword-value lexing doesn't claim
  them.
- All **16** RFC 5234 Appendix B.1 **core rules** — `ALPHA`, `BIT`,
  `CHAR`, `CR`, `CRLF`, `CTL`, `DIGIT`, `DQUOTE`, `HEXDIG`, `HTAB`,
  `LF`, `LWSP`, `OCTET`, `SP`, `VCHAR`, `WSP` — are auto-included when
  referenced and not locally defined; a local `DIGIT = …` always wins.
  They are defined in `converter.ts` (search `RFC 5234 Appendix B.1`)
  and emitted as flattened `core` nodes so a matched char class doesn't
  litter the tree with one node per character.

Classic-BNF `::=` / `|` does **not** parse. (Some stale comments in
`src/converter.ts` and a CLI example in `ts/README.md` still show `::=` —
ignore those; the parser only accepts the ABNF forms above.)

## Repository map

| Path | What it is |
|---|---|
| [`ts/`](ts/) | **Canonical** implementation — the `@tabnas/abnf` package, plus the `tabnas-abnf` CLI. |
| [`ts/src/abnf.ts`](ts/src/abnf.ts) | Plugin entry point. Wires `tn.abnf` / `tn.abnf.toSpec` and re-exports the converter. Thin. |
| [`ts/src/converter.ts`](ts/src/converter.ts) | The whole compiler (~2.3k lines): ABNF parser (`parseAbnf`), left-recursion rewriter (`eliminateLeftRecursion`), probe-dispatch analyser, and the `GrammarSpec` emitter (`emitGrammarSpec`). |
| [`ts/src/bin/tabnas-abnf-cli.ts`](ts/src/bin/tabnas-abnf-cli.ts) | CLI implementation (`run(argv, console)`). |
| [`ts/bin/tabnas-abnf`](ts/bin/tabnas-abnf) | Executable shim → `dist/bin/tabnas-abnf-cli`. The `bin` entry in `package.json`. |
| [`ts/test/`](ts/test/) | `node --test` suite (see below). |
| [`ts/test/grammar/`](ts/test/grammar/) | `.abnf` fixture grammars (`greet`, `pair`, `arith`, `arith-leftrec`, `json-subset`, `rfc3986-uri`). |
| [`go/`](go/) | Go port (`package tabnasabnf`), tracking the TS implementation; facade in [`go/facade.go`](go/facade.go), ABNF parser in [`go/parser_abnf.go`](go/parser_abnf.go), CLI in [`go/cmd/tabnas-abnf`](go/cmd/tabnas-abnf). |

The usual tabnas "Go port tracks TS" contract applies: the `go/` directory
mirrors the TypeScript implementation (`Abnf`, `ParseAbnf`, `AbnfCompile`,
matching the TS `abnfConvert` / `parseAbnf` / `abnfCompile`).

## How the compiler is itself a tabnas grammar

The ABNF source is parsed by a tabnas instance whose grammar is the
declarative `abnfRules` table inside `converter.ts` — i.e. the converter
eats its own dog food. The emitter then walks that AST and produces the
output `GrammarSpec`.

Unlike the json/csv plugins (which layer on jsonic's grammar and prune
unwanted rules with `tn.rule(name, null)`), the compiled output is a
**complete grammar built from the ABNF source alone** — `abnfConvert`
returns a freestanding spec, and the CLI's parse mode runs it on a bare
`new Tabnas()` with no other plugin. (The install path does call
`j.rule(name, null)` internally while wiring rules onto the instance.)

Non-obvious things an agent should know before touching `converter.ts`:

- **Left recursion is rewritten automatically.** Direct left recursion
  `P = P a / b` becomes `P = b *(a)` (`eliminateLeftRecursion`). See the
  `arith-leftrec.abnf` fixture, which must parse identically to `arith.abnf`.
- **Optional-prefix ambiguity uses a probe + phase-retry pattern.** For
  shapes like `[X D] Y` where X and Y share a character vocabulary and D
  is a terminal disambiguator, the rewriter synthesises a *dispatcher*
  rule that marks the token position, runs a failure-proof `*vocab`
  probe, peeks `ctx.t[0]`, rewinds, and commits to the right branch on a
  retry pass. This is the trickiest part of the compiler; `probe.test.js`
  documents and pins it. Don't "simplify" it without re-reading that test.
- **Synthetic rules.** Multi-segment alternatives are chained through
  `<prodname>$stepN` continuation rules; probe machinery adds dispatcher
  and `*vocab` helper rules. Output AST nodes carry a `nodeKind`
  (`user` / `core` / `helper`); only `user` nodes get their own tree
  node, the others flatten their `src`/`kids` into the enclosing rule.
- **The RFC 5234 notation itself is complete** — every construct listed
  under "The dialect is ABNF" above parses and compiles. What remains
  partial is not the *notation* but the three limits below. When you
  extend the dialect, add a fixture grammar under `ts/test/grammar/` and
  an end-to-end test — plus a cross-runtime case under
  [`test/spec/`](test/) so `go/` cannot drift (see
  [`test/AGENTS.md`](test/AGENTS.md)).
- **Prose-val is deliberately narrow.** It is supported only as the whole
  body of a production naming a built-in lexer token (`NR = <number>`),
  where it is informational, and as the `<remove>` directive. General
  prose such as RFC 3986's `path-empty = 0<pchar>` is an error, since
  there is no definition behind it — see the notes at the top of
  `ts/test/grammar/rfc3986-uri.abnf` for the rewrite that fixture needed.
- **Paull's substitution can blow up on large mutually-recursive
  grammars.** The full RFC 5322 (`email.abnf`) and Dhall grammars do not
  finish compiling. This is the "pathological grammars grow" caveat the
  README already states, not a separate bug: substitution runs over every
  production to collect the multi-token `altPrefixes` that populate tcol,
  so it cannot yet be scoped to the cyclic SCCs. Keep grammars
  reasonably small.
- **Alt dispatch is one-token lookahead plus the probe pattern.** Two
  alternatives sharing an arbitrarily deep prefix with no terminal
  tie-breaker (`S = A Z / A Y`), or an `[X B] C` whose disambiguator is a
  nonterminal, need generalised catch-and-rewind at the alt-dispatch
  level, which the emitter does not provide. The two `it.skip` cases at
  the end of `ts/test/probe.test.js` mark exactly that boundary and are
  skipped on purpose — they are capability documentation, not broken
  tests. (`rfc3986.test.js` carries a third, for the same reason.)

### Conformance, as measured

`test/abnf-corpus/` holds four other ABNF implementations (`ex_abnf`,
`go-abnf`, `node-abnf`, `tree-sitter-abnf`), kept for their grammar
corpora — 68 `.abnf` files, including the collected ABNF of RFC 3261,
3986, 4566, 5322, 7405, JSON, JSONPath, TOML and Dhall. There is **no
official IETF conformance suite for RFC 5234**, so this is the closest
thing that exists: the ABNF real RFCs publish, as collected by other
people. It is **not vendored** — four separately-licensed upstream repos
do not belong in this tree, and a checkout carrying its own `.git` would
commit as a mode-160000 gitlink and ship an empty directory to everyone.
It is fetched, pinned to exact commit SHAs, by
`test/fetch-abnf-corpus.sh`.

It is no longer a reference corpus you measure by hand. Both runtimes
run it as a suite — `ts/test/conformance.test.js` and
`go/conformance_test.go` — and both **fail, never skip**, if it is
absent. You should never have to fetch it yourself: `npm test` does so
through the `pretest` hook, `go test` from `TestMain`, and `make
test-go` depends on `make abnf-corpus`. CI therefore runs the
conformance suite on every push.

How it is judged, and by whom:

- Each file's class (`valid` / `invalid` / `fragment`) is in
  `test/corpus/manifest.tsv` and was decided by an **independent
  third-party ABNF parser** (npm `abnf` 5.0.4 == `hildjj/node-abnf`),
  not by hand and not by this implementation. Regenerate with
  `test/classify-abnf-corpus.sh`. `fragment` files parse but reference
  rules nothing defines, so they are neither must-accept nor
  must-reject: excluded from both halves, and **counted**.
- A valid grammar gets a **value assertion**, not "it didn't throw":
  every rulename the source declares (RFC 5234 §4) must be reachable in
  the compiled `GrammarSpec` as a rule, a fixed token or a match token.
- The must-fail half is widened by the 13 mutation classes in
  `test/corpus/mutations.tsv`, each appending one line that violates a
  named RFC 5234 Appendix B production. Every mutant was confirmed
  rejected by that same third-party oracle before its class was
  admitted.
- Every corpus compile runs **in its own process, under a 256 MB / 60 s
  budget**. The two grammars that hit the Paull's blow-up above
  (`node-abnf/examples/email.abnf`, `tree-sitter-abnf/examples/dhall.abnf`)
  exceed it, in both runtimes. That is recorded as a failure to accept —
  never a pass, never a skip — and it is the only reason those two are
  left out of the mutation half, since a mutant of a base that never
  compiles measures nothing.
- The residual gaps are pinned as an **exact set** in
  `test/corpus/known-gaps.tsv`, per runtime. Fixing one fails the suite
  as loudly as regressing one; the fix is to delete its row. Never edit a
  row to silence a failure you did not fix, and never narrow the corpus
  or loosen an assertion to raise the figure.

Measured on 2026-08-09, at the commit that introduced the suite (run
`make test` and read the dial the conformance tests print):

|                                   | TS        | Go        |
| --------------------------------- | --------- | --------- |
| valid accepted **and** value-correct | 48/52  | 48/52     |
| invalid rejected                  | 611/661   | 513/661   |
| excluded fragments                | 5         | 5         |
| over budget (counted as failures) | 2         | 2         |

The four valid-half gaps are the same files in both runtimes: the two
budget blow-ups, `go-abnf/testdata/void.abnf` (an empty grammar), and
`tree-sitter-abnf/examples/elements.abnf` (the deliberate prose-val
limit above). The invalid-half difference is real and is the largest
TS/Go divergence in the corpus: Go additionally accepts an unclosed
group `( "a" / "b"` and an unclosed option `[ "a"`, which TS rejects.
Both runtimes still accept a dangling alternation `"a" /`.

## The tabnas engine dependency

The engine is consumed as a **sibling checkout** (the same model the rest
of tabnas uses until `@tabnas/parser` publishes tagged releases):

- `@tabnas/parser` is a **`peerDependency`** (`"file:../../parser/ts"`)
  and is mirrored as a `file:` **devDependency** so local builds resolve.
- `@tabnas/debug` and `@tabnas/railroad` are **dev-only** `file:`
  devDependencies — `debug` for the `debug.model()` composition test,
  `railroad` for regenerating the README railroad diagram. Neither is a
  runtime dependency.
- `engines.node` is `">=24"`; npm ≥ 7 auto-installs the peer.

Clone `https://github.com/tabnas/parser` (and `debug`) as siblings of
this repo and build `parser/ts` before working here. CI does this for you
(see below).

## Build & test

From `ts/` (or use the top-level `Makefile`):

```bash
cd ts && npm install && npm run build   # tsc --build src
npm test                                # node --enable-source-maps --test test/**/*.test.js
```

Top-level `Makefile` targets:

```bash
make build        # cd ts && npm run build
make test         # cd ts && npm test
make clean        # rm -rf ts/dist ts/dist-test
make publish-ts   # test, then npm publish --access public
make reset        # cd ts && npm run reset  (clean + install + build + test)
```

The test suite (`ts/test/*.test.js`, run against the built `dist`):

- `abnf.test.js` — the core converter/parser unit suite.
- `probe.test.js` — the probe + phase-retry disambiguation pattern.
- `rfc3986.test.js` — end-to-end: compiles `test/grammar/rfc3986-uri.abnf`
  and parses URIs, exercising most of the supported ABNF surface.
- `doc-examples.test.js` — keeps the README/doc examples honest.
- `debug-model.test.js` — composition test with `@tabnas/debug`: compiles
  a small ABNF grammar, installs the `Debug` plugin, and asserts
  `j.debug.model()` (the rule-name set including the `__start__` wrapper,
  `m.config.start === '__start__'`, the `#ZZ` close, `m.plugins`, and the
  rule-reference graph edges). It **dynamically resolves** `@tabnas/debug`
  and **skips** when absent (or when `TABNAS_DEBUG_PATH` is unset and the
  dep is missing), so it is safe outside the package.

## CLI (`tabnas-abnf`)

`bin/tabnas-abnf` → `dist/bin/tabnas-abnf-cli`. By default it prints the
compiled `GrammarSpec` as JSON. Flags (`run` in
`src/bin/tabnas-abnf-cli.ts`): `-`/stdin, `--file`/`-f`, `--start`/`-s`,
`--tag`/`-t` (group tag on every emitted alt, default `abnf`),
`--compact`/`-c`, `--parse`/`-P` and `--parse-file` (compile, install on a
bare engine, parse the sample(s), print the tree(s), exit non-zero on any
failure), and `--help`/`-h`. Bare non-flag args are treated as inline ABNF
source. Example: `tabnas-abnf 'greet = "hi" / "hello"' --parse 'hi'`.

## CI

`.github/workflows/build.yml` runs on `ubuntu`/`windows`/`macos` ×
Node 24. It sets `core.autocrlf false` (CRLF would corrupt fixtures),
**clones the sibling closure** `parser debug json railroad` from
`github.com/tabnas`, builds them plus this repo in topo order
(`parser debug json abnf railroad`), then runs `npm test` in `abnf/ts`.
Packages are not published to npm, hence the sibling-checkout strategy.
