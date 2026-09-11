# Design: collecting a repetition into an `; @array`

| | |
|---|---|
| **Status** | **Open — needs a decision.** The behaviour described below is what `@tabnas/abnf` 0.4.11 ships. It follows from the annotation contract, is documented in `ts/doc/guide.md`, and is pinned by tests in both runtimes — so it cannot change by accident. Nothing here is a regression; it is undelivered scope. |
| **Scope** | `@tabnas/abnf` (the annotation surface) + `@tabnas/bnf` (the emitter) + a proposed `@tabnas/parser` engine primitive |
| **Repo** | This document lives in `tabnas/abnf` because that is where the annotation is authored and where a user meets the problem. Option A's engine half belongs in `tabnas/parser` and its emitter half in `tabnas/bnf`. |
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
| `*item` | `123` | `["123"]` | blob |
| `1*item` | `123` | `["123"]` | blob |
| `( item ) *( item )` | `123` | `["123", ""]` | blob |
| `item [ "," list ]` | `1,2,3` | refused — self-recursive part | error |
| `a "," b` | `1,2` | `["1", "2"]` | **correct** |

Only the last row works, and only because its length is fixed at two. **No ABNF
spelling of a variable-length list is handled today**, which means the half of
the feature arrays exist for is unreachable. That is the argument for treating
this as more than a rough edge.

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

### A. Spread on push, and propagate value mode into helpers — *recommended*

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

**B now, A when there is room.** They compose: the refusal costs little, removes
the silent wrong answer today, and is deleted by A rather than worked around.
Shipping B alone is defensible; shipping neither leaves a documented silent
failure in a feature whose whole review history was about eliminating exactly
that.

A is worth doing because the gap is narrow. The engine change is small and
mirrors a rule that already exists; the emitter change follows a path tree mode
already walks. It is a release chain — 0.9.6 through bnf and abnf — not a
redesign.

## 5. Open questions

Answer these before A starts.

1. **Does an object member that is a repetition stay as text?** For `@object`
   the author *names* the part, so "this member is the matched text of the run"
   is a defensible reading. Arrays name nothing, which is what makes the blob
   indefensible there. Suggest: fix arrays only, leave objects as they are.
2. **What does an empty run produce?** Today `["1", ""]`. Under A it should be
   `["1"]` — no element rather than an empty one. Worth stating explicitly,
   because it is a behaviour change beyond the obvious fix.
3. **Do separators ever become elements?** No — a literal produces no value and
   is not a member. Named here so the helper emission does not quietly decide
   otherwise.
4. **Is `spread` configuration, or a separate builtin?** A flag on `@push$`
   keeps the action set small and matches how `src` and `lit` are already
   carried. A distinct `@concat$` would read more legibly in an emitted spec.
   Weak preference for the flag.

## 6. Related

- [`alt-action-refs.md`](./alt-action-refs.md) — the `$`-builtin design this
  feature is built on.
- [`implementation-diary.md`](./implementation-diary.md) — §9 "Still design-only"
  is where this belongs in the log.
- `ts/doc/guide.md`, "Build a value instead of a tree" — the user-facing
  documentation of the current behaviour, including the repetition caveat.
- `test/spec/alignment-abnf-ast.tsv` — the five positive annotation rows;
  `alignment-abnf-errors.tsv` — the eighteen refusals, byte-for-byte in both
  runtimes.
