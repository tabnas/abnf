# Design: collecting a repetition into an `; @array`

| | |
|---|---|
| **Status** | **Done.** Option A′ (§3.A′) is implemented and green in both runtimes. `; @array` collects a repetition, with no new builtin and no change to the spec format. The Go engine defect in §6 is fixed in `tabnas/parser` (PR #167); this repository's change lands after that and the `@tabnas/bnf` emitter half (PR #39) are released. |
| **Implementation** | `@tabnas/bnf` PR #39 (emitter, both runtimes) + `@tabnas/parser` PR #167 (the §6 fix) + this repository's guide and fixtures. |
| **Scope** | `@tabnas/bnf` (the emitter) + `@tabnas/abnf` (guide and fixtures), plus one Go-only fix in `@tabnas/parser` — §6 |
| **Repo** | This document lives in `tabnas/abnf` because that is where the annotation is authored and where a user meets the problem. The emitter half belongs in `tabnas/bnf`, the §6 fix in `tabnas/parser`. |
| **Measured against** | `@tabnas/parser` 0.9.5, `@tabnas/bnf` 0.1.14, `@tabnas/abnf` 0.4.11 |

## 1. Problem

A value annotation says what a rule **builds**. `; @object` names one member per
part of the alternative that produces a value; `; @array` names nothing, and
every part that produces a value becomes an element, in order.

A **repetition is one part**. So the whole repeated run arrives as a single
element, resolved to its source text:

```abnf
list = item *( "," item )   ; @array
item = 1*DIGIT
```

```
parse("1,2,3")  ->  ["1", ",2,3"]
parse("1")      ->  ["1", ""]
```

The second element is the repetition's matched text, separators included. An
empty run yields an empty-string element rather than no element.

This is consistent with the stated contract. It is also not what the grammar
means, and there is no diagnostic — the author gets a plausible-looking array
with the wrong contents.

### 1.1 There is no working spelling

Every way of writing a variable-length list degrades the same way. Each row
below was compiled and parsed, not reasoned about:

| grammar body | input | result | |
|---|---|---|---|
| `item *( "," item )` | `1,2,3` | `["1", ",2,3"]` | blob |
| `*( item "," ) item` | `1,2,3` | `["1,2,", "3"]` | blob |
| `*item` | `123` | `["123"]` | correct, by accident — see below |
| `1*item` | `123` | `["123"]` | correct, by accident |
| `( item ) *( item )` | `123` | `["123", ""]` | blob |
| `item [ "," list ]` | `1,2,3` | refused — self-recursive part | error |
| `a "," b` | `1,2` | `["1", "2"]` | **correct** |

Only the last row works, and only because its length is fixed at two. **No ABNF
spelling of a variable-length list is handled today**, which means the half of
the feature arrays exist for is unreachable. That is the argument for treating
this as more than a rough edge.

(Three rows need a footnote, because `item = 1*DIGIT` is **greedy**. On
`123` it matches all three digits, so there genuinely is one item and
`["123"]` is the right answer: `*item` and `1*item` are not blobs as
written, and `( item ) *( item )`'s `["123", ""]` is one correct element
plus a spurious empty one, not a blob either. Re-measured with
`item = DIGIT`, where three items are what the grammar means, 0.4.11
answers `["123"]`, `["123"]` and `["1", "23"]` — and those are blobs.
The point stands, and the last of them is the sharpest illustration of
it in the table; the rows as originally written overstated it.)

## 2. Why

`*( "," item )` is desugared into a generated helper rule before the emitter
sees it, and that helper is right-recursive — each iteration pushes the next.
The enclosing rule pushes the helper **once**, so `@push$`, which appends
`r.child.node`, sees a single child standing for the entire run:

```
list        open  p: _gen1_plus_DIGIT        <- item, inlined by Paull's pass
            close r: list$step1
list$step1  open  p: _gen4_star__gen3_group  <- the whole repetition, one push
            close @capture$

_gen4_star__gen3_group$alt0
            open  p: _gen3_group
            close r: ...$alt0$step1          <- recurses into itself
```

The helper builds a tree node, not an array, so the outer `@push$` has nothing
structural to take and falls back to that node's accumulated `src`. Hence the
blob.

(Note in passing that `item` does not appear in `list`'s first segment either —
Paull's substitution inlined it. That is the same defect the annotation's
*named members* exist to work around, and it is why the first element is
correct here while the rest are not.)

### 2.1 The tree side already solved this

Which is what makes the gap narrow rather than deep. `@capture$` distinguishes a
tagged child from an untagged helper and **splices** the helper's children into
the parent, so a repetition flattens into siblings:

```js
if (c.rule) n.kids.push(c)                              // a real rule: one kid
else if (Array.isArray(c.kids)) n.kids.push(...c.kids)   // a helper: splice
```

And for the same-depth repeat shape the engine carries `@fold$`, whose own
comment states this problem in its own words:

> each iteration delivers its own node to the parent as a sibling kid, since the
> parent's `r.child` pointer stays on the FIRST iteration and capture-on-close
> cannot see the run

So: tree-building has a splice rule and a fold. Value-building has neither —
`@push$` only ever appends one thing.

## 3. Options

### A′. Let the helper fill the array it already has — *chosen*

**No engine change, no spread, no schema bump.** Option A below was
costed on an assumption that turned out to be false: that the enclosing
array and the helper's result are two arrays, so one must be spliced
into the other. They need not be. The engine already **seeds a pushed
child's node from its parent** (`ts/src/rules.ts:665` and `:691`;
`go/rule.go:1237` and `:1262`), so a helper that simply *does not
allocate* is handed the annotated rule's array and pushes its elements
straight into it. There is only ever the one array, and nothing has to
learn to splice.

