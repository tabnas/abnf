# @tabnas/abnf

ABNF grammar compiler for the
[`tabnas`](https://github.com/rjrodger/tabnas) parser.

Takes ABNF source (the RFC 5234 dialect — `=` and `/`, not `::=`) and
emits a tabnas `GrammarSpec`. Installed on an engine, the spec parses
inputs in that grammar and builds a `{rule, src, kids}` AST. It can also
emit "pure-data" jsonic and supports user actions. Ships the
`tabnas-abnf` CLI.

## Install

```bash
npm install @tabnas/parser @tabnas/abnf
```

## Use

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)

tn.parse('hi') // => ({ rule: 'greet', src: 'hi', kids: [] })
```

## Left recursion

Left-recursive rules are accepted directly. A left-recursion pass
(Paull's algorithm) rewrites direct (`P = P a / b`) and indirect
recursion into `P = b *(a)`, which the push-down engine runs without
re-entering a rule at the same position:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  expr = expr PL term / term
  term = NR
  PL   = "+"
`)

tn.parse('1+2+3').kids.map((k) => k.rule) // => ['PL', 'term', 'PL', 'term']
```

Because it is a rewrite, the tree is **flat** (no nested `expr`; the
leading operand folds into the rule, so associativity is applied in an
action, not read off the AST), and `@ref` alt actions on the rewritten
branches are look-up-only — attach actions to the sub-rules instead. A
**purely** left-recursive rule (no non-recursive branch) is an error.
See [concepts.md](doc/concepts.md) and the root
[README](../README.md#left-recursion) for the full details and caveats.

## Documentation

Four-quadrant [Diátaxis](https://diataxis.fr) docs:

- [tutorial.md](doc/tutorial.md) — learning-oriented: zero to a working
  parser, step by step.
- [guide.md](doc/guide.md) — task-oriented recipes for real problems.
- [reference.md](doc/reference.md) — the exact API surface and CLI flags.
- [concepts.md](doc/concepts.md) — how the compiler works and why.

## CLI

```bash
tabnas-abnf -f grammar.abnf
tabnas-abnf 'g = "a"' --parse 'a'
tabnas-abnf -C 'greet = "hi"'   # compile to pure-data jsonic
```

## License

MIT. Copyright (c) Richard Rodger.
