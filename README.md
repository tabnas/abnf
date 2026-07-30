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

**Why you would want this.** The tabnas engine ships no grammar of its
own — you bring one, normally as a hand-written table of `open`/`close`
rule alternatives. That table is precise but verbose, and it is not the
notation the specification you are implementing is written in. RFC
specifications, from URIs to media types to protocol headers, publish
their grammars in ABNF. This package lets you paste that ABNF in and get
a working parser, instead of transcribing it by hand.

```bash
npm install @tabnas/parser @tabnas/abnf
```

## A first grammar

A grammar is a set of **productions**, `name = definition`. Alternatives
are separated by `/`, and terminals are quoted strings.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)

tn.parse('hi') // => ({ rule: 'greet', src: 'hi', kids: [] })
```

Every rule that matches produces one AST node with three fields:

- **`rule`** — the production's name, so you can navigate the tree by the
  names you wrote.
- **`src`** — the source text the rule matched.
- **`kids`** — child nodes, one per sub-rule the production referenced.

`greet` matches a single terminal, so it has no children.

## Sequences and sub-rules

Write elements one after another to match them in order, and reference
another production by its bare name to nest it:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  greeting = "hello" name
  name     = 1*ALPHA
`)

const out = tn.parse('hello world')
out.kids.map((k) => k.rule) // => ['name']
out.kids[0].src             // => 'world'
```

`1*ALPHA` is "one or more letters". `ALPHA` is one of the RFC 5234
Appendix B.1 **core rules** (`ALPHA`, `DIGIT`, `HEXDIG`, `CRLF`, `WSP`,
…), which are spliced in automatically whenever a grammar references one
without defining it.

Note that `out.src` is `'helloworld'`, not `'hello world'`: the lexer
skips whitespace between tokens, so `src` is the concatenation of what
was matched, not a slice of the original input.

## Repetition, optionals, and groups

The usual ABNF operators all work — `*x` (zero or more), `1*x` (one or
more), `m*nx` (bounded), `[ x ]` (optional), and `( … )` for grouping:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`csv = NR *( "," NR )`)

tn.parse('1,2,3').src // => '1,2,3'
tn.parse('7').src     // => '7'
```

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`flag = "log" [ "=" TX ]`)

tn.parse('log=debug').src // => 'log=debug'
tn.parse('log').src       // => 'log'
```

## Terminals: literals, built-in tokens, and prose

There are three ways to say "a terminal goes here".

**A quoted literal** is matched verbatim — case-insensitively, per RFC
5234, unless you write `%s"…"`. A production whose *whole* body is a
single literal is a lexical definition rather than a rule, so it compiles
to a named lexer token instead of a rule (`PL = "+"` becomes `#PL`).

**A built-in lexer token** — `TX` (bareword), `NR` (number), `ST`
(quoted string), `VL` (`true`/`false`/`null`) — matches whole tokens the
engine's lexer already produces. Prefer these over deriving text
character by character: because whitespace between tokens is skipped,
a char-level `1*ALPHA` would happily run two space-separated words
together, while `TX` will not.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  pair = "{" key ":" val "}"
  key  = TX
  val  = NR / ST
`)

tn.parse('{a:1}').kids.map((k) => k.rule)     // => ['key', 'val']
tn.parse('{a:"x"}').kids[1].src               // => '"x"'
```

**Prose** (`<free text>`) is RFC 5234's escape hatch for describing a
terminal in English. It defines nothing, so it is accepted in exactly one
place: as the whole body of a production naming a built-in token, where
it documents what the lexer already provides.

```
NR = <number>     ; informational — compiles to nothing
```

## The same grammar, two ways

