/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Converter behaviour that `roundtrip.test.js` used to carry: which
// literals lift to named fixed tokens and which stay rules, how prose is
// accepted and refused, and which aliases survive compilation.
//
// Split out under tabnas/abnf#73. Those assertions are about the compiled
// grammar and never touch a renderer, but they sat in a file that hard
// fails without @tabnas/debug, so they could not run where that sibling
// is absent and they had no twin in either port.
//
// Most of them are now ALSO pinned across all three runtimes by
// `test/spec/alignment-abnf-tokens.tsv`, `alignment-abnf-rules.tsv`,
// `alignment-abnf-ast.tsv` and `alignment-abnf-errors.tsv`, which is the
// stronger arrangement: a fixture row holds Go and Rust to the same
// answer by construction. What stays here is the reading of a single
// named field, which no fixture column reaches, and the repeated
// emission, which is not a grammar-to-output case at all.
//
// Both of those two ARE pinned in the ports, as per-runtime tests rather
// than fixture rows: `TestAstPureAliasSurvives` and `TestEmitIsRepeatable`
// in `go/abnf_test.go`, `ast_pure_alias_survives` and `emit_is_repeatable`
// in `rs/tests/abnf_test.rs`. Said here because a census taken over file
// names sees no `lifting` twin in either port and reads that as a gap.

const { describe, it } = require('node:test')
const assert = require('node:assert')
const Fs = require('node:fs')
const Path = require('node:path')

const { Tabnas } = require('@tabnas/parser')
const { abnf: abnfPlugin } = require('..')
const { abnf, parseAbnf, emitGrammarSpec } = require('../dist/converter.js')

const FIXTURE = Path.join(__dirname, 'grammar', 'addition.abnf')


describe('addition.abnf fixture', () => {
  // End-to-end over the on-disk fixture, as required for a dialect
  // extension (AGENTS.md: add a fixture grammar plus an end-to-end test).
  const src = Fs.readFileSync(FIXTURE).toString()

  it('compiles and parses from the fixture file', () => {
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf(src)
    assert.equal(tn.parse('1+2+3').rule, 'val')
    assert.equal(tn.parse('1+2+3').src, '1+2+3')
    assert.equal(tn.parse('7').src, '7')
  })

  it('compiles NR to the built-in token and PL to a named token', () => {
    const spec = abnf(src)
    assert.equal(spec.options.fixed.token['#PL'], '+')
    assert.equal(spec.rule.NR, undefined, 'prose line emits no rule')
    assert.equal(spec.rule.PL, undefined, 'lexical definition emits no rule')
    assert.ok(spec.rule.val, 'the pure alias survives as a rule')
  })

})


