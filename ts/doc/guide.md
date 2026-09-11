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

## Build a value instead of a tree

By default a grammar produces a parse tree — a `rule`/`src`/`kids` node
per rule. A trailing comment can say what a rule should build instead:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  ver = maj "." min "." pat   ; @object maj min pat
  maj = 1*DIGIT
  min = 1*DIGIT
  pat = 1*DIGIT
`)

tn.parse('1.2.30') // => { maj: '1', min: '2', pat: '30' }
```

The keys are the names in the annotation. Nothing in the input spells
them — `1.2.30` contains no `maj` — so they can only come from the
comment.

A comment is the only place in RFC 5234 that carries no meaning of its
own, which is why the annotation can live there without changing what
the grammar accepts. Delete every annotation and the same inputs parse;
you get the tree back. That property is worth relying on: the annotation
is about the OUTPUT, never about the language.

`@object` names one member per part of the rule that produces a value. A
part is a rule reference, a group, or a repetition; a literal produces
nothing and is not named. So `"."` above is not a member, and
`; @object maj min pat` names three parts for three references.

### Nesting

A member whose own rule is annotated is assigned whole, so values nest:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  top   = name "=" inner   ; @object name inner
  name  = 1*ALPHA
  inner = maj "." min      ; @object maj min
  maj   = 1*DIGIT
  min   = 1*DIGIT
`)

tn.parse('ab=1.2') // => { name: 'ab', inner: { maj: '1', min: '2' } }
```

Every other member is the source text the part matched. There is no
third case: annotated means nested, unannotated means text.

### Arrays

`@array` names nothing. Every part that produces a value becomes an
element, in order:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  pair = a "," b   ; @array
  a    = 1*DIGIT
  b    = 1*ALPHA
`)

tn.parse('1,xy') // => ['1', 'xy']
```

Elements nest by the same rule as members: an element whose rule is
annotated is pushed whole.

**A repetition is one element, not many.** `*( "," item )` is a single
part, so its whole run arrives as one element of source text:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  list = item *( "," item )   ; @array
  item = 1*DIGIT
`)

tn.parse('1,2,3') // => ['1', ',2,3']
```

That is consistent with "one element per part", and it is almost
certainly not what you want. Collecting a repetition into one element
each is not supported yet; until it is, build lists with a user action
(see the next section) rather than `@array`.

### What is refused

The annotation describes the rule the author wrote, and several rewrites
happen between that and the emitted parser. Where a rewrite would make
the annotation describe something else, the conversion fails rather than
building a differently-shaped value:

- **More than one alternative.** One list of names cannot describe two
  alternatives' parts. Split the rule, or annotate the alternatives' own
  rules.
- **A member count that does not match the parts.** Including the case
  where a part stops being one: a rule whose whole body is a single
  literal (`PL = "+"`) becomes a lexer token, so it is no longer a part.
  Give it a body that is not a bare terminal to keep it nameable.
- **A leading member whose own rule builds a value.** A rule's first
  reference is folded into it by left-recursion elimination, which
  erases that rule's builders — the member would hold an internal node
  instead of the value you asked for. Putting a literal before it stops
  the fold:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  top   = "v" inner "," x   ; @object inner x
  inner = a "." b           ; @object a b
  a     = 1*DIGIT
  b     = 1*DIGIT
  x     = 1*DIGIT
`)

tn.parse('v1.2,3') // => { inner: { a: '1', b: '2' }, x: '3' }
```

- **A leading member whose rule is not one part.** Same fold, followed
  through aliases: if it resolves to a body with more than one part, the
  boundary you drew would be lost.
- **A source-text member that reaches a value-building rule.** See the
  next section.

Each refusal names the rule and says what to change.

### A rule that builds a value produces no text

This is the one rule to carry away, and everything above follows from
it: a rule with an annotation contributes its **value** to whatever
contains it, and no **text**. Its object becomes a child; it is not a
span of characters any more.

Two consequences, with different severity.

Inside a plain (unannotated) grammar this is mild — the value lands
where you expect, and only the enclosing node's `src` is short of it:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  doc  = head ":" body
  head = 1*ALPHA
  body = d        ; @object d
  d    = 1*DIGIT
`)

const out = tn.parse('ab:7')
out.kids[0]  // => ({ d: '7' })
out.src      // => 'ab:'
```

`body` built its object and it is right there in `kids` — but `doc.src`
is `'ab:'`, not `'ab:7'`, because `body` gave a value rather than text.
Mixing the two like this is supported; just do not read `src` on a node
that contains an annotated rule.

Inside an **annotated** rule the same loss would be the whole answer, so
it is refused instead. `top = "<" ( inner ) ">"` with `; @array` and an
annotated `inner` would have built `[""]` — the element is the text of
the group, and `inner` contributed none. Making the annotated rule the
part itself is the fix, since a part that *is* an annotated rule nests
rather than resolving to text:

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`
  top   = "<" inner ">"   ; @array
  inner = d               ; @object d
  d     = 1*DIGIT
`)

tn.parse('<7>') // => [{ d: '7' }]
```

The refusal follows plain rule references too, not just groups and
repetitions — an ordinary intermediate rule loses the text in exactly
the same way.

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

## Accumulate a value across a whole parse

For state that spans the parse — a running total, a counter, a collected
list — keep it on a wrapping rule's node and let the repeating rule write
to `r.parent.node`. The compiler turns a tail self-reference
(`add = NR [ PL add ]`) into a same-depth repeat, so **every** repetition
shares the same parent:

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
    // `val` holds the running total.
    '@val:o:add': (r) => { r.node.value = 0 },

    // Each number adds to it.
    '@add:o:NR': (r) => { r.parent.node.value += r.o[0].val },
  },
})

tn.parse('1+2+3').value // => 6
```

Both actions are a single expression, and the total lives on the node
rather than in a variable outside the parse — so `parse` returns it, the
instance holds no state between calls, and two parses cannot interfere.

The tail-repeat compilation also flattens the tree: `1+2+3` yields three
sibling `add` kids under `val`, each spanning its own number, rather than
a right-nested chain. The repeat itself is addressable as a close-phase
mark (`@add:c:PL`), and the run's last iteration closes through
`@add:c:_`.

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