This is the integer-addition grammar from the
[engine README](https://github.com/tabnas/parser#readme), written in ABNF:

```
val = add
add = NR [ PL add ]

NR = <number>
PL = "+"
```

`NR` is the engine's built-in number token and `PL` is `"+"`. Two things
are worth noticing, because they are what make this the *same* grammar
the engine README builds by hand:

- **`PL = "+"` compiles to a token, not a rule.** A production whose
  whole body is a single string literal is a lexical definition, so the
  compiler binds it to a named fixed token `#PL` — exactly what the
  hand-written grammar spells `fixed: { token: { '#PL': '+' } }`.
  Multi-alternative productions (`sign = "+" / "-"`) are real choices and
  stay rules.
- **`NR = <number>` is informational.** RFC 5234 `prose-val` describes a
  terminal in English rather than defining one. For a built-in lexer
  token that is exactly right — the lexer already supplies `NR` — so the
  line documents the terminal and compiles to nothing. Prose anywhere
  else is an error, because there would be no definition behind it.

Because both sides agree, the grammar round-trips: compile it, and
[`@tabnas/debug`](https://github.com/tabnas/debug) renders the running
engine back to the ABNF it was written in.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')
const { Debug } = require('@tabnas/debug')

const GRAMMAR = `val = add
add = NR [ PL add ]

NR = <number>
PL = "+"`

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(GRAMMAR)
tn.use(Debug, { print: false })

// Rendered back out, character for character — and re-compilable.
tn.debug.model().abnf === GRAMMAR // => true
```

Add an action to fold the operands into a running total on `val`, the
same way the hand-written grammar accumulates into its `val` node:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  val = add
  add = NR [ PL add ]
  NR  = <number>
  PL  = "+"
`, {
  actions: {
    // Add every number to the one `val` node — no child integration.
    '@add:o:NR': (r) => {
      let val = r
      while (val.parent && 'val' !== val.name) val = val.parent
      val.node.value = (val.node.value || 0) + Number(r.o[0].val)
    },
  },
})

tn.parse('1+2+3').value   // => 6
tn.parse('12+3+45').value // => 60
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
tn.parse('1+2+3').kids.map((k) => k.rule) // => ['term', 'term']
```

(`PL = "+"` is a single-literal production, so it compiles to the token
`#PL` rather than a rule — which is why the operators do not appear among
the children. Only `term` does.)

### Details and caveats

- **It is a rewrite, not native left-recursive parsing.** `expr = expr PL
  term / term` is compiled as `expr = term *(PL term)`. Two consequences
  follow from that:
  - **The tree is flat, not left-nested.** There is no `expr` nested inside
    `expr`. The repeated `(PL term)` pairs become direct children, and the
    *leading* operand (`1` above) is folded into `expr` itself rather than
    surfacing as its own `term` child — so `1+2+3` yields
    `['term','term']`, and a lone `1` parses to an `expr` with no
    children at all. Left-associativity is a fact you apply in an action,
    not a shape you read off the AST.
  - **The rewritten branches are look-up-only for `@ref` actions.** The
    source `P a` / `b` alternatives do not survive as distinct marks, so an
    alt-mark action cannot reliably attach to them. Hang actions on the
    sub-rules (`term`) instead, or fold a running value as you would
    for any iterative (`*(...)`) rule.
- **A rewritten rule does not round-trip.** The `*(…)` iteration compiles
  to a probe-optimised subgraph that `@tabnas/debug` cannot reconstruct,
  so rendering the live grammar back to ABNF yields a
  recognition-equivalent grammar spelled in terms of the generated helper
  rules, not the left-recursive source. Grammars written in the iterative
  form (like the addition grammar above) round-trip exactly.
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

That claim is enforced, not asserted: [`test/spec/*.tsv`](test/spec/)
holds cross-runtime conformance fixtures pinning, for each grammar, the
tokens allocated, the rule names emitted, the AST a sample input parses
to, and the exact message for each rejected grammar. `ts/test/parity.test.js`
and `go/parity_test.go` run the *same* files, so neither runtime can drift
without going red. See [`test/AGENTS.md`](test/AGENTS.md).

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
