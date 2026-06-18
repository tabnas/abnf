# Reference (Go)

Complete exported API and CLI flags for the Go port, package
`tabnasabnf` (import path `github.com/tabnas/abnf/go`). For an
introduction see [tutorial.md](tutorial.md); for recipes see
[guide.md](guide.md).

```go
import (
	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)
```

## Conversion

### `Abnf(src string, opts *AbnfConvertOptions) (*tabnas.GrammarSpec, error)`

Convert ABNF source to a tabnas `*GrammarSpec`. The primary entry point.
Pass `nil` for default options. Returns an `*AbnfParseError` for
malformed source or no productions.

### `ParseAbnf(src string) (*abnfGrammar, error)`

Parse ABNF source into the grammar AST without emitting a spec. The
`*abnfGrammar` type is package-internal; for external use prefer `Abnf`.

### `EmitGrammarSpec(grammar *abnfGrammar, opts *AbnfConvertOptions) (*tabnas.GrammarSpec, error)`

Convert an already-parsed grammar into a `*GrammarSpec`.

### `EliminateLeftRecursion(grammar *abnfGrammar) *abnfGrammar`

Rewrite direct and indirect left recursion via Paull's algorithm.
Exported to mirror the TS surface.

### `AbnfConvertOptions`

```go
type AbnfConvertOptions struct {
	Start    string // start rule; default = first production
	Tag      string // group tag on every alt; default "abnf"
	Builtins bool   // emit control + tree actions as $-builtin refs (pure data)
	Marks    bool   // stamp per-alt marks for @rule:o|c:mark actions
}
```

## Install (convert + install on an engine)

### `Install(j *tabnas.Tabnas, src string, opts *AbnfConvertOptions, actions ActionsMap) (*tabnas.GrammarSpec, error)`

Convert `src` and install the grammar on `j`, returning the spec. When
`actions` is non-nil, conversion runs in closure mode with `Marks: true`
and the actions are attached. Mirrors the TS `tn.abnf(src, opts)`
callable. Pass `nil` for `opts` and/or `actions` when unused.

### `Plugin(j *tabnas.Tabnas, _ map[string]any) error`

The tabnas Plugin form, provided for parity with the TS plugin shape; it
is a no-op installer (prefer `Abnf`/`Install` directly).

## User actions

### `ActionFn` and `ActionsMap`

```go
type ActionFn = tabnas.AltAction         // func(r *tabnas.Rule, ctx *tabnas.Context)
type ActionsMap map[string][]ActionFn
```

Each map value is a *slice* of actions (run in order). Action-ref grammar:

- `@<rule>:<phase>` — a rule-phase hook (`bo`, `ao`, `bc`, `ac`).
- `@<rule>:o:<mark>` — open alt(s) in `<rule>` carrying `<mark>`.
- `@<rule>:c:<mark>` — close alt(s) carrying `<mark>`.

### `AttachActions(spec *tabnas.GrammarSpec, actions ActionsMap) error`

Attach user semantic actions to a spec in place. Actions run *after* the
compiler's own action. The spec must be converted with `Marks: true` for
`o:`/`c:` refs. Returns `*AbnfActionError` for an unresolvable or
`$`-containing ref. Action keys are processed in sorted order for
determinism.

### `AttachActionSlots(spec *tabnas.GrammarSpec, refNames []string) error`

Declare named user-action slots on a pure-data spec without supplying
functions, to be bound at load time. Returns `*AbnfActionError` for a
rule-phase ref (slots are alt-action-only) or an unknown target.

### `MarkListing(spec *tabnas.GrammarSpec) string`

Newline-separated listing of compiler-assigned marks, one line per
marked alt: `<rule>  <o|c>:<mark>  <what>` (`<what>` is `s:<tokens>`,
`p:<rule>`, or `(empty)`). Requires `Marks: true`.

## Compilation (pure-data output)

### `AbnfCompile(src string, opts *AbnfCompileOptions) (string, error)`

