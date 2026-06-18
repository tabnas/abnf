# Tutorial: your first ABNF grammar

This is a learning-oriented walkthrough. By the end you will have taken
an [ABNF](https://datatracker.ietf.org/doc/html/rfc5234) grammar from a
string of text to a working parser that builds a tree from your input.
Every step builds on the last; follow them in order.

`@tabnas/abnf` is a **compiler**: it reads ABNF source and emits a
tabnas `GrammarSpec`. You install that spec on a tabnas engine, and the
engine parses inputs in your grammar and hands you back an AST.

> One dialect note up front: this ABNF uses `=` to define a rule and `/`
> to separate alternatives — *not* `::=` or `|`. That is genuine
> RFC 5234 ABNF, not EBNF.

## Step 0: install

```bash
npm install @tabnas/parser @tabnas/abnf
```

`@tabnas/parser` is the engine; `@tabnas/abnf` is this compiler. You
need both.

## Step 1: install a grammar and parse

The simplest path uses the **plugin form**. You hand the ABNF compiler
to a `Tabnas` instance as a plugin; that adds a `tn.abnf(...)` method.
Call it with your grammar, then call `tn.parse(...)`.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)

tn.parse('hi').rule    // => 'greet'
tn.parse('hello').rule // => 'greet'
```

`greet = "hi" / "hello"` reads: *the rule `greet` matches the literal
`hi` or the literal `hello`.* `tn.parse('hi')` returns a tree node; its
`.rule` field is the name of the rule that matched.

## Step 2: look at the whole tree

Every rule produces a `{rule, src, kids}` node:

- `rule` — the grammar rule's name.
- `src` — the source text this rule matched.
- `kids` — child nodes, one per *referenced* sub-rule.

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)

tn.parse('hi') // => ({ rule: 'greet', src: 'hi', kids: [] })
```

`greet` matched only a literal, so it has no children — `kids` is empty.

## Step 3: sequences and sub-rules

Literals laid side by side form a sequence. A bare word is a *reference*
to another rule. Here `x` matches `"x"`, then a `name`, then `"="`, then
a `value`:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
x = "x" name "=" value
name = 1*ALPHA
value = 1*DIGIT
`)

tn.parse('xfoo=42') // => ({ rule: 'x', src: 'xfoo=42', kids: [{ rule: 'name', src: 'foo', kids: [] }, { rule: 'value', src: '42', kids: [] }] })
```

Two things to notice:

- `name` and `value` appear as `kids` — referenced rules become child
  nodes.
- `1*ALPHA` and `1*DIGIT` use **core rules** (`ALPHA`, `DIGIT`) you never
  defined. They come from RFC 5234 Appendix B.1 and are spliced in
  automatically whenever you reference them. `1*X` means "one or more
  `X`".

## Step 4: repetition and grouping

ABNF gives you repetition prefixes and parentheses for grouping. A
classic shape is a bracketed, comma-separated list:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
list = "[" item *("," item) "]"
item = 1*ALPHA
`)

tn.parse('[a,b,c]') // => ({ rule: 'list', src: '[a,b,c]', kids: [{ rule: 'item', src: 'a', kids: [] }, { rule: 'item', src: 'b', kids: [] }, { rule: 'item', src: 'c', kids: [] }] })
```

`*("," item)` means "zero or more of (`,` then `item`)". Each `item`
becomes a child of `list`.

## Step 5: parse without the plugin

You do not have to install the compiler as a plugin. `abnfConvert`
returns the spec directly, and you install it with `tn.grammar(spec)`:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnfConvert } = require('@tabnas/abnf')

const spec = abnfConvert(`greet = "hi" / "hello"`)
spec.options.rule.start // => '__start__'

const tn = new Tabnas()
tn.grammar(spec)
tn.parse('hello').rule // => 'greet'
```

The start rule is reported as `__start__`: the compiler wraps your start
rule in a small synthetic rule that makes sure end-of-input is consumed.
You still get your own rule's node back from `parse`.

## Where to go next

- **[guide.md](guide.md)** — recipes for real tasks: left recursion,
  case sensitivity, user actions, compiling to pure data.
- **[reference.md](reference.md)** — the exact API and CLI flags.
- **[concepts.md](concepts.md)** — how the compiler works and why.
