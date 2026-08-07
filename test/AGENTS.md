# Agents Guide — shared spec fixtures

`spec/*.tsv` holds the cross-runtime conformance fixtures. Both runtimes
run every fixture here, so a change affects both languages — edit with
that in mind. Same convention as
[`@tabnas/parser`'s `test/spec/`](https://github.com/tabnas/parser/tree/main/test).

These fixtures are what keeps `go/` honest against `ts/`. The older
`go/leftrec_test.go` and `go/rfc3986_test.go` mirror the TS suite by
hand, which catches nothing when only one side changes.

## Format

Tab-separated, one case per line, with a header row. Loaders unescape
`\n`, `\r`, `\r\n` in **every** column — ABNF grammars are multi-line, so
the `grammar` column depends on it.

Because this package compiles a grammar rather than parsing one fixed
language, the first column is the ABNF source under test, not the input.

| Fixture | Columns | `expected` |
|---|---|---|
| `alignment-abnf-ast` | `grammar`, `input`, `expected` | JSON of the parse result |
| `alignment-abnf-tokens` | `grammar`, `expected` | JSON of the emitted fixed-token map |
| `alignment-abnf-rules` | `grammar`, `expected` | JSON array of emitted rule names, sorted |
| `alignment-abnf-errors` | `grammar`, `expected` | `ERROR:<message>` — the exact message, byte for byte |
| `alignment-abnf-accept` | `grammar`, `expected`, `why` | `ACCEPT` or `REJECT` — does the compiler take this grammar at all |

`alignment-*` follows the parser repo's meaning: behaviours pinned
identical across TS and Go.

## Who runs what

- TypeScript: `ts/test/parity.test.js`.
- Go: `go/parity_test.go`.

Both resolve `../test/spec` (Go) / `../../test/spec` (TS) and assert the
same rows.

## Rules

- Prefer adding a fixture here over a one-off in-language assertion when
  a case is expressible as grammar → output. That is what keeps the two
  runtimes honest against each other.
- A new case must pass in BOTH runtimes: run `go test ./...` (from `go/`)
  and `npm test` (from `ts/`) before considering it done.
- `ts/` is canonical. Author expectations from the TS side, then make Go
  match — not the other way round. **`alignment-abnf-accept` is the one
  exception**: its expectations are the RFC 5234 / RFC 7405 answer, confirmed
  by a third-party ABNF parser (npm `abnf` 5.0.4 == hildjj/node-abnf), and
  several rows are currently RED in one or both runtimes on purpose. It is a
  measuring instrument, not a pass mark. Never edit a row to make a runtime
  green — fix the compiler, or leave it failing.
- Anything user-visible that lists a set must be **ordered**, not the
  result of a map walk. Go map iteration is randomised, so the
  prose-val error message uses `builtinTokenOrder` rather than sorting;
  `alignment-abnf-errors` pins the exact bytes and will catch a
  regression here.
- A grammar with no parseable input (e.g. `empty = ""`, which derives
  epsilon and cannot match at end-of-source) still belongs in the
  `tokens` and `rules` fixtures — just not in `ast`.
