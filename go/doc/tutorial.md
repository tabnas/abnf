# Tutorial: your first ABNF grammar (Go)

This is a learning-oriented walkthrough for the Go port,
`tabnasabnf` (import path `github.com/tabnas/abnf/go`). By the end you
will have taken an [ABNF](https://datatracker.ietf.org/doc/html/rfc5234)
grammar from a string of text to a working parser that builds a tree
from your input.

`tabnasabnf` is a **compiler**: it reads ABNF source and emits a tabnas
`GrammarSpec`. You install that spec on a tabnas engine, and the engine
parses inputs in your grammar and hands you back an AST.

> Dialect note: this ABNF uses `=` to define a rule and `/` to separate
> alternatives — *not* `::=` or `|`. That is genuine RFC 5234 ABNF.

The Go port tracks the canonical TypeScript implementation in `../ts`;
both compile the same `.abnf` fixtures and produce the same parse trees.

## Step 0: get the package

```bash
go get github.com/tabnas/abnf/go
go get github.com/tabnas/parser/go
```

The parser is the engine; abnf is this compiler. You need both.

## Step 1: convert, install, parse

`Abnf(src, opts)` converts ABNF source to a `*GrammarSpec`. Install it
on an engine with `j.Grammar(spec)`, then `j.Parse(input)`.

```go
package main

import (
	"fmt"

	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)

func main() {
	spec, err := abnf.Abnf(`greet = "hi" / "hello"`, nil)
	if err != nil {
		panic(err)
	}
	j := tabnas.Make()
	j.Grammar(spec)

	out, _ := j.Parse("hi")
	fmt.Println(out) // map[kids:[] rule:greet src:hi]
}
```

`greet = "hi" / "hello"` reads: *the rule `greet` matches the literal
`hi` or the literal `hello`.* The result is a node map.

## Step 2: the tree shape

Every rule produces a node, returned as a `map[string]any` with three
keys:

- `rule` — the grammar rule's name.
- `src` — the source text this rule matched.
- `kids` — a `[]any` of child node maps, one per *referenced* sub-rule.

A leaf rule that matched only a literal has an empty `kids`, as above.

## Step 3: install in one step

`Install(j, src, opts, actions)` converts and installs in a single
call. Pass `nil` for both `opts` and `actions` when you don't need them.

```go
package main

import (
	"fmt"

	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)

func main() {
	j := tabnas.Make()
	abnf.Install(j, "p = \"a\" q\nq = \"b\"", nil, nil)

	out, _ := j.Parse("a b")
	fmt.Println(out) // map[kids:[map[kids:[] rule:q src:b]] rule:p src:ab]
}
```

Here `p = "a" q` matches `"a"` then a `q`. `q` appears as a child of
`p` because it's *referenced* and not at the leading position of the
alternative.

## Step 4: repetition, grouping, and core rules

ABNF gives you repetition prefixes (`1*X` is one-or-more) and
parentheses for grouping. The RFC 5234 core rules (`ALPHA`, `DIGIT`, …)
are auto-included whenever you reference them.

```go
package main

import (
	"fmt"

	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)

func main() {
	j := tabnas.Make()
	abnf.Install(j, "list = \"[\" item *(\",\" item) \"]\"\nitem = 1*ALPHA", nil, nil)

	out, _ := j.Parse("[a,b,c]")
	fmt.Println(out)
	// map[kids:[map[kids:[] rule:item src:a] map[kids:[] rule:item src:b] map[kids:[] rule:item src:c]] rule:list src:[a,b,c]]
}
```

`*("," item)` is "zero or more of (`,` then `item`)". Each `item`
becomes a child of `list`.

## Step 5: handle the error returns

Both `Abnf` and `Parse` return an `error`. Bad ABNF source surfaces as
an `*AbnfParseError`:

```go
package main

import (
	"fmt"

	abnf "github.com/tabnas/abnf/go"
)

func main() {
	_, err := abnf.Abnf(`g = missing`, nil) // references an undefined rule
	fmt.Println(err != nil) // true
}
```

## Where to go next

- **[guide.md](guide.md)** — recipes: left recursion, case sensitivity,
  user actions, compiling to pure data.
- **[reference.md](reference.md)** — the exact Go API and CLI flags.
- **[concepts.md](concepts.md)** — how it works, plus differences from
  the TS version.
