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

    // The four above are the front-end's own — they are about the
    // COMMENT. The two below come from the compiler, and are here
    // because they are what an ABNF author actually hits, in ABNF's own
    // words. They reach the author through `abnfConvert`, not
    // `parseAbnf`: the comment is well-formed, the grammar is not.
    const convertFails = (src, re) =>
      assert.throws(() => abnf(src), (e) => {
        assert.match(e.message, /^abnf: /,
          'the diagnostic must name ABNF, not the compiler underneath')
        assert.match(e.message, re)
        return true
      })

    it('a leading member whose own rule builds a value', () => {
      // A rule's first reference is folded into it, which erases that
      // rule's builders — the member would hold an internal node. The
      // guide documents the fix (put a literal first); this pins that
      // the author is TOLD so rather than handed the node.
      convertFails(
        'top = inner "," x   ; @object inner x\n' +
        'inner = a "." b     ; @object a b\n' +
        'a = 1*DIGIT\nb = 1*DIGIT\nx = 1*DIGIT\n',
        /erases the value 'inner' is annotated to build/)
    })

    it('an UNANNOTATED rule that inlines an annotated one', () => {
      // The erasure needs a LEADING reference, not an annotated caller.
      // `top` names nothing, so nothing looked at it, and the grammar
      // compiled to an ordinary AST with `leaf`'s value nowhere in it.
      convertFails(
        'top = leaf ","\nleaf = d   ; @object d\nd = 1*DIGIT\n',
        /erases the value 'leaf' is annotated to build/)
    })

    it('nests through an annotated pure alias instead of refusing', () => {
      // Not a refusal — the opposite. A pure alias is the one caller
      // left-recursion elimination does not substitute into, so this
      // shape works and must not be caught by the rule above.
      assert.deepEqual(
        build('top = child   ; @object child\n' +
          'child = d   ; @object d\nd = 1*DIGIT\n', '7', 'top'),
        { child: { d: '7' } })
    })

    it('a group that is a part but cannot be named', () => {
      // A group produces a value, so it is a member and must be named —
      // but a member name has to be a rule name, and a group has none.
      // The shape is therefore un-annotatable today. Refusing is the
      // point: naming only `c` used to key the GROUP as `c` and then
      // overwrite it.
      convertFails(
        'top = ( a / b ) c   ; @object c\n' +
        'a = 1*DIGIT\nb = 1*ALPHA\nc = 1*DIGIT\n',
        /names 1 member but has 2 parts that produce a value/)
    })
  })

  // A repetition is ONE part, so its whole run is one element. This is
  // not the behaviour anyone wants from `; @array` on the ABNF list
  // idiom, and the guide says so — but it is the behaviour, and pinning
  // it means a change to it has to be deliberate rather than a surprise.
  it('takes a repetition as a single array element', () => {
    const src = 'list = item *( "," item )   ; @array\nitem = 1*DIGIT\n'
    assert.deepEqual(build(src, '1,2,3', 'list'), ['1', ',2,3'])
    assert.deepEqual(build(src, '1', 'list'), ['1', ''],
      'and an empty run is an empty-string element, not an absent one')
  })
})
