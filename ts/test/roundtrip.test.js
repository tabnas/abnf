/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Round-trip: ABNF source -> compiled engine grammar -> ABNF source.
//
// @tabnas/debug renders a *live* engine's grammar back to ABNF. For the
// grammar shape the engine README uses, that render must reproduce the
// source it was compiled from, and the render must itself re-compile.
// Three converter behaviours are what make that hold, and each is
// pinned separately below:
//
//   1. `PL = "+"`     -> a named fixed token `#PL`, not a rule.
//   2. `NR = <number>` -> informational prose for a built-in token.
//   3. `val = add`    -> a pure alias is not inlined away.
//
// @tabnas/debug is a devDependency; this SKIPS when it is absent so the
// suite stays runnable outside the package (CI's `compose-debug` job can
// point TABNAS_DEBUG_PATH at a sibling checkout's built plugin).

const { describe, it } = require('node:test')
const assert = require('node:assert')

const { Tabnas } = require('@tabnas/parser')
const { abnf: abnfPlugin } = require('..')
const { abnf } = require('../dist/converter.js')

function loadDebug() {
  const candidates = [process.env.TABNAS_DEBUG_PATH, '@tabnas/debug'].filter(
    Boolean,
  )
  for (const c of candidates) {
    try {
      return require(c).Debug
    } catch {
      /* try next */
    }
  }
  return null
}

const Debug = loadDebug()
const skip = Debug ? false : '@tabnas/debug not available (set TABNAS_DEBUG_PATH)'

// The grammar shared with the @tabnas/parser README, verbatim — including
// the blank line that separates productions from token definitions.
const GRAMMAR = `val = add
add = NR [ PL add ]

NR = <number>
PL = "+"`

const render = (src) => {
  const tn = new Tabnas({ plugins: [abnfPlugin] })
  tn.abnf(src)
  tn.use(Debug, { print: false })
  return tn.debug.model().abnf
}


describe('round-trip', () => {
  it('renders the shared grammar back to its own source', { skip }, () => {
    assert.equal(render(GRAMMAR), GRAMMAR)
  })

  it('the rendered grammar re-compiles to the same rendering', { skip }, () => {
    // The real test of a round-trip: debug's output is valid input.
    assert.equal(render(render(GRAMMAR)), GRAMMAR)
  })

  it('matches the hand-written engine grammar', { skip }, () => {
    // The same grammar built directly against the engine — the version in
    // the @tabnas/parser README — must render to the same ABNF.
    const tn = new Tabnas()
    tn.grammar({
      options: {
        fixed: { token: { '#PL': '+' } },
        rule: { start: 'val' },
      },
      rule: {
        val: {
          open: [{ p: 'add', a: (r) => { r.node = 0 } }],
          close: [{}],
        },
        add: {
          open: [{ s: '#NR', a: (r) => { r.parent.node += r.o[0].val } }],
          close: [{ s: '#PL', r: 'add' }, {}],
        },
      },
    })
    tn.use(Debug, { print: false })
    assert.equal(tn.debug.model().abnf, GRAMMAR)
  })

  it('still parses, accumulating onto val', { skip }, () => {
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf(GRAMMAR, {
      actions: {
        '@add:o:NR': (r) => {
          let val = r
          while (val.parent && 'val' !== val.name) val = val.parent
          val.node.value = (val.node.value || 0) + Number(r.o[0].val)
        },
      },
    })
    assert.equal(tn.parse('1+2+3').value, 6)
    assert.equal(tn.parse('12+3+45').value, 60)
    assert.equal(tn.parse('7').value, 7)
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
