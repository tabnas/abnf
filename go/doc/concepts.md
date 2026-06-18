# Concepts (Go)

How the Go port of `tabnasabnf` works and why. The design mirrors the
canonical TypeScript implementation in `../ts`; the conceptual model is
identical, and this document focuses on that model plus what differs in
Go. For the API see [reference.md](reference.md).

## What the compiler is, and what the engine is

`tabnasabnf` is a **compiler**, not a parser. `Abnf(src, opts)` reads
ABNF source and emits a tabnas `*GrammarSpec` — a declarative
description of rules, tokens, and AST-building actions. The actual
parsing is done by the **tabnas engine** (`github.com/tabnas/parser/go`),
a push-down recursive-descent parser:

```
ABNF source ──Abnf──▶ *GrammarSpec ──j.Grammar──▶ engine ──j.Parse──▶ AST
```

The compiler decides *what* grammar the engine runs; the engine decides
*whether an input matches* and *what tree to build*. That separation is
what lets a compiled grammar be serialised to data (via `AbnfCompile`)
and re-loaded on a bare engine — in Go, in TS, or in another process.

## The ABNF dialect: `=` and `/`

This is RFC 5234 ABNF: rules defined with `=`, alternatives separated by
`/`. Because `/` is alternation, regex-literal delimiters can't be used,
so character classes come from numeric values (`%x30-39`). Comments use
`;`. A bare `"GET"` is case-*insensitive* (it lowers to an `i`-flagged
match-token regex); `%s"GET"` is case-sensitive (a fixed token);
`%i"GET"` is the explicit default.

## The meta-grammar bootstrap

ABNF source is parsed by a tabnas instance whose grammar is itself
written as tabnas rules — the compiler bootstraps on the engine it
targets, with the JSON-oriented default tokens remapped to ABNF
operators. There is no separate hand-rolled ABNF lexer.

## The compilation pipeline

`emitGrammarSpec` runs, in order: merge `=/` incrementals; splice in
referenced RFC 5234 core rules; eliminate left recursion (Paull's,
direct and indirect, rewriting `P → P α | β` into `P → β (α)*`);
rewrite the ambiguous `[X D] Y` optional-prefix pattern into a probe +
phase-retry dispatcher; desugar EBNF sugar (`?`/`*`/`+`/`m*n`/grouping)
into plain helper rules; allocate tokens; compute FIRST sets; emit
tabnas rules.

## The output AST: `{rule, src, kids}`

Every rule emits a node — in Go a `map[string]any` with keys `rule`,
`src`, and `kids` (a `[]any` of child maps). Productions carry a
`nodeKind`:

- **user** — your rules, tagged `{rule, src, kids}`.
- **core** — RFC 5234 char-class bricks (`ALPHA`, `DIGIT`); their `src`
  flattens into the enclosing user rule, adding no child nodes.
- **helper** — synthetic desugar/dispatcher/chain rules; also flatten.

Because Paull's substitution inlines a *leading* reference, a rule whose
alternative begins with a reference to another user rule loses that
rule's own node. `p = "a" q` keeps `q` as a child; `p = q "a"` inlines
`q`. To keep a sub-rule visible, don't put its reference first.

## The probe dispatcher: unbounded lookahead

Some ABNF grammars aren't LL(k) for any bounded k — canonically RFC
3986's `[ userinfo "@" ] host`, where the optional prefix and the tail
share a character vocabulary and the disambiguating `@` may be
arbitrarily far away. For the pattern `[X D] Y`, the compiler emits a
probe that greedily consumes the joint vocabulary (minus `D`), peeks the
next token, commits to the `X D Y` or `Y` branch, rewinds to a saved
mark, and retries itself. It uses only ordinary engine primitives (`r:`
retry, `k:` config, `c:` guards, `ctx` mark/rewind/peek).

## Recognition vs. tree-building, and pure-data emission

A spec carries two kinds of behaviour: **recognition** (whether input
matches — fully structural, no functions) and **tree-building**
(constructing the AST — by default closures in `spec.Ref`).
`AbnfCompile` exploits the split to emit pure data:

