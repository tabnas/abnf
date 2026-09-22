# Divergences

Where a port produces a different result from the canonical TypeScript
in `ts/`, for the same input. Every row below was MEASURED, on
2026-09-21, by running the three implementations over the input in its
first column; nothing here is inferred from reading the source.

Each entry names who owns the repair. An entry that closes must be
deleted, and the test that pins it fails until it is, so this file
cannot quietly go stale.

The conformance dial is NOT a divergence. All three runtimes accept and
reject the same grammars in the third-party corpus, and every grammar
either compiler can finish emits byte-identical pure-data grammar text.
The figures are 48/52 valid, 5 fragments, and 611/661 invalid with 2
over budget in TypeScript and Go against 610/661 with 3 over budget in
Rust. The one file behind that difference,
`ex_abnf/test/resources/RFC5322.abnf`, is REJECTED by all three with the
same message; the Rust suite runs the unoptimised test profile and takes
161s over it where node takes 13s and `go test` 16s, so it exceeds the
shared 60s budget and is counted as over budget rather than as the
rejection it eventually is. That is cost, not behaviour, and
`AGENTS.md` records it under "Conformance, as measured" beside the
figures. Go's invalid figure used to be lower for a real reason; it was
re-measured on 2026-09-21 and now agrees.

## Where the divergences are pinned

There is no executable register in `test/spec` for these. Most are
invisible to a grammar-to-output fixture, which is what every file there
compares: two concern the shape of a value no fixture reads, one
concerns the API rather than any value, and one is about the parse tree
a compiled grammar builds rather than about the grammar. So each one is
pinned by a Rust test in `rs/tests/divergence_test.rs`, asserted in BOTH
directions: the behaviour recorded here, and the behaviour the canonical
runtime has, so a port that starts agreeing fails as loudly as one that
starts disagreeing.

**Both directions are RUN, not described.** The canonical half of every
entry below is measured by executing the canonical implementation:
`rs/tests/divergence_test.rs` starts `node` on `ts/dist/abnf.js` once per
run and compares each TypeScript cell in the tables below against what
comes back. All seven entries are measured that way, the surrogate,
span, nesting, API, probe, reversed-range and literal-size entries alike,
and every TypeScript cell in every table below has an assertion behind
it. Each one is written so that the CANONICAL behaviour changing is what
fails, and the failure names the entry, so an entry that closes from the
TypeScript side cannot go stale in prose.

The GO column is the exception, and it is the one kind of claim here
that can go stale without a test going red. No Rust test can run the Go
port, so each Go cell was measured by hand on the date at the top of this
file, by the procedure its entry names, and re-measuring it is a step a
maintainer repeats rather than something CI does. Where an entry is a
defect in the Go port rather than a divergence of this one, it says so
and names the function.

Strings cross that boundary as UTF-16 CODE UNITS rather than as text.
Three of these entries are about a value no well-formed string can
carry, and `JSON.parse` would repair a lone surrogate into U+FFFD on the
way across, which is the value under test.

The one thing that build needs is `ts/dist/abnf.js`, which `make
build-ts` produces and which is not committed. Without it the divergence
suite FAILS rather than skipping. `ABNF_CANONICAL=off` turns the
canonical half off for a checkout that genuinely cannot build the
canonical; `ci/rust/run.sh` sets it when the canonical is missing, and
prints a warning naming every entry it has therefore left half measured.
That warning is the only state in which a green Rust gate has not checked
the canonical side.

## 1. A numeric value naming a LONE surrogate

`%xD800` names one half of a UTF-16 surrogate pair with no other half
beside it. A JavaScript string can hold one; a Rust `String` and a Go
`string` cannot.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = %xD800` | fixed token `#T` is U+D800 | fixed token `#T` is U+FFFD | fixed token `#T` is U+FFFD |
| `g = %xDC00.D800` | the two halves, in that order, neither paired | two U+FFFD | two U+FFFD |
| `g = %xD800.0041` | U+D800 then `A` | U+FFFD then `A` | U+FFFD then `A` |
| `g = %xD800 %xDC00` | two tokens, U+D800 and U+DC00 | ONE token, U+FFFD | ONE token, U+FFFD |
| `g = %xD800-DFFF` | compiles to a character class | compiles to a character class | refused: `abnf: invalid regular expression for token #RX___UD800__UDFFF: ... class ... is not a valid Unicode scalar value` |

