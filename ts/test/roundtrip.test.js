/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Round-trip: ABNF source -> compiled engine grammar -> ABNF source.
//
// @tabnas/debug renders a *live* engine's grammar back to ABNF. For the
// grammar shape the engine README uses, that render must reproduce the
// source it was compiled from, and the render must itself re-compile.
// Three converter behaviours are what make that hold:
//
//   1. `PL = "+"`     -> a named fixed token `#PL`, not a rule.
//   2. `NR = <number>` -> informational prose for a built-in token.
//   3. `val = add`    -> a pure alias is not inlined away.
//
// None of the three is pinned here any more. They are converter
// assertions that never touch a renderer, so under tabnas/abnf#73 they
// moved to `lifting.test.js`, which runs without @tabnas/debug, and most
// of them are now held across all three runtimes by the shared fixtures
// under `test/spec`. What is left in this file is the render itself.
//
// @tabnas/debug is a devDependency; this SKIPS when it is absent so the
// suite stays runnable outside the package (CI's `compose-debug` job can
// point TABNAS_DEBUG_PATH at a sibling checkout's built plugin).

const { describe, it } = require('node:test')
const assert = require('node:assert')
const Fs = require('node:fs')
const Path = require('node:path')

const { Tabnas } = require('@tabnas/parser')
const { abnf: abnfPlugin } = require('..')

const FIXTURE = Path.join(__dirname, 'grammar', 'addition.abnf')

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

// @tabnas/debug is a declared devDependency of THIS package, so inside the
// package it is always resolvable. This used to fall back to `skip`, which
// meant a missing sibling turned every case below into a green tick for a
// suite that never ran — the same defect as a swallowed failure. If the
// sibling really is absent, say so and fail.
if (!Debug) {
  throw new Error(
    '@tabnas/debug could not be resolved, so these tests would silently ' +
      'vanish. It is a devDependency of this package — run `npm install`, or ' +
      'clone the sibling repo and set TABNAS_DEBUG_PATH. This must NOT skip.',
  )
}
const skip = false

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


describe('addition.abnf fixture, rendered', () => {
  // The render half of the fixture's coverage. Everything about that
  // fixture which does NOT need a renderer is in `lifting.test.js`, which
  // runs without @tabnas/debug.
  const src = Fs.readFileSync(FIXTURE).toString()

  it('renders back to the fixture, comments and all stripped', { skip }, () => {
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf(src)
    tn.use(Debug, { print: false })
    assert.equal(tn.debug.model().abnf, GRAMMAR)
  })
})