What the emitter does, then, is not propagate "value mode" so much as
withhold the tree builders:

- Helpers generated by a **repetition** inside an `; @array` lose
  `@node$`/`@capture$` and allocate nothing — they inherit the array.
- A link inside such a helper that pushes a rule of the author's gets
  `@push$` (bare when that rule is annotated, `{src: true}` otherwise).
- The annotated rule's own link that pushes such a helper pushes
  **nothing** — the helper has already filled the array.

Measured, not reasoned about — every row of §1.1, re-run:

| grammar body | input | 0.4.11 | under A′ |
|---|---|---|---|
| `item *( "," item )` | `1,2,3` | `["1", ",2,3"]` | `["1","2","3"]` |
| `item *( "," item )` | `1` | `["1", ""]` | `["1"]` |
| `*( item "," ) item` | `1,2,3` | `["1,2,", "3"]` | `["1","2","3"]` |
| `*item` | `123` | `["123"]` | `["1","2","3"]` |
| `1*item` | `123` | `["123"]` | `["1","2","3"]` |
| `( item ) *( item )` | `123` | `["1", "23"]` | `["1","2","3"]` |
| `*( a b )` | `x1y2` | — | `["x","1","y","2"]` |
| `a [ "," b ]` | `1` | `["1", ""]` | `["1"]` |

(The last four rows are measured with `item = DIGIT`, `a = ALPHA`,
`b = DIGIT` — one character each. With `item = 1*DIGIT` the item is
greedy and swallows the run, so `*item`, `1*item` and
`( item ) *( item )` all answer `["123"]` under A′ and always answered
`["123"]`/`["123", ""]` before: there genuinely is one item, and no
value-building change can alter what the lexer consumed. The original
table recorded those as blobs, which overstated them. A one-character
item is what makes the collection visible.)

| | |
|---|---|
| **Engine** | No new builtin, no config, no `BUILTIN_SCHEMA_VERSION` bump, no validator rows, no spec-format change, no TypeScript or Rust work. One Go-only bug fix, which still has to be released before the emitter can be — §6. |
| **bnf** | `planArrayHelpers`, one flag per part in the annotation plan, and array mode in the three emit paths. ~200 lines per runtime. |
| **abnf** | No syntax change. `; @array` starts working; guide and fixtures updated. |
| **Parser** | One Go-only bug fix — §6. No spec-format or schema change, nothing for TypeScript or Rust. |

A′ also settles three of §5's open questions by construction rather than
by decision, which is the strongest sign it is the right shape: an empty
run pushes nothing and so yields no element; a separator is a literal
and pushes nothing; and `spread` is not configuration because it does
not exist.

### A. Spread on push, and propagate value mode into helpers — *superseded by A′*

Give `@push$` a `spread` configuration that extends the array with an
array-valued child instead of appending it — the exact counterpart of
`@capture$`'s splice — and make the emitter build **arrays** in the generated
helpers of an annotated repetition, rather than only in the annotated rule's own
chain.

The engine half:

```js
const makePush$ = (cfg) => (r) => {
  const c = r.child.node
  if (undefined === c || !Array.isArray(r.node)) return
  if (cfgTrue(cfg.spread) && Array.isArray(c)) r.node.push(...c)   // new
  else r.node.push(cfgTrue(cfg.src) ? srcVal(c) : c)
}
```

A dozen lines across three ports. The weight is in `@tabnas/bnf`: today the
value builders are applied to one production's chain, and they would have to
follow desugaring into every helper the repetition generates, with separator
literals contributing nothing. Tree mode already does exactly this — every
helper gets `@node$` and `@capture$` — so there is a working precedent to mirror
rather than a new mechanism to invent.