describe('single-literal productions lift to named tokens', () => {
  it('binds the production name as the token name', () => {
    const spec = abnf('add = NR [ PL add ]\nPL = "+"')
    assert.equal(spec.options.fixed.token['#PL'], '+')
    // ...and no `PL` rule is emitted; it is lexical, not syntactic.
    assert.equal(spec.rule.PL, undefined)
  })

  it('a multi-alternative production stays a rule', () => {
    const spec = abnf('e = NR sign NR\nsign = "+" / "-"')
    assert.ok(spec.rule.sign, 'sign is a choice, so it stays a rule')
    assert.equal(spec.options.fixed.token['#sign'], undefined)
  })

  it('the start rule is never lifted', () => {
    // It has to stay a rule for the grammar to have an entry point.
    const spec = abnf('greet = "hi"')
    assert.ok(spec.rule.greet)
  })

  it('an engine-owned token name is never claimed', () => {
    // Binding `#TX` to a literal would displace the lexer's text matcher.
    const spec = abnf('top = TX\nTX = "literal"')
    assert.ok(spec.rule.TX, 'TX stays a rule')
    assert.equal(spec.options.fixed.token['#TX'], undefined)
  })

  it('an empty literal is not a token', () => {
    // RFC 3986's `path-empty = ""` derives epsilon — no token can match it.
    const spec = abnf('p = "a" empty\nempty = ""')
    assert.ok(spec.rule.empty)
  })

  it('the production name wins over the literal-derived name', () => {
    // `"+"` has no word characters, so without a name it would be `#T`.
    const spec = abnf('e = plus NR\nplus = "+"')
    assert.equal(spec.options.fixed.token['#plus'], '+')
    assert.equal(spec.options.fixed.token['#T'], undefined)
  })

  it('emits the token even when nothing references it', () => {
    // The production is removed from the grammar, so if allocation only
    // walked `alts` the declaration would vanish without a trace.
    const spec = abnf('top = "x"\nPL = "+"')
    assert.equal(spec.options.fixed.token['#PL'], '+')
  })

  it('survives a second emission from the same parsed grammar', () => {
    // The emit pipeline rewrites the grammar, and lifting *removes* the
    // production — so without a defensive copy the second emission would
    // find no `PL` left and drop the token entirely.
    const ast = parseAbnf('top = "x"\nPL = "+"')
    const first = emitGrammarSpec(ast)
    const second = emitGrammarSpec(ast)
    assert.equal(first.options.fixed.token['#PL'], '+')
    assert.deepStrictEqual(
      second.options.fixed.token, first.options.fixed.token)
    assert.deepStrictEqual(
      Object.keys(second.rule).sort(), Object.keys(first.rule).sort())
  })

  it('lifts neither of two names sharing one literal', () => {
    // The engine keys fixed tokens by literal (cfg.fixed.token inverts to
    // src -> tin), so `+` can only ever be one token. Lifting either name
    // would drop the other; emitting both would collapse to a single tin
    // and leave rules expecting the loser permanently unmatchable.
    const spec = abnf('top = A B\nA = "+"\nB = "+"')
    assert.ok(spec.rule.A, 'A stays a rule')
    assert.ok(spec.rule.B, 'B stays a rule')
    assert.equal(spec.options.fixed.token['#A'], undefined)
    assert.equal(spec.options.fixed.token['#B'], undefined)

    // And the grammar still parses — one shared token, both rules live.
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf('top = A B\nA = "+"\nB = "+"')
    assert.equal(tn.parse('++').src, '++')
  })
})


describe('prose-val terminals', () => {
  it('accepts prose for a built-in lexer token', () => {
    const spec = abnf('add = NR [ PL add ]\nNR = <number>\nPL = "+"')
    // The prose line compiles to nothing; NR is the lexer's own token.
    assert.equal(spec.rule.NR, undefined)
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf('add = NR [ PL add ]\nNR = <number>\nPL = "+"')
    assert.equal(tn.parse('1+2').src, '1+2')
  })

  it('a leading prose line is not mistaken for the start rule', () => {
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf('NR = <number>\nadd = NR [ PL add ]\nPL = "+"')
    assert.equal(tn.parse('1+2').rule, 'add')
  })

  it('rejects prose for a name with no built-in behind it', () => {
    assert.throws(
      () => abnf('x = <foo>'),
      /rule 'x' is defined only by prose/,
    )
  })

  it('rejects prose used inside an expression', () => {
    assert.throws(
      () => abnf('y = "a" <foo>'),
      /rule 'y' uses prose .* inside an expression/,
    )
  })

  it('rejects prose nested in a group or repetition', () => {
    assert.throws(
      () => abnf('y = ( <foo> / "a" )'),
      /rule 'y' uses prose .* inside an expression/,
    )
    assert.throws(
      () => abnf('y = *<foo>'),
      /rule 'y' uses prose .* inside an expression/,
    )
  })

  it('rejects a grammar that is only prose', () => {
    assert.throws(
      () => abnf('NR = <number>'),
      /defines no rules/,
    )
  })
})


describe('pure aliases survive compilation', () => {
  it('does not inline a cycle-free alias', () => {
    const spec = abnf('val = add\nadd = NR [ PL add ]\nPL = "+"')
    // `val` pushes `add` rather than absorbing its body.
    assert.ok(
      spec.rule.val.open.some((a) => 'add' === a.p),
      'val should push add',
    )
  })

  it('still inlines an alias inside a leading-reference cycle', () => {
    // P = Q, Q = P a / b — Paull's substitution is doing real work here,
    // so the alias must still be inlined to expose the recursion.
    assert.doesNotThrow(() => abnf('P = Q\nQ = P PL / NR\nPL = "+"'))
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf('P = Q\nQ = P PL / NR\nPL = "+"')
    assert.equal(tn.parse('1++').src, '1++')
  })
})