Compile ABNF source into pure-data jsonic text (no closures). Always
converts internally with `Builtins: true` and `Marks: true`. Returns an
`*AbnfCompileError` if the grammar still needs control closures.

### `AbnfCompileOptions`

```go
type AbnfCompileOptions struct {
	Start       string
	Tag         string
	Strict      bool  // true: valid JSON; false: relaxed jsonic
	Indent      int   // default 2
	Recognition *bool // default true (recognition-only); false keeps tree builtins
}
```

`Recognition` is a `*bool` so the unset (`nil`) case defaults to `true`.

### `ToJsonic(value any, strict bool, indent int) string`

Serialise a function-free data value (`map[string]any` / `[]any` /
scalars / regex holders) as jsonic text. `strict` emits valid JSON;
otherwise relaxed jsonic. `indent` of 0 becomes 2.

### `SpecToData(spec *tabnas.GrammarSpec) map[string]any`

Convert a `*GrammarSpec` into a plain data tree
(`map`/`slice`/scalar/regex), emitting action refs as their `@`-name
strings and listing any closure ref names as `"@fn"`. Used by the CLI's
default spec-dump.

### `SpecToJSON(spec *tabnas.GrammarSpec, indent int) string`

Render a spec as JSON text (the CLI default output) — `ToJsonic` in
strict mode.

## Errors

```go
type AbnfParseError struct {
	Message string
	Line    int
	Column  int
	Cause   error // unwrappable
}

type AbnfCompileError struct {
	Message string
	Rules   []string // offending rule names
}

type AbnfActionError struct {
	Message string
}
```

All three implement `error`; `*AbnfParseError` also implements
`Unwrap() error`.

## CLI: `cmd/tabnas-abnf`

```
tabnas-abnf <args> [<abnf-source>]*
```

Reads ABNF from `--file` paths, inline arguments, and/or stdin (in that
order, concatenated). By default prints the emitted spec as JSON. Exit
code is non-zero on error or on a failed `--parse` sample.

| Flag | Alias | Argument | Effect |
|---|---|---|---|
| `-` | | | Read ABNF source from stdin. |
| `--file` | `-f` | `<path>` | Read source from `<path>` (repeatable). |
| `--start` | `-s` | `<name>` | Set the start rule (default: first production). |
| `--tag` | `-t` | `<name>` | Group tag on every emitted alt (default `abnf`). |
| `--compact` | `-c` | | Emit single-line JSON (default indent 2). |
| `--compile` | `-C` | | Emit a pure-data *recognition* grammar as jsonic text. |
| `--full` | `-F` | | With compilation, emit the full AST grammar (tree builtins retained). Implies `--compile`. |
| `--marks` | `-m` | | List the per-alt marks. |
| `--parse` | `-P` | `<input>` | Parse `<input>`; print the tree. Repeatable. Exits non-zero on any failure. |
| `--parse-file` | | `<path>` | Parse the contents of `<path>` (repeatable). |
| `--help` | `-h` | | Print help. |

```bash
go run ./cmd/tabnas-abnf 'greet = "hi" / "hello"'
go run ./cmd/tabnas-abnf -f grammar.abnf --parse 'hi'
go run ./cmd/tabnas-abnf -C 'greet = "hi"'
go run ./cmd/tabnas-abnf -m 'op = "inc" / "dec"'
```

## The ABNF dialect

Identical to the TS version: rule definitions `name = …`, incremental
`name =/ …`, alternation `/`, sequences, case-insensitive `"literals"`
(default) and case-sensitive `%s"…"` / explicit `%i"…"`, numeric values
(`%xNN`/`%dNN`/`%bNN`, ranges `-`, concatenation `.`), repetition
(`*A`, `1*A`, `m*nA`, `*nA`, `nA`), optional `[ … ]`, grouping `( … )`,
references, the RFC 5234 Appendix B.1 core rules (auto-included on
reference), and `;` line comments.