| | |
|---|---|
| **Engine** | `@push$ spread`, `BUILTIN_SCHEMA_VERSION` 5 → 6, validator rows, three ports (ts/go/rs), six version sites — a 0.9.6 release. |
| **bnf** | Value-mode propagation through desugar. The substantial piece. |
| **abnf** | No syntax change. `; @array` simply starts working; guide and fixtures updated. |
| **Risk** | Touches the desugar/emit boundary, where most of the 31 review findings on this feature already lived. |

### B. Refuse a repetition inside `; @array`

Make the planner reject what it cannot build, with a diagnostic naming the rule.
`; @array` becomes an explicitly fixed-arity feature.

This is consistent with how every other boundary-moving rewrite in this feature
is already handled: the emitter refuses probe dispatch, left factoring and the
tail repeat for the same reason — the member count survives while the boundary
the author drew does not.

The cost is honesty about scope: the annotation would advertise arrays while
supporting only the shape nobody writes. A good holding position, a poor
destination.

| | |
|---|---|
| **Engine** | None. |
| **bnf** | One refusal in `planValueAnnotations`, both ports, plus tests. |
| **abnf** | Two `alignment-abnf-errors` rows; the guide's repetition caveat becomes a refusal. |
| **Risk** | Low, and reversible — A deletes it rather than working around it. |

### C. Leave it, documented — *what 0.4.11 ships*

The behaviour follows from the contract, the guide says so plainly, and tests in
both runtimes pin it so a change has to be deliberate. The objection is that the
author gets `["1", ",2,3"]` with no error and no hint. The failure is silent, and
silence is the category this feature's entire review history was about.

### D. Flatten the array afterwards — *rejected*

Post-process the finished array to split the blob element. Rejected because the
information is already gone: by the time the element exists it is **text**, and
recovering elements would mean re-parsing the separator out of a string the
grammar already knew how to cut. A value builder that re-lexes its own output is
a worse defect than the one it repairs.

### E. Rewrite the list into a tail repeat, and give `@fold$` a value counterpart — *rejected*

Tempting, because `@fold$` already delivers each iteration to the parent and a
value-side `@foldpush$` would be a close parallel. But `rewriteTailRepeats` only
recognises `X = seq [ sep X ]` where the prefix and separator are **bare
terminals**, and `item *( "," item )` is neither shape — `item` is a rule
reference. Making the idiom fit would mean a grammar transformation that changes
what the author wrote, which is precisely what the refusals in §3.B exist to
avoid.

## 4. Recommendation

**A′.** B was never shipped, and should not be. Its case was that A was
expensive enough to need a holding position; A′ is not, and a refusal for
a shape that now works would have had to be written, tested, documented
in two runtimes and then deleted.

What A′ costs is **no new builtin and no schema revision** — not no
engine release. The distinction matters, and the first draft of this
section blurred it. The §6 fix has to ship in `@tabnas/parser` first,
then be picked up by `@tabnas/bnf` and `@tabnas/abnf` (which pin
`parser/go v0.9.5` and `>=0.9.5` today) before the shared fixtures can
pass in both runtimes. That is a release chain, and publishing the
emitter against a parser without the fix is exactly the order not to do
it in.

What A′ does save is the part that made A expensive: no `@push$ spread`,
no `BUILTIN_SCHEMA_VERSION` 5 → 6, no validator rows, no spec-format
change, and no TypeScript or Rust work — the fix is Go-only, and it is a
bug fix in the engine rather than a new primitive for grammars to
target.

The order matters, and is the one thing here that cannot be shortcut:
`tabnas/parser` #167, then `@tabnas/bnf` #39, then this repository.
Applying the emitter half against a parser without the §6 fix has Go DROP
the collected elements rather than blob them — a worse answer to the same
question than the one being replaced.

## 5. Questions, answered

1. **Does an object member that is a repetition stay as text?** **Yes.**
   For `@object` the author *names* the part, so "this member is the
   matched text of the run" is a reading they asked for. Arrays name
   nothing, which is what makes it indefensible there. `top = a *( "," a )`
   with `; @object a rest` still gives `{a: "1", rest: ",2,3"}`.
2. **What does an empty run produce?** **No element** — `["1"]`, not
   `["1", ""]`. Not decided so much as inherited: the helper pushes
   nothing, so nothing arrives. An absent `[ option ]` behaves the same.
3. **Do separators ever become elements?** **No.** A literal pushes
   nothing, in a helper exactly as in the annotated rule.
4. **Is `spread` configuration, or a separate builtin?** **Neither** —
   A′ needs no spread at all.

Two the original list did not ask, both settled while building:

5. **What does an iteration with more than one value-producing part
   do?** Flattens, in order: `*( a b )` on `x1y2` gives
   `["x","1","y","2"]`. `; @array` takes every part that produces a
   value, and an iteration is not a special case.
