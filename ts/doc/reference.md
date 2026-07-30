# Reference

Complete API surface and CLI flags for `@tabnas/abnf`. For an
introduction see [tutorial.md](tutorial.md); for usage recipes see
[guide.md](guide.md).

All exports come from the package root:

```js
const {
  abnf, abnfConvert, parseAbnf, emitGrammarSpec, eliminateLeftRecursion,
  abnfRules, AbnfParseError,
  abnfCompile, toRecognitionSpec, toPureSpec, toJsonic,
  attachActions, attachActionSlots, markListing,
  AbnfCompileError, AbnfActionError,
} = require('@tabnas/abnf')
```

## Conversion

### `abnfConvert(src, opts?) => GrammarSpec`

Take ABNF source and return a tabnas `GrammarSpec` (with a `ref` map of
action closures, an `options` block, and a `rule` table). This is the
primary entry point. (Also exported as `abnf` — the bare function, not
to be confused with the plugin of the same name below.)

- `src: string` — the ABNF source.
- `opts?: AbnfConvertOptions` — see below.

### `parseAbnf(src) => AbnfGrammar`

Parse ABNF source into the grammar AST (`{ productions: [...] }`)
*without* emitting a spec. Each production is `{ name, alts }`, where
`alts` is a list of sequences of `AbnfElement`s (`{kind:'term',
literal}`, `{kind:'ref', name}`, `{kind:'opt'|'star'|'plus', inner}`,
etc.). Useful for inspecting or transforming a grammar. Throws
`AbnfParseError` for malformed source or no productions.

### `emitGrammarSpec(grammar, opts?) => GrammarSpec`

Convert an already-parsed `AbnfGrammar` (from `parseAbnf`) into a
`GrammarSpec`. `abnfConvert(src)` is `emitGrammarSpec(parseAbnf(src))`.

### `eliminateLeftRecursion(grammar) => AbnfGrammar`

Rewrite direct and indirect left recursion via Paull's algorithm,
returning a new grammar. Called internally by `emitGrammarSpec`;
exported for inspection.

### `abnfRules`

The declarative table of tabnas rules that defines the ABNF grammar
itself (the meta-grammar used to parse ABNF source). Exported for
introspection and tooling.

### `AbnfConvertOptions`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `start` | `string` | first production | The user-visible start rule. |
| `tag` | `string` | `'abnf'` | Group tag (`g`) stamped on every emitted alt. |
| `builtins` | `boolean` | `false` | Emit control + tree actions as engine `$`-builtin refs + `k` config instead of closures, so the spec survives compilation to pure data. Requires an engine that ships the probe `$`-builtins. |
| `marks` | `boolean` | `false` | Stamp a stable `m` (mark) on each user-rule alt, enabling `@<rule>:o\|c:<mark>` action refs. Off by default; the emitted spec shape is unchanged unless requested. |

## Plugin form

### `abnf` (tabnas Plugin)

Install with `new Tabnas({ plugins: [abnf] })` (or `tn.use(abnf)`). It
decorates the instance with a callable `tn.abnf` member:

#### `tn.abnf(src, opts?) => GrammarSpec`

Convert `src`, install the grammar on this instance, and return the
spec. `opts` is `AbnfConvertOptions` plus an optional
`actions?: ActionsMap`. When `actions` are supplied, conversion runs in
closure mode with `marks: true` and the actions are attached
automatically.

#### `tn.abnf.toSpec(src, opts?) => GrammarSpec`

Build the spec **without** installing it. Equivalent to `abnfConvert`,
but reachable from the instance.

## User actions

### `attachActions(spec, actions) => spec`

Attach user semantic actions to a spec in place (and return it). Keys
are action refs; values are a function or array of functions, run
*after* the compiler's own action, in attachment order. Works in both
closure and `builtins` mode. The spec must have been converted with
`marks: true` for `o:`/`c:` refs to resolve. Throws `AbnfActionError`
for a ref that matches no rule, hook, or marked alt, or that contains
`$` (reserved for engine builtins).

Action-ref grammar:

- `@<rule>:<phase>` — a rule-phase hook, where `<phase>` is `bo`, `ao`,
  `bc`, or `ac` (before/after open/close).
- `@<rule>:o:<mark>` — the open alt(s) in `<rule>` carrying `<mark>`.
- `@<rule>:c:<mark>` — the close alt(s) carrying `<mark>`.

### `attachActionSlots(spec, refNames) => spec`

Declare user-action *slots* on a pure-data spec without supplying
functions: each `@<rule>:o|c:<mark>` name is injected into the matched
alt's action list, to be bound at load time from a consumer-supplied
`ref` map. Throws `AbnfActionError` for a rule-phase ref (slots are for
alt actions only) or an unknown target.

### `markListing(spec) => string`

Human-readable, newline-separated listing of the marks the compiler
assigned — one line per marked alt: `<rule>  <o|c>:<mark>  <what>`,
where `<what>` is `s:<tokens>`, `p:<rule>`, or `(empty)`. The spec must
have been converted with `marks: true`.

### `ActionsMap`

`Record<string, ActionFn | ActionFn[]>` where
`ActionFn = (r, ctx, alt) => any`.

## Compilation (pure-data output)

### `abnfCompile(src, opts?) => string`

Compile ABNF source into a pure-data tabnas grammar as **jsonic text**
(no closures). Always converts internally with `builtins: true` and
`marks: true`.

