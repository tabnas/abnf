/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Value annotations carried in ABNF comments.
//
//   ver = maj "." min "." pat    ; @object maj min pat
//
// RFC 5234 has nowhere else to put this. A comment is the only place in
// the notation that carries no meaning of its own, which is exactly why
// it can carry one here without changing what the grammar accepts —
// strip every annotation and the same language parses, just into a tree
// instead of a value. These tests assert that both halves hold.

const { describe, it } = require('node:test')
const assert = require('node:assert')

const { Tabnas } = require('@tabnas/parser')
const { parseAbnf, abnfConvert: abnf, AbnfParseError } = require('..')

const build = (src, input, start) => {
  const j = new Tabnas()
  j.grammar(abnf(src, { start }))
  return j.parse(input)
}

const valueOf = (src, name) =>
  parseAbnf(src).productions.find((p) => p.name === name)?.value

describe('value annotations in comments', () => {
  const VER = 'ver = maj "." min "." pat   ; @object maj min pat\n' +
    'maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n'

  it('builds an object whose keys the input never spells', () => {
    assert.deepEqual(build(VER, '1.2.30', 'ver'),
      { maj: '1', min: '2', pat: '30' })
  })

  it('reads the annotation off the comment into the IR', () => {
    assert.deepEqual(valueOf(VER, 'ver'),
      { kind: 'object', members: ['maj', 'min', 'pat'] })
    assert.equal(valueOf(VER, 'maj'), undefined,
      'only the annotated rule carries one')
  })

  it('changes what the grammar BUILDS, never what it accepts', () => {
    // The point of putting this in a comment. Same source minus the
    // annotation must still parse the same inputs — it just produces the
    // tree it always did.
    const plain = VER.replace(/\s*;.*/, '')
    const accepts = (src, input) => {
      try { build(src, input, 'ver'); return true } catch { return false }
    }
    for (const input of ['1.2.30', '11.22.33', '1.2', 'x', '']) {
      assert.equal(accepts(VER, input), accepts(plain, input),
        `the annotation changed whether ${JSON.stringify(input)} parses`)
    }
    // ...and without it, the old tree comes back.
    const tree = build(plain, '1.2.30', 'ver')
    assert.equal(tree.rule, 'ver')
    assert.equal(tree.src, '1.2.30')
  })

  it('nests a member whose own rule is annotated', () => {
    const src = 'top = name "=" inner    ; @object name inner\n' +
      'name = 1*ALPHA\n' +
      'inner = maj "." min    ; @object maj min\n' +
      'maj = 1*DIGIT\nmin = 1*DIGIT\n'
    assert.deepEqual(build(src, 'ab=1.2', 'top'),
      { name: 'ab', inner: { maj: '1', min: '2' } })
  })

  it('builds an array, with each element taken from its text', () => {
    // `@array` names nothing: every part that produces a value becomes
    // an element, in order.
    const src = 'top = a "," b   ; @array\na = 1*DIGIT\nb = 1*DIGIT\n'
    assert.deepEqual(build(src, '1,2', 'top'), ['1', '2'])
  })

  it('attaches to the rule it FOLLOWS, not the line it sits on', () => {
    // A rule written across several lines, with the annotation on the
    // last of them, means the same thing.
    const src = 'ver = maj "." min\n' +
      '                        ; @object maj min\n' +
      'maj = 1*DIGIT\nmin = 1*DIGIT\n'
    assert.deepEqual(valueOf(src, 'ver'),
      { kind: 'object', members: ['maj', 'min'] })
  })

  it('leaves a `;` inside a quoted string alone', () => {
    // `";@object x"` is a LITERAL semicolon, not a comment — treating it
    // as one would attach an annotation the author never wrote.
    const src = 'top = sep 1*DIGIT\nsep = ";@object x"\n'
    assert.equal(valueOf(src, 'top'), undefined)
    assert.equal(valueOf(src, 'sep'), undefined)
    assert.ok(build(src, ';@object x1', 'top'), 'and it still parses')
  })

  it('leaves any other `; @…` comment alone', () => {
    // The notation has no directive namespace, so this must not assume
    // one: a reader's own `; @deprecated` has to keep meaning nothing.
    const src = 'top = 1*DIGIT   ; @deprecated use ver instead\n'
    assert.equal(valueOf(src, 'top'), undefined)
  })

  describe('refusals', () => {
    const fails = (src, re) =>
      assert.throws(() => parseAbnf(src), (e) => {
        assert.ok(e instanceof AbnfParseError, `wanted AbnfParseError, got ${e}`)
        assert.match(e.message, re)
        return true
      })

    it('an annotation before any rule', () => {
      fails('; @object a b\ntop = 1*DIGIT\n', /before any rule/)
    })

    it('two annotations on one rule', () => {
      fails('top = a "." b   ; @object a b\n                ; @object a b\n' +
        'a = 1*DIGIT\nb = 1*DIGIT\n', /more than one value annotation/)
    })

    it('`@array` naming members', () => {
      fails('top = a "," b   ; @array a b\na = 1*DIGIT\nb = 1*DIGIT\n',
        /'@array' names no members/)
    })

    it('a member name that is not a rule name', () => {
      fails('top = a "." b   ; @object a 9nope\na = 1*DIGIT\nb = 1*DIGIT\n',
        /is not a rule name/)
    })
  })
})
