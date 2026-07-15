# @tabnas/abnf

<!-- tabnas-badges -->
[![npm](https://tabnas.github.io/status/badges/abnf-npm.svg)](https://www.npmjs.com/package/@tabnas/abnf)
[![CI](https://github.com/tabnas/abnf/actions/workflows/ci.yml/badge.svg)](https://github.com/tabnas/abnf/actions/workflows/ci.yml)
[![go](https://tabnas.github.io/status/badges/abnf-go.svg)](https://pkg.go.dev/github.com/tabnas/abnf/go)
[![tabnas standard](https://tabnas.github.io/status/badges/abnf-standard.svg)](https://tabnas.github.io/status/)
<!-- /tabnas-badges -->

ABNF grammar compiler for the
[tabnas](https://github.com/rjrodger/tabnas) parser. Takes ABNF source
— the RFC 5234 dialect (`=` and `/`, not `::=`) — and emits a tabnas
`GrammarSpec`. Installed on an engine, the spec parses inputs in that
grammar and builds a `{rule, src, kids}` AST. It can also emit
"pure-data" jsonic and supports user actions. Ships the `tabnas-abnf`
CLI.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)
tn.parse('hi') // => ({ rule: 'greet', src: 'hi', kids: [] })
```

## Left recursion

ABNF grammars are often clearest written left-recursively — an additive
expression is "an expression, a `+`, then a term". The compiler accepts
that directly: a left-recursion pass (Paull's algorithm) rewrites both
**direct** (`P = P a / b`) and **indirect** (`P = Q a`, `Q = P b`)
recursion into the iterative form the push-down engine can run without
re-entering a rule at the same source position:

```
P = P a / b     →     P = b *(a)
```

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  expr = expr PL term / term
  term = NR
  PL   = "+"
`)

// The left-recursive `expr` parses a whole additive chain, left to right:
tn.parse('1+2+3').rule                    // => 'expr'
tn.parse('1+2+3').kids.map((k) => k.rule) // => ['PL', 'term', 'PL', 'term']
```

### Details and caveats

- **It is a rewrite, not native left-recursive parsing.** `expr = expr PL
  term / term` is compiled as `expr = term *(PL term)`. Two consequences
  follow from that:
  - **The tree is flat, not left-nested.** There is no `expr` nested inside
    `expr`. The repeated `(PL term)` pairs become direct children, and the
    *leading* operand (`1` above) is folded into `expr` itself rather than
    surfacing as its own `term` child — so `1+2+3` yields
    `['PL','term','PL','term']`, and a lone `1` parses to an `expr` with no
    children at all. Left-associativity is a fact you apply in an action,
    not a shape you read off the AST.
  - **The rewritten branches are look-up-only for `@ref` actions.** The
    source `P a` / `b` alternatives do not survive as distinct marks, so an
    alt-mark action cannot reliably attach to them. Hang actions on the
    sub-rules (`term`, `PL`) instead, or fold a running value as you would
    for any iterative (`*(...)`) rule.
- **A purely left-recursive rule is an error.** `loop = loop PL`, with no
  base (seed) alternative, throws `abnf: rule 'loop' is purely
  left-recursive (no seed alternative); cannot eliminate` — there is
  nothing to anchor the iteration on. Always give the recursive rule a
  non-recursive branch (`/ term`).
- **Indirect recursion works, but can enlarge the grammar.** Paull's
  algorithm inlines earlier rules' alternatives to expose hidden recursion,
  which can duplicate branches; pathological grammars grow. This is a
  first-step converter, not a full grammar toolchain — keep grammars
  reasonably small.

This repository contains two implementations. `ts/` is canonical; `go/`
tracks it. Both compile the same `.abnf` fixtures (in
`ts/test/grammar/`) and produce the same parse trees.

| Path | Description |
|---|---|
| [`ts/`](ts/) | TypeScript / JavaScript (`@tabnas/abnf`) + the `tabnas-abnf` CLI. |
| [`go/`](go/) | Go port (`github.com/tabnas/abnf/go`, package `tabnasabnf`) + CLI. |

## Documentation

Four-quadrant [Diátaxis](https://diataxis.fr) docs in each language:

| | TypeScript | Go |
|---|---|---|
| Tutorial (learn) | [ts/doc/tutorial.md](ts/doc/tutorial.md) | [go/doc/tutorial.md](go/doc/tutorial.md) |
| Guide (tasks) | [ts/doc/guide.md](ts/doc/guide.md) | [go/doc/guide.md](go/doc/guide.md) |
| Reference (API/CLI) | [ts/doc/reference.md](ts/doc/reference.md) | [go/doc/reference.md](go/doc/reference.md) |
| Concepts (why) | [ts/doc/concepts.md](ts/doc/concepts.md) | [go/doc/concepts.md](go/doc/concepts.md) |

See [`ts/README.md`](ts/README.md) and [`go/README.md`](go/README.md)
for per-language orientation.

## License

MIT. Copyright (c) Richard Rodger.