- recognition mode (default) drops all tree-building, keeping the same
  accepted language but a generic tree;
- full mode (`Recognition = &false`) keeps tree-building as engine
  `$`-builtin refs (`@node$`, `@capture$`, `@bubble$`) plus `k` config —
  still pure data, rebuilding the exact tree on load.

The converter's `Builtins: true` emits both control and tree logic as
`$`-builtin refs instead of closures. A probe grammar converted
*without* `Builtins` has closure control logic and can't be pure
recognition data, so compilation refuses it (`*AbnfCompileError`).
`AbnfCompile` always converts with `Builtins: true`, avoiding that.

## User actions and marks

With `Marks: true`, every user-rule alternative is stamped with a
stable mark — its leading discriminator (token name, pushed-rule name,
or `_`), with `~N` suffixes for collisions. Bind a function to
`@<rule>:o:<mark>` / `@<rule>:c:<mark>` or a rule-phase hook
`@<rule>:<bo|ao|bc|ac>`. Your action is injected *after* the compiler's
tree action (the engine's array-`a` form), so `r.Node` already holds the
node map. `$` is reserved for engine builtins. `AttachActionSlots` is
the serialisable variant: named slots bound by the consumer at load.

## Differences from the TS version

The Go port tracks `../ts` (the canonical implementation) closely —
same pipeline, same fixtures, same parse trees — but a few things differ
because of Go's type system and idioms:

- **AST node type.** The tree is a `map[string]any` (`rule`, `src`,
  `kids []any`) rather than a JS object. Map keys print in alphabetical
  order (`map[kids:[] rule:greet src:hi]`); the data is the same.

- **Errors, not exceptions.** `Abnf`, `Install`, `AbnfCompile`,
  `AttachActions`, and `j.Parse` return an `error`. The error *types*
  (`*AbnfParseError`, `*AbnfCompileError`, `*AbnfActionError`) mirror
  the TS classes; `*AbnfParseError` implements `Unwrap`. The one case
  the TS code throws that Go *panics* on instead is a purely
  left-recursive rule with no seed (`a = a "x"`) — an internal
  invariant violation, not a user-recoverable error.

- **`ActionsMap` values are slices.** `map[string][]ActionFn` — each
  ref maps to a slice of actions — whereas TS accepts either a single
  function or an array. Action functions take `(*tabnas.Rule,
  *tabnas.Context)` (no third `alt` argument).

- **Compile-stage helpers are unexported.** TS exports
  `toRecognitionSpec` and `toPureSpec` directly; the Go port keeps the
  equivalents (`toRecognitionData` / `toPureData`) internal and exposes
  the higher-level `AbnfCompile`, plus `SpecToData` / `SpecToJSON` /
  `ToJsonic` for serialisation. Use `AbnfCompile` for pure-data output.

- **`ToJsonic` signature.** Positional `(value, strict bool, indent int)`
  rather than an options object, and it operates on the generic data
  tree (`map[string]any`/`[]any`/regex holders), since Go has no
  structural `GrammarSpec` literal to walk by reflection cheaply.

- **No callable instance member.** TS adds `tn.abnf(...)` /
  `tn.abnf.toSpec(...)` to the engine via the plugin. In Go you call the
  package functions `Abnf` / `Install` directly; `Plugin` exists only
  for shape parity and is a no-op.

- **Engine construction.** You build the engine with `tabnas.Make(...)`
  and may pass `tabnas.Options` (e.g. a larger rewind history for
  probe-heavy grammars, as the tests do) — there is no JS-style
  `new Tabnas({ plugins: [...] })` constructor.

## Design trade-offs

These match the TS implementation: bootstrapping on the engine keeps the
ABNF parser tiny but ties it to tabnas; Paull's substitution always runs
(needed to populate lookahead token columns) at the cost of inlining
leading-ref nodes; pure-data emission trades a little expressiveness
(probe grammars need the `$`-builtins on the loading engine) for
portability; and the probe dispatcher covers terminal-disambiguated
optional-prefix ambiguity but not non-terminal disambiguators.