6. **Does a bare group collect too?** **No** — only a repetition does.
   Tried it both ways: collecting a group loses what the author wrote.
   `( "[" p "]" )` means the one element `[7]`, and a group holding only
   terminals has no reference in it at all, so collecting either yields
   *fewer* elements, silently. A group written as the ITEM of a
   repetition does collect, because there it is the repeated thing
   rather than an element — `*( "," item )` is a list of `item`, not a
   list of runs. Two existing tests pin the distinction, which is how it
   was caught.

One refusal is deliberately kept: a part that reaches the annotated rule
**itself** (`top = 1*DIGIT [ "+" top ]   ; @array`). Collecting it is
well-defined — `top`'s own array nests as an element — but nobody
writing a list means `["1", ["2", ["3"]]]`, so it stays refused.

## 6. What blocked it: Go grew a list the caller could not see

The emitters agree. Compiling `list = *( a b )   ; @array` with both
CLIs and diffing the JSON gives the same rules, the same actions and the
same config, down to the `@push$ {src:true}` on each link. The values
differ anyway: TypeScript answers `["x","1","y","2"]` and Go answers
`[]`.

The cause is not the annotation, or the plan, or the emitted spec. It is
that **a Go slice is a value and a JavaScript array is a reference.**

`@push$` in TypeScript does `r.node.push(v)`, mutating the one array
object every rule in the chain is holding. Go's `NodeListAppend` returns
a *new header*, so `builtinPushCfg` has to re-publish it — and it
publishes to exactly two places (`go/builtins.go`, `@push$`): the
immediate parent, and the rules this one replaced along `r.Prev`.

That is enough for the shapes it was written for, where the push happens
one level below the rule holding the list. A′ pushes from *arbitrarily
deep*: the repetition helper is right-recursive, so a three-element list
grows at three different depths, and only the innermost push reaches a
parent that is still holding the array.

Instrumenting `@push$` confirms it is not an inheritance failure — every
level does receive the array:

```
PUSH rule=_gen1_star_item$alt0   node=[]interface {} child=map[string]interface {}
PUSH rule=_gen1_star_item$alt0   node=[]interface {} child=map[string]interface {}
PUSH rule=_gen1_star_item$alt0   node=[]interface {} child=map[string]interface {}
```

Three pushes, each with the array in hand. The elements are appended and
then lost on the way back up.

Widening the write-back to walk the whole ancestor chain was tried and
does not work: `sameGrownList` cannot match an EMPTY list, deliberately
(Go gives two distinct zero-length slices the same data pointer, so it
would guess wrong) — and an array starts empty, which is exactly when
the first push needs to propagate. `ListRef` does not help either; it is
a value struct too.

**Fixed in `tabnas/parser` #167**, and not by the representation change
this section first proposed. Making the list node a reference would have
touched all 82 places that handle `[]any`. Instead every rule now knows
which rule holds the authoritative copy of the container it is building
into — a new unexported `Rule.nodeOwner`, seeded down from the parent on
a push and from the REPLACED rule on an `r:`. `@push$` grows that rule's
list and writes it back there: one write, whatever the depth.

The second seeding link is load-bearing. `list` replaces itself with
`list$step1` before pushing the repetition, and the replacement is
parented ABOVE `list`, so inheriting through `Parent` alone would skip
the owner — the list would reach everything except the rule whose value
is read.

Naming the owner rather than searching for it is also what keeps it
linear. The first version of the fix walked the ancestors on each append,
which is Θ(n²) in the length of the list and reachable from untrusted
input: 1600 elements took 32.8 ms walking against 4.0 ms with the owner.
Caught in review, and worth recording because the walk *worked* — it was
correct and quadratic, which is the kind of fix that passes every test
and is still wrong.

Slice identity could never have answered "who still holds this?" — two
distinct empty slices share a data pointer, and a list is empty exactly
when the first push happens. Naming the owner removes the question rather
than answering it, and retires the case `go/doc/differences.md` recorded
as deliberately traded away. Go-only bookkeeping: no API, spec-format or
schema change, and nothing for the other ports to mirror.

It buys more than this feature — the one-level write-back was a general
limit on how deep ANY value grammar could accumulate.

## 7. Related

- [`array-repetition.patch`](./array-repetition.patch) — the A′
  implementation, both runtimes, not applied.
- [`alt-action-refs.md`](./alt-action-refs.md) — the `$`-builtin design this
  feature is built on.
- [`implementation-diary.md`](./implementation-diary.md) — §9 "Still design-only"
  is where this belongs in the log.
- `ts/doc/guide.md`, "Build a value instead of a tree" — the user-facing
  documentation of the current behaviour, including the repetition caveat.
- `test/spec/alignment-abnf-ast.tsv` — the five positive annotation rows;
  `alignment-abnf-errors.tsv` — the eighteen refusals, byte-for-byte in both
  runtimes.
