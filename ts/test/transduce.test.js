/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// The transducer part of an ABNF grammar: `abnfTransduce` converts the
// source and hands the spec to @tabnas/bnf's `transduceAlc`, which writes
// alchemy's `transduce-tree` with a spec read off the value actions the
// `; @object` and `; @array` annotations attach, or says why the grammar
// is read whole. @tabnas/bnf's own suite holds the part's events to the
// walk of the value; this holds the front-end to the compiler.

const { describe, it } = require('node:test')
const assert = require('node:assert')

const { abnfConvert, abnfTransduce, transduceAlc } = require('..')

// /etc/hosts, as aless's grammar library writes it.
const HOSTS =
  'hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array\n' +
  'entry   = address names                      ; @object address names\n' +
  'address = word\n' +
  'names   = 1*word                             ; @array\n' +
  'word    = ( TX )\n'

describe('transduce', () => {
  it('writes the part of an annotated grammar', () => {
    const part = abnfTransduce(HOSTS, 'hosts')
    assert.equal(part.entry, 'hosts-transduce')
    assert.equal(part.source, [
      '; The transducer part of the grammar hosts: the JSON events of the value',
      '; its @object and @array annotations build, from its rule passes, as',
      '; tabnas-bnf wrote them from the value actions those annotations attach.',
      'def hosts-transduce [input]',
      '  transduce-tree',
      '    record',
      '      entry :_gen6_plus_word (record (entry :text true))',
      '      entry :_gen6_star_word$alt0 (record (entry :text true))',
      '      entry :entry (record (entry :container :object) (entry :member "address") (entry :text true))',
      '      entry :entry$step1 (record (entry :container :object) (entry :member "names"))',
      '      entry :hosts (record (entry :container :array))',
      '      entry :hosts$step1 (record (entry :container :array))',
      '      entry :names (record (entry :container :array))',
      '    input',
      '',
    ].join('\n'))
  })

  it('is the compiler\'s part of the converted spec, with the options it converts with', () => {
    const opts = { wordKeywords: true, builtins: true }
    assert.deepEqual(abnfTransduce(HOSTS, 'hosts', opts), transduceAlc(abnfConvert(HOSTS, opts), 'hosts'))
  })

  it('reads a grammar whole whose start rule has no annotation', () => {
    assert.deepEqual(abnfTransduce('top = 1*DIGIT "." 1*DIGIT\n', 'g'), {
      whole: 'the start rule top has no annotation, so the value is the {rule, src, kids} tree, ' +
        'whose src comes before its kids and holds the text of every node beneath it: ' +
        'no stream writes the root before the document has ended',
    })
  })

  it('refuses a source that does not convert, as abnfConvert does', () => {
    assert.throws(() => abnfTransduce('top = = x\n', 'g'))
  })
})
