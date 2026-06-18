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
