# Divergences

Where a port produces a different result from the canonical TypeScript
in `ts/`, for the same input. Every row below was MEASURED, on
2026-09-21, by running the three implementations over the input in its
first column; nothing here is inferred from reading the source.

Each entry names who owns the repair. An entry that closes must be
deleted, and the test that pins it fails until it is, so this file
cannot quietly go stale.

The conformance dial is NOT a divergence: TypeScript and Rust read the
same figures over the third-party corpus (48/52 valid, 611/661 invalid,
5 fragments, 2 over budget), and every grammar either compiler can
finish emits byte-identical pure-data grammar text. Go's invalid figure
is lower, which `AGENTS.md` records under "Conformance, as measured".

## Where the divergences are pinned

There is no executable register in `test/spec` for these. Two of the
four are invisible to a grammar-to-output fixture, which is what every
file there compares, and the fourth is about the shape of the API rather
than about any value. So each one is pinned by a Rust test in
`rs/tests/divergence_test.rs`, asserted in BOTH directions: the
behaviour recorded here, and the behaviour the canonical runtime has, so
a port that starts agreeing fails as loudly as one that starts
disagreeing.

## 1. A numeric value naming a lone surrogate

`%xD800` names one half of a UTF-16 surrogate pair. A JavaScript string
can hold one; a Rust `String` and a Go `string` cannot.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = %xD800` | fixed token `#T` is U+D800 | fixed token `#T` is U+FFFD | fixed token `#T` is U+FFFD |

**Reason.** `String.fromCodePoint(0xD800)` answers a lone surrogate.
`char::from_u32(0xD800)` answers `None`, because a `char` is a Unicode
scalar value by definition, and `string(rune(0xD800))` in Go yields the
replacement character for the same reason. There is no representation to
port to.

**Owner.** Nobody, unless the notation gains a way to mean this. A
surrogate code point names no character, so a grammar that matches one
matches nothing a well-formed document can contain. No grammar in the
third-party conformance corpus writes one.

## 2. Source span offsets count bytes

A span records where an element came from, in the units the front-end's
own engine tokens use. That is a UTF-16 code unit in TypeScript and a
byte in Go and Rust.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `a = "éé"\nb = "x"\n`, span of `b` | `s=9 e=10 r=2 c=1` | `s=11 e=12 r=2 c=1` | `s=11 e=12 r=2 c=1` |

**Reason.** The engine already records token positions this way, and the
front-end copies every field straight across precisely so that no
arithmetic, and so no off-by-one, happens at the boundary. Converting
would mean re-deriving a position the engine already knows.

**Owner.** Nobody: this is the engine's unit, recorded for it. Slicing
the original source with a span gives the same TEXT in every runtime,
which is what a consumer wants and what `rs/tests/spans_test.rs`
asserts. A consumer that treats a span as a UTF-16 offset is the case
this entry exists to warn.

## 3. Nested groups are refused sooner

A grammar arrives from outside the system, the parse tree nests once per
bracket, and a Rust stack that runs out ABORTS the process rather than
unwinding. Two caps apply, and the lower one is the shared compiler's.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `top = ( ( … "x" … ) )`, 127 deep | accepted | accepted | accepted |
| the same, 128 deep | accepted | accepted | refused: `abnf: rule 'top' nests elements more than 128 deep` |
| the same, 200 deep | accepted | accepted | refused, as above |
| the same, 5000 deep | refused: `RangeError: Maximum call stack size exceeded` | accepted | refused: `abnf: grammar nests too deeply (more than 2048 rule levels, about 500 nested groups or options)` |

**Reason.** The 128 limit is `MAX_ELEMENT_DEPTH` in `tabnas-bnf`, which
this crate inherits: the compiler's passes over an element are
recursive, and that port bounds them rather than trusting the stack. The
2048 limit is this crate's own, on the front-end, and stops a source
building a parse tree deep enough to overflow the stack on the way back
out. It admits about five hundred nested brackets, so the shared
compiler's limit is always the one a grammar meets first.

TypeScript raises a catchable `RangeError` several thousand levels
further on, and Go accepts every depth tried.

**Owner.** `tabnas-bnf` owns the 128 limit and records it in its own
`rs/README.md`. This crate owns the 2048 limit. Neither is reachable by
ABNF an author writes: the deepest grammar in the third-party corpus
nests nowhere near it.

## 4. A failure is returned, never raised

`parse_abnf`, `abnf_convert`, `abnf_compile` and `abnf` all answer a
`Result`. TypeScript throws `AbnfParseError` or the shared compiler's
own error, and Go returns as Rust does.

TypeScript also DECORATES an engine instance with a callable `tn.abnf`
member. Rust has no such mechanism, so the install path is the free
function `abnf(&mut parser, src, opts)` and the convert-only path is
`abnf_convert(src, opts)`, which is what `tn.abnf.toSpec` does.

**Reason.** Rust has no exceptions and no dynamic instance properties.

**Owner.** Nobody. The diagnostic TEXT is identical in all three
runtimes, which is the part that is a contract:
`test/spec/alignment-abnf-errors.tsv` compares 33 of them byte for byte
in every runtime, this one included.
