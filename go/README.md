# github.com/tabnas/abnf/go

Go port of [`@tabnas/abnf`](../ts) — an ABNF grammar compiler for the
[`tabnas`](https://github.com/tabnas/parser) parsing engine.

Takes ABNF source — the RFC 5234 dialect (`=` and `/`, not `::=`) — and
emits a tabnas `GrammarSpec` that, installed on an engine, parses inputs
in that grammar and builds a `{rule, src, kids}` AST. Also emits
"pure-data" jsonic (recognition / full-AST specs), supports user
actions, and ships a CLI (`tabnas-abnf`).

The Go package tracks the canonical TypeScript implementation in `../ts`;
both compile the same `.abnf` fixtures (in `../ts/test/grammar/`) and
produce the same parse output.

## Install

```bash
go get github.com/tabnas/abnf/go
```

## Use

```go
package main

import (
	"fmt"

	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)

func main() {
	spec, _ := abnf.Abnf(`greet = "hi" / "hello"`, nil)
	j := tabnas.Make()
	j.Grammar(spec)
	out, _ := j.Parse("hi")
	fmt.Println(out) // map[kids:[] rule:greet src:hi]
}
```

## Documentation

Four-quadrant [Diátaxis](https://diataxis.fr) docs:

- [tutorial.md](doc/tutorial.md) — learning-oriented: zero to a working
  parser, step by step.
- [guide.md](doc/guide.md) — task-oriented recipes for real problems.
- [reference.md](doc/reference.md) — the exact exported API and CLI flags.
- [concepts.md](doc/concepts.md) — how it works, plus differences from
  the TS version.

## CLI

```bash
go run ./cmd/tabnas-abnf -f grammar.abnf
go run ./cmd/tabnas-abnf 'greet = "hi" / "hello"' --parse 'hi'
go run ./cmd/tabnas-abnf -C 'greet = "hi"'   # compile to pure-data jsonic
go run ./cmd/tabnas-abnf -m 'op = "inc" / "dec"'  # list action marks
```

## License

MIT.
