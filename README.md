# @tabnas/abnf

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
