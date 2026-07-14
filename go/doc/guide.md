# How-to guide (Go)

Focused recipes for the Go port (`github.com/tabnas/abnf/go`, package
`tabnasabnf`). Each is independent. For the full API see
[reference.md](reference.md); for the "why" see [concepts.md](concepts.md).

All examples import:

```go
import (
	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)
```

## Convert without installing

`Abnf` returns the `*GrammarSpec`; install it later with `j.Grammar`.

```go
spec, err := abnf.Abnf(`pair = "a" "b"`, nil)
if err != nil {
	return err
}
j := tabnas.Make()
j.Grammar(spec)
out, _ := j.Parse("ab") // map[kids:[] rule:pair src:ab]
```

## Choose a different start rule

By default the first production is the start rule. Override it via
`AbnfConvertOptions.Start`:

```go
spec, _ := abnf.Abnf("a = \"x\"\nb = \"y\"", &abnf.AbnfConvertOptions{Start: "b"})
j := tabnas.Make()
j.Grammar(spec)
out, _ := j.Parse("y") // map[kids:[] rule:b src:y]
```

## Write a left-recursive grammar

Direct and indirect left recursion are rewritten automatically, so the
natural form just works:

```go
j := tabnas.Make()
abnf.Install(j, `
expr   = expr "+" term / expr "-" term / term
term   = "(" expr ")" / number
number = 1*DIGIT
`, nil, nil)

out, _ := j.Parse("1+2-3")
_ = out // node with rule == "expr"
```

A production that is *only* left-recursive (no non-recursive seed, e.g.
`a = a "x"`) cannot be eliminated — conversion panics for that input, so
give every recursive rule a base case.

## Match case-sensitively

A bare quoted string is case-*insensitive* per RFC 5234. Use `%s"…"` for
a case-sensitive match (`%i"…"` is the explicit default):

```go
ci := tabnas.Make()
abnf.Install(ci, `g = "GET"`, nil, nil)
ci.Parse("get") // accepts

cs := tabnas.Make()
abnf.Install(cs, `g = %s"GET"`, nil, nil)
_, err := cs.Parse("get") // err != nil — case-sensitive
```

## Match by code point with numeric values

`%x` hex, `%d` decimal, `%b` binary. A range `%x30-39` matches one
character in `[0-9]`; a dotted form `%x66.6f.6f` concatenates into the
literal `foo`.

```go
j := tabnas.Make()
abnf.Install(j, "digits = 1*DIGIT\nDIGIT = %x30-39", nil, nil)
j.Parse("12345") // accepts
```

## Extend a rule incrementally

`=/` appends alternatives to a rule defined earlier (the base must come
first):

```go
j := tabnas.Make()
abnf.Install(j, `
command = "get"
command =/ "post"
command =/ "delete"
`, nil, nil)
j.Parse("post") // accepts
```

## Override a core rule

Define a rule with a core name locally and your version wins:

```go
j := tabnas.Make()
abnf.Install(j, "number = 1*DIGIT\nDIGIT = \"1\" / \"3\" / \"5\" / \"7\" / \"9\"", nil, nil)
j.Parse("135")  // accepts (odd digits only)
```

## Attach user actions

Pass an `ActionsMap` to `Install`. Each value is a slice of
`func(*Rule, *Context)` run after the compiler's own tree action, so
`r.Node` already holds the `{rule, src, kids}` map. Reference an
alternative by `@<rule>:o:<mark>` (open) or `@<rule>:c:<mark>` (close).

```go
j := tabnas.Make()
abnf.Install(j, `op = "inc" / "dec"`, nil, abnf.ActionsMap{
	"@op:o:INC": {func(r *tabnas.Rule, _ *tabnas.Context) {
		if n, ok := r.Node.(map[string]any); ok {
			n["delta"] = 1
		}
	}},
	"@op:o:DEC": {func(r *tabnas.Rule, _ *tabnas.Context) {
		if n, ok := r.Node.(map[string]any); ok {
			n["delta"] = -1
		}
	}},
})

out, _ := j.Parse("inc")
m := out.(map[string]any)
_ = m["delta"] // 1
```

To discover the marks, list them with `MarkListing` (or the CLI
`--marks`). The grammar must be converted with `Marks: true`:

```go
spec, _ := abnf.Abnf(`op = "inc" / "dec"`, &abnf.AbnfConvertOptions{Marks: true})
fmt.Println(abnf.MarkListing(spec))
// op  o:INC  s:#INC
// op  o:DEC  s:#DEC
```

A mark is the alternative's leading discriminator (token name without
`#`, pushed-rule name, or `_` for empty); collisions get a `~N` suffix.
You can also attach actions to an already-built spec with
`AttachActions(spec, actions)`, and rule-phase hooks via
`@<rule>:<bo|ao|bc|ac>`.

## Compile to portable pure data

`AbnfCompile` emits the grammar as jsonic text with no closures. By
default it's recognition-only; set `Recognition` to keep the
AST-building builtins.

```go
text, err := abnf.AbnfCompile(`greet = "hi" / "hello"`, nil)
if err != nil {
	return err
}
_ = text // jsonic string, no closures
```

`AbnfCompileOptions`:

```go
recognition := false
text, _ := abnf.AbnfCompile(`greet = "hi"`, &abnf.AbnfCompileOptions{
	Start:       "",             // default: first production
	Strict:      true,           // valid JSON instead of relaxed jsonic
	Indent:      2,
	Recognition: &recognition,   // *bool; default true (recognition-only)
})
```

`AbnfCompile` always converts internally with `Builtins: true`, so even
a probe-dispatch grammar (optional-prefix ambiguity) compiles to pure
data. Calling `ToRecognitionSpec` on a closure-mode probe spec returns
an `*AbnfCompileError`.

## Validate against samples from the shell

The CLI's `--parse` flag converts, installs, and parses each sample:

```bash
go run ./cmd/tabnas-abnf -c 'greet = "hi" / "hello"' --parse 'hi'
# ok: "hi" -> {"kids":[],"rule":"greet","src":"hi"}

go run ./cmd/tabnas-abnf 'g = "hi"' --parse 'bye'
# fail: "bye": ... (exit code 1)
```

(`-c` prints the tree on one line; the default indents it.)
