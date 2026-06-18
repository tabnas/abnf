# github.com/tabnas/abnf/go

Go port of [`@tabnas/bnf`](../ts) — a BNF / ABNF grammar compiler for the
[`tabnas`](https://github.com/tabnas/parser) parsing engine.

Takes BNF or ABNF source and emits a tabnas `GrammarSpec` that, installed
on an engine, parses inputs in that grammar and builds a `{rule, src,
kids}` AST. Also emits "pure-data" jsonic (recognition / full-AST specs),
supports user actions, and ships a CLI (`tabnas-bnf`).

The Go package tracks the canonical TypeScript implementation in `../ts`;
both compile the SAME `.bnf`/`.abnf` fixtures (in `../ts/test/grammar/`)
and produce the same parse output.

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
	spec, _ := abnf.Bnf(`greet = "hi" / "hello"`, nil)
	j := tabnas.Make()
	j.Grammar(spec)
	out, _ := j.Parse("hi")
	fmt.Println(out) // map[rule:greet src:hi kids:[]]
}
```

Convert + install in one step (optionally attaching user actions):

```go
j := tabnas.Make()
abnf.Install(j, `greet = "hi"`, nil, nil)
```

Compile to a function-free, pure-data grammar (jsonic text):

```go
text, _ := abnf.BnfCompile(`greet = "hi" / "hello"`, nil)
```

## Exports

- `Bnf(src, *BnfConvertOptions) (*GrammarSpec, error)` — convert source.
- `ParseBnf`, `EmitGrammarSpec`, `EliminateLeftRecursion` — pipeline stages.
- `Install(j, src, opts, actions)` — convert + install on an engine.
- `BnfCompile`, `ToJsonic`, `AttachActions`, `AttachActionSlots`,
  `MarkListing` — compilation / action / serialisation surface.
- Errors: `BnfParseError`, `BnfCompileError`, `BnfActionError`.

## CLI

```bash
go run ./cmd/tabnas-bnf -f grammar.bnf
go run ./cmd/tabnas-bnf 'greet = "hi" / "hello"' --parse 'hi'
go run ./cmd/tabnas-bnf -C 'greet = "hi"'   # compile to pure-data jsonic
go run ./cmd/tabnas-bnf -m 'op = "inc" / "dec"'  # list action marks
```

## License

MIT.
