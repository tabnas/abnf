# How-to guide

Focused recipes for real problems. Each is independent; jump to the one
you need. For the full API see [reference.md](reference.md); for the
"why" see [concepts.md](concepts.md).

## Convert a grammar without installing it

Use `abnfConvert` when you want the `GrammarSpec` in hand — to install it
later, inspect it, or pass it around.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnfConvert } = require('@tabnas/abnf')

const spec = abnfConvert(`pair = "a" "b"`)
const tn = new Tabnas()
tn.grammar(spec)
tn.parse('ab').rule // => 'pair'
```

## Choose a different start rule

By default the first production is the start rule. Override it with
`start`:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnfConvert } = require('@tabnas/abnf')

const spec = abnfConvert(`a = "x"\nb = "y"`, { start: 'b' })
const tn = new Tabnas()
tn.grammar(spec)
tn.parse('y').rule // => 'b'
```

## Write a left-recursive grammar

ABNF often reads most naturally as left recursion (`expr = expr "+"
term / term`). The compiler rewrites direct and indirect left recursion
automatically, so you can write it the natural way:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
expr   = expr "+" term / expr "-" term / term
term   = "(" expr ")" / number
number = 1*DIGIT
`)

tn.parse('1+2-3').rule // => 'expr'
```

A production that is *only* left-recursive (no non-recursive seed
alternative, e.g. `a = a "x"`) cannot be eliminated and throws at
convert time. Give every recursive rule a base case.

## Match case-sensitively

By RFC 5234, a bare quoted string is **case-insensitive**: `"GET"`
matches `get`, `Get`, `GET`. Use `%s"…"` to force a case-sensitive
match (`%i"…"` is the explicit form of the default).

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const ci = new Tabnas({ plugins: [abnf] })
ci.abnf(`g = "GET"`)
ci.parse('get').rule // => 'g'

const cs = new Tabnas({ plugins: [abnf] })
cs.abnf(`g = %s"GET"`)
cs.parse('GET').rule // => 'g'
```

With the case-sensitive grammar, `cs.parse('get')` throws.

## Match by code point with numeric values

ABNF numeric values name characters by code point. `%x` is hex, `%d`
decimal, `%b` binary. A range like `%x30-39` matches any one character
in `[0-9]`; a dotted form like `%x66.6f.6f` concatenates code points
into the literal `foo`.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`digits = 1*DIGIT\nDIGIT = %x30-39`)
tn.parse('12345').rule // => 'digits'
```

## Extend a rule incrementally

ABNF's `=/` operator appends alternatives to a rule defined earlier.
The base `name = …` must appear first.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
command = "get"
command =/ "post"
command =/ "delete"
`)

tn.parse('post').rule // => 'command'
```

A `=/` with no earlier base rule throws.

## Override a core rule

The RFC 5234 core rules (`ALPHA`, `DIGIT`, `HEXDIG`, …) are auto-included
when referenced. Define a rule with the same name locally and your
definition wins:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`number = 1*DIGIT\nDIGIT = "1" / "3" / "5" / "7" / "9"`)
tn.parse('135').rule // => 'number'
```

Here `number` accepts only odd digits, because the local `DIGIT`
shadows the core one.

## Attach user actions to build a custom value

Pass `actions` to the plugin call to run your own code on a matched
alternative. Reference an alternative by `@<rule>:o:<mark>` (open) or
`@<rule>:c:<mark>` (close). The compiler's own tree action runs first,
so `r.node` already exists when your action runs.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf('op = "inc" / "dec"', {
  actions: {
    '@op:o:INC': (r) => { r.node.delta = 1 },
    '@op:o:DEC': (r) => { r.node.delta = -1 },
  },
})

tn.parse('inc').delta // => 1
tn.parse('dec').delta // => -1
```

To discover the available marks, list them with `markListing` (or the
CLI `--marks` flag):

```js
const { abnfConvert, markListing } = require('@tabnas/abnf')

markListing(abnfConvert('op = "inc" / "dec"', { marks: true }))
// => 'op  o:INC  s:#INC\nop  o:DEC  s:#DEC'
```

The mark is the alternative's leading discriminator — the token name
(without the `#`), the pushed rule name, or `_` for an empty alt.
Same-leading-token alternatives get a `~N` suffix to keep marks unique.

## Compile a grammar to portable pure data

`abnfCompile` emits the grammar as **jsonic text** with no closures —
just data and engine-builtin references. It is the way to ship a
compiled grammar to another process or language. By default it emits a
*recognition-only* grammar (tree building dropped); pass
`recognition: false` to keep the AST-building builtins.

```js
const { abnfCompile } = require('@tabnas/abnf')

const text = abnfCompile('greet = "hi" / "hello"')
typeof text // => 'string'
```

Round-trip it back into a working grammar with the engine's
`resolveFuncRefs`. Use `recognition: false` to keep the tree-building
builtins so the reloaded grammar rebuilds the full `{rule, src, kids}`
AST:

```js
const { Tabnas } = require('@tabnas/parser')
const { resolveFuncRefs } = require('@tabnas/parser/utility')
const { abnfCompile } = require('@tabnas/abnf')

const spec = resolveFuncRefs(JSON.parse(
  abnfCompile('greet = "hi" / "hello"', { recognition: false, strict: true })))
const tn = new Tabnas()
tn.grammar(spec)
tn.parse('HI').rule // => 'greet'
```

Use `strict: true` to emit valid JSON (double quotes, commas);
the relaxed default emits jsonic (bare keys, single quotes). In the
default *recognition* mode the reloaded grammar still accepts/rejects
the same inputs, but `parse` returns no tagged tree (tree building is
dropped) — use `recognition: false` when you need the AST back.

Note: a grammar that needs the probe dispatcher (optional-prefix
ambiguity like `[ A "@" ] A`) can only be compiled as *recognition*
data if you also keep the control builtins. `abnfCompile` handles this
for you by always converting with `builtins: true`; calling
`toRecognitionSpec` directly on a closure-mode spec for such a grammar
throws `AbnfCompileError`.

## Validate a grammar against samples from the shell

The CLI's `--parse` flag converts a grammar, installs it, and parses
each sample, printing the tree or an error:

```bash
tabnas-abnf 'greet = "hi" / "hello"' --parse 'hi'
# ok: "hi" -> {"rule":"greet","src":"hi","kids":[]}

tabnas-abnf 'g = "hi"' --parse 'bye'
# fail: "bye": ... (exit code 1)
```