The fourth row is the same fact one step downstream: the two halves are
distinct strings in TypeScript and so get a token each, and they are the
same string once both are U+FFFD, so the emitter allocates one token for
both. The fifth is the same fact in a character class: the `regex`
crate's classes are over scalar values, and a range with no scalar value
in it at all cannot be built.

**Reason.** `String.fromCodePoint(0xD800)` answers a lone surrogate.
`char::from_u32(0xD800)` answers `None`, because a `char` is a Unicode
scalar value by definition, and `string(rune(0xD800))` in Go yields the
replacement character for the same reason. There is no representation to
port to.

**Owner.** Nobody, unless the notation gains a way to mean this. A
surrogate code point names no character, so a grammar that matches one
matches nothing a well-formed document can contain. No grammar in the
third-party conformance corpus writes one.

### An ADJACENT pair is not this entry, and Go gets it wrong

`%xD800.DC00` is a dotted concatenation, and the canonical runtime joins
its parts into one JavaScript string before anything asks what
characters that string holds. The two halves are then a well-formed
surrogate pair, which is the single character U+10000 and which every
runtime can represent. So this is not a divergence but a defect wherever
the answer differs.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = %xD800.DC00` | one character, U+10000, so the grammar matches `𐀀` | two U+FFFD, so `𐀀` is REJECTED and `��` accepted | one character, U+10000 |
| `g = %xD83D.DE00` | one character, U+1F600 | two U+FFFD | one character, U+1F600 |
| `g = %x41.D800.DC00.42` | `A` U+10000 `B` | `A` U+FFFD U+FFFD `B` | `A` U+10000 `B` |

**Provenance of each cell.** The TypeScript column was measured on
2026-09-21 by running `ts/dist/abnf.js` under node and reading the
charCodes of the element literal; the Rust column by
`rs/tests/abnf_test.rs`, which pins every row of this table; the Go
column by calling `ParseAbnf` from a throwaway test in `go/` on the same
day and printing the runes of `Literal`.

**Why no shared fixture row.** `test/spec/*.tsv` runs in all three
runtimes, so a row pinning U+10000 here would go red in Go. The Rust
behaviour is pinned by `an_adjacent_surrogate_pair_is_the_character_it_encodes`
in `rs/tests/abnf_test.rs` instead, and the canonical behaviour by the
entry 1 test in `rs/tests/divergence_test.rs`.

**Owner.** The Go port, in `parseNumericValue` in `go/converter.go`,
which calls `sb.WriteRune(rune(codePoint(n)))` once per dotted part and
so replaces each half before the two can pair. The repair is to build
the UTF-16 sequence for the whole concatenation and decode it once, as
`push_utf16` plus `String::from_utf16_lossy` do in `rs/src/numeric.rs`.
A pair split across a concatenation BOUNDARY (`%xD800 %xDC00`, two
elements) is two terms in every runtime and is row four of the table
above, not this one.

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

**Owner.** Nobody. The TEXT is the part that is a contract, and what is
under contract is the 35 diagnostics
`test/spec/alignment-abnf-errors.tsv` names: it compares each of them
byte for byte in every runtime, this one included. That is narrower than
"every diagnostic this crate writes". Entry 6 records the one class of
refusal whose wording this crate does not own.

The numeric-value diagnostic used to reach a source the fixture rows did
not: `g = %x110000` on one line and an unterminated string on the next
was answered by TypeScript and Rust with the out-of-range value and by
Go with the lexer's complaint about line two, measured 2026-09-21. That
was a defect in `go/converter.go`, which hung the deferred diagnostic on
the element it decoded and so had nowhere to keep it once the parse that
element belonged to was refused. Since tabnas/abnf#75 the recorder rides
in the parse's own meta, as `rs/src/parser_abnf.rs` keeps it beside the
parse, and the two sources are rows of
`test/spec/alignment-abnf-errors.tsv` rather than a sentence here.

## 5. A probe and retry keeps the node it built

An optional prefix whose vocabulary overlaps what follows it is resolved
with a probe and a retry pass. The canonical runtime discards whatever
the retried alternative built and answers an EMPTY node; this port keeps
it.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = [ user "@" ] host`, `user = 1*ALPHA`, `host = 1*ALPHA`, parsing `ab@cd` | `{rule: 'g', src: '', kids: []}` | `{rule: 'g', src: '', kids: []}` | `{rule: 'g', src: 'ab@cd', kids: [user 'ab', host 'cd']}` |
| the same grammar, parsing `abc` | `{rule: 'g', src: '', kids: []}` | `{rule: 'g', src: '', kids: []}` | `{rule: 'g', src: 'abc', kids: [host 'abc']}` |
| RFC 3986 `authority`, parsing `user@example.com` | `{rule: 'authority', src: '', kids: []}` | `{rule: 'authority', src: '', kids: []}` | the full tree, `userinfo` and `host` under it |

**Reason.** Not this crate. The `GrammarSpec` the three compilers emit
for each of those grammars is BYTE IDENTICAL, and the difference shows
with `builtins` both off, where the retry hooks are closures the shared
compiler registers, and on, where they are the engine's own `$`
builtins. What differs is what the engine does with the node across a
rewind.

Every runtime ACCEPTS and REJECTS the same inputs here, which is all
`ts/test/probe.test.js`, `go/probe_test.go` and `rs/tests/probe_test.rs`
assert, so this stayed invisible until the trees were compared.

**Owner.** The engine port at `../../parser/rs`, with the shared
compiler at `../../bnf/rs` as the other candidate. A consumer of this
crate reads the difference as a populated tree where the canonical
runtime gives an empty one, so it is recorded here until the engine
settles which answer is right.

## 6. A reversed numeric range is refused in the regex engine's words

`%x5A-41` names a range whose start is above its end. All three
compilers refuse it, and none of them writes the message: each hands the
pattern to its platform's regular expression engine and reports what
comes back.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = %x5A-41` | throws `Invalid regular expression: /^[\u005a-\u0041]/: Range out of order in character class` | returns `abnf: invalid regular expression: Compile(...): error parsing regexp: invalid character class range: ...` | returns `abnf: invalid regular expression for token #RX___U005A__U0041: regex parse error: ... invalid character class range, the start must be <= the end` |

**Reason.** The wording belongs to V8, to Go's `regexp` and to the
`regex` crate respectively. The shared fixtures pin the diagnostics this
crate writes, and this is not one of them.

Go used to PANIC here rather than return, out of the
`regexp.MustCompile` the shared compiler hands every character class
to. Since tabnas/abnf#72 the boundary in `go/bnf_alias.go` converts that
one panic into the error return above, pinned by
`go/numeric_range_test.go`; the `MustCompile` itself is in
`tabnas/bnf`'s `go/emit.go` and still wants to become `regexp.Compile`,
which would let the message name the token as the Rust one does.

**Owner.** Nobody, unless a front-end starts checking the bounds itself
before a pattern is built, which would give all three the same sentence.

## 7. A very long literal exceeds the regular expression size limit

A case-insensitive literal becomes one regular expression, and the
`regex` crate refuses to compile a pattern whose compiled form is larger
than its default ten megabyte budget. V8 has no such budget.

| input | TypeScript | Go | Rust |
|---|---|---|---|
| `g = "a…"`, 120000 characters | compiles | compiles | compiles |
| the same, 150000 characters | compiles | compiles | refused: `abnf: invalid regular expression for token #AAA…: Compiled regex exceeds size limit of 10485760 bytes.` |

**Reason.** Not this crate, and not the notation: the shared compiler
builds the pattern in `rs/src/emit.rs` and takes the crate's default
size limit, which `regex::RegexBuilder::size_limit` can raise.

**Owner.** `tabnas-bnf` at `../../bnf/rs`, which builds the pattern. No
grammar in the third-party conformance corpus writes a literal within
two orders of magnitude of this, and every literal shorter than the
limit compiles identically in all three runtimes.