`opts: AbnfCompileOptions` = `AbnfConvertOptions` plus:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `recognition` | `boolean` | `true` | `true`: emit a recognition-only grammar (tree building dropped). `false`: keep the AST-building `$`-builtins (still pure data; builds `{rule,src,kids}`). |
| `strict` | `boolean` | `false` | `true`: valid JSON (double quotes, commas). `false`: jsonic (bare keys, single quotes). |
| `indent` | `number` | `2` | Indentation width. |

### `toRecognitionSpec(spec) => GrammarSpec`

Strip a converted spec to a function-free *recognition* grammar (tree
building dropped, `ref` map removed, `v` schema version stamped).
Throws `AbnfCompileError` (with `.rules` listing offenders) for a
grammar whose control logic is still closures (a probe dispatcher
converted without `builtins: true`).

### `toPureSpec(spec) => GrammarSpec`

Reduce a spec to pure data that *keeps* the AST-building `$`-builtins
(so the deserialized grammar still builds the full tree). Requires
`builtins: true` conversion; throws `AbnfCompileError` if any closures
remain in `spec.ref`.

### `toJsonic(value, opts?) => string`

Serialise a function-free value as jsonic text. `opts:
{ strict?: boolean; indent?: number }`. RegExp instances serialise as
`@/source/flags` (or `@~/…/` to carry the eager-matcher flag), which the
engine's `resolveFuncRefs` reconstructs on load.

## Errors

| Class | Thrown by | Notable fields |
|---|---|---|
| `AbnfParseError` | `parseAbnf` / `abnfConvert` on bad source | `line?`, `column?`, `cause?` |
| `AbnfCompileError` | `toRecognitionSpec` / `toPureSpec` / `abnfCompile` | `rules: string[]` (offending rule names) |
| `AbnfActionError` | `attachActions` / `attachActionSlots` | — |

## CLI: `tabnas-abnf`

```
tabnas-abnf <args> [<abnf-source>]*
```

Reads ABNF from `--file` paths, inline arguments, and/or stdin (in that
order, concatenated). By default prints the emitted `GrammarSpec` as
JSON.

| Flag | Alias | Argument | Effect |
|---|---|---|---|
| `-` | | | Read ABNF source from stdin. |
| `--file` | `-f` | `<path>` | Read source from `<path>` (repeatable). |
| `--start` | `-s` | `<name>` | Set the start rule (default: first production). |
| `--tag` | `-t` | `<name>` | Group tag on every emitted alt (default `abnf`). |
| `--compact` | `-c` | | Emit single-line JSON (default indent 2). |
| `--compile` | `-C` | | Emit a pure-data *recognition* grammar as jsonic text. |
| `--full` | `-F` | | With compilation, emit the full AST grammar (tree `$`-builtins retained). Implies `--compile`. |
| `--marks` | `-m` | | List the per-alt marks (for `@<rule>:o\|c:<mark>` refs). |
| `--parse` | `-P` | `<input>` | Parse `<input>` against the grammar; print the tree. Repeatable. Exits non-zero if any sample fails. |
| `--parse-file` | | `<path>` | Parse the contents of `<path>` (repeatable). |
| `--help` | `-h` | | Print help. |

Examples:

```bash
tabnas-abnf 'greet = "hi" / "hello"'
tabnas-abnf -f grammar.abnf
echo 'g = "a"' | tabnas-abnf -
tabnas-abnf -f grammar.abnf --parse 'hi'
tabnas-abnf -C 'greet = "hi"'           # compile to pure-data jsonic
tabnas-abnf -m 'op = "inc" / "dec"'     # list action marks
```

## The ABNF dialect

Supported: rule definitions `name = …`, incremental `name =/ …`,
alternation `/`, sequences, case-insensitive `"literals"` (default) and
case-sensitive `%s"…"` / explicit `%i"…"`, numeric values
(`%xNN`/`%dNN`/`%bNN`, ranges `-`, concatenation `.`), repetition
(`*A`, `1*A`, `m*nA`, `*nA`, `nA`), optional `[ … ]`, grouping
`( … )`, references (bare names), the RFC 5234 Appendix B.1 core rules
(auto-included on reference), prose-val `< … >` for built-in lexer tokens,
and `;` line comments. Quoted-string case folding follows RFC 5234.

Out of scope: ABNF features beyond the above.

### Prose-val

RFC 5234 `prose-val` (`< … >`) describes a terminal in English rather
than defining one, so it is accepted in exactly one position: as the
entire body of a production naming a built-in lexer token.

```
NR = <number>
```

The line documents that `NR` is the engine's number token; the lexer
already supplies it, so the production compiles to nothing and every `NR`
reference binds to `#NR`. The recognised names are `TX`, `NR`, `ST` and
`VL`. Prose for any other name, or prose used inside a larger
expression, is an error — there would be no definition behind it.

### Single-literal productions

A production whose whole body is one string literal is a *lexical*
definition, and compiles to a named fixed token rather than a rule:

```
PL = "+"        ; => fixed token #PL, no `PL` rule
```

The production name becomes the token name, so the grammar renders back
out as `PL = "+"` rather than as a rule wrapping an anonymously-named
token. Four cases are deliberately excluded, and stay rules: the start
rule (the grammar needs an entry point), multi-alternative productions
(`sign = "+" / "-"` is a choice), names the engine already owns (`TX`,
`NR`, `ZZ`, `OB`, … — binding one would displace a lexer matcher), and
the empty literal (`path-empty = ""` derives epsilon, which no token can
match).

Because tokens are not AST nodes, a lifted production does not appear in
the parse tree. Give it a second element (`q = "b" "c"`) to keep it a
rule.
