/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

const { describe, it } = require('node:test')
const assert = require('node:assert')

const { Tabnas } = require('@tabnas/parser')
const { abnf: abnfPlugin, abnfConvert: abnf } = require('..')

// Overlapping character classes.
//
// The lexer produces ONE token per position and picks it by running the
// matchers the rule expects in allocation order, first match wins. So
// when two class tokens both cover a character, whichever was allocated
// first always won, and every alternative keyed on the other one was
// unreachable. Which alternative died depended only on the order the
// classes happened to be allocated in — which depends on the order the
// productions are visited, so the SAME language written two ways gave
// two different parsers.
//
// The compiler now lays overlapping classes over a shared partition of
// disjoint atoms and expresses each class as a token set over them, so
// there is nothing left for allocation order to decide.

function make(grammar) {
  const tn = new Tabnas({ plugins: [abnfPlugin] })
  tn.abnf(grammar)
  return tn
}

const accepts = (grammar, input) => {
  try {
    make(grammar).parse(input)
    return true
  } catch (e) {
    return false
  }
}

describe('overlapping character classes', () => {
  // The reduced case, both ways round. Before the partition, `DIGIT`
  // first accepted "8" and rejected "18"; `%x31-39` first did exactly
  // the reverse.
  const BOTH_ORDERS = [
    ['DIGIT first', 'top = c\nc = DIGIT / %x31-39 DIGIT\n'],
    ['%x31-39 first', 'top = c\nc = %x31-39 DIGIT / DIGIT\n'],
  ]

  for (const [label, grammar] of BOTH_ORDERS) {
    it(`accepts both widths, ${label}`, () => {
      assert.ok(accepts(grammar, '8'), 'one digit')
      assert.ok(accepts(grammar, '18'), 'two digits')
      assert.ok(!accepts(grammar, '08'), '08: the two-digit alt needs 1-9')
      assert.ok(!accepts(grammar, 'x'), 'not a digit at all')
    })
  }

  it('accepts the same strings whichever order the alternatives are in', () => {
    const [, a] = BOTH_ORDERS[0]
    const [, b] = BOTH_ORDERS[1]
    for (const input of ['0', '5', '9', '10', '42', '99', '08', 'x', '']) {
      assert.equal(
        accepts(a, input), accepts(b, input),
        `alternative order changed the verdict for ${JSON.stringify(input)}`,
      )
    }
  })

  it("RFC 3986's dec-octet accepts one- and two-digit octets", () => {
    // Verbatim from RFC 3986 Appendix A. Standing alone — outside the
    // `"." dec-octet` context that gave its alternatives distinguishing
    // two-token prefixes — every multi-digit octet used to be rejected.
    const G = 'top = dec-octet\n' +
      'dec-octet = DIGIT\n' +
      '          / %x31-39 DIGIT\n' +
      '          / "1" 2DIGIT\n' +
      '          / "2" %x30-34 DIGIT\n' +
      '          / "25" %x30-35\n'
    for (const v of ['0', '9', '10', '42', '99']) {
      assert.ok(accepts(G, v), `dec-octet should accept ${v}`)
    }
    for (const v of ['a', '1a']) {
      assert.ok(!accepts(G, v), `dec-octet should reject ${v}`)
    }
  })

  it('lays overlapping classes over disjoint atoms, and leaves the rest alone', () => {
    // %x30-39 and %x31-39 overlap, so the atoms are [0-0] and [1-9] and
    // both classes become sets over them — the second a one-member set,
    // so that its own token name (and every mark derived from it) stays
    // put whatever the partition does underneath. ALPHA overlaps nothing
    // and keeps the tokens it has always had.
    const spec = abnf('top = c\nc = DIGIT / %x31-39 DIGIT / ALPHA\n')
    const sets = spec.options.tokenSet ?? {}
    assert.deepEqual(
      Object.keys(sets).sort(),
      ['RX___U0030__U0039', 'RX___U0031__U0039'],
      'each overlapping class becomes a set over the atoms it covers',
    )
    assert.deepEqual(
      sets.RX___U0030__U0039,
      ['#RXA___U0030__U0030', '#RXA___U0031__U0039'],
    )
    assert.deepEqual(sets.RX___U0031__U0039, ['#RXA___U0031__U0039'])
    // Keyed WITHOUT the leading `#`: that is the only form both engines
    // resolve (TS falls back to the stripped name, Go trims it outright).
    for (const k of Object.keys(sets)) {
      assert.ok(!k.startsWith('#'), `set key ${k} must not carry a '#'`)
    }
    // The atoms are disjoint — that is the whole point, so assert it
    // rather than trusting the names.
    const src = (p) => String(p).replace(/^\/\^?|\/$/g, '')
    const spans = Object.entries(spec.options.match.token)
      .filter(([n]) => n.startsWith('#RXA'))
      .map(([, re]) => /\[\\u([0-9A-F]{4})-\\u([0-9A-F]{4})\]/.exec(src(re)))
      .filter(Boolean)
      .map((m) => [parseInt(m[1], 16), parseInt(m[2], 16)])
      .sort((x, y) => x[0] - y[0])
    for (let i = 1; i < spans.length; i++) {
      assert.ok(
        spans[i - 1][1] < spans[i][0],
        `class token spans overlap: ${JSON.stringify(spans)}`,
      )
    }
  })

  it('does not fan out alternatives whose heads are merely similar', () => {
    // `#HELLO` and `#HI` share an `h` but are distinct TOKENS, so the
    // dispatch was never in doubt and needs no lookahead help. Asking
    // the character question here doubled this rule's alternates.
    const spec = abnf('greeting = "hello" name / "hi" name\nname = TX\n')
    assert.equal(spec.rule.greeting.open.length, 2)
  })
})
