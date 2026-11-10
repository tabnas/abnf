/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// The tree a host translates, `parseAbnfTree`: an ABNF document read as the
// schema grammar-rules (tabnas/bnf's docs/grammar-rules.md), its rules as
// the text writes them, uncompiled and in source order. `test/spec/tree.tsv`
// holds each grammar to its tree's JSON text, member order included, which
// the Go and Rust runners (`go/tree_test.go`, `rs/tests/tree_test.rs`) hold
// their ports to; the order is part of the contract, since a host streams
// the members in that order and the transducer part makes the same events
// from the rule passes of `make()`, the parser whose value the tree is.
//
// Repetition is replacement (AGENTS.md): the rules of a grammar, the
// alternatives of a rule or a group and the elements of an alternative add
// no rule depth however many there are.

const Assert = require('node:assert/strict')
const Fs = require('node:fs')
const Path = require('node:path')
const { test } = require('node:test')

const { findSpecDir, makeRunner } = require('@tabnas/support')
const { checkRules } = require('@tabnas/bnf')

const { make, parseAbnfTree, abnfTreeRules, abnfRules } = require('..')

const SPEC = findSpecDir(__dirname)

makeRunner({
  input: 'grammar',
  expected: 'expected',
  parse: (grammar) => {
    const tree = parseAbnfTree(grammar)
    checkRules(tree)
    return JSON.stringify(tree)
  },
  // The cell is the tree's JSON text, compared as text.
  parseExpected: (expected) => expected,
  // An `ERROR:<code>` cell holds the code the parse refused with.
  matchError: (err, want) => err?.cause?.code === want,
}).file(Path.join(SPEC, 'tree.tsv'))


// Every grammar of the fixtures and of this repository's grammars that the
// tree parser reads is a tree of the schema.
test('every tree the reader builds is a grammar-rules tree', () => {
  const sources = []
  for (const file of Fs.readdirSync(SPEC).filter((f) => f.endsWith('.tsv'))) {
    for (const line of Fs.readFileSync(Path.join(SPEC, file), 'utf8').split('\n').slice(1)) {
      if ('' === line || line.startsWith('#')) continue
      sources.push(line.split('\t')[0]
        .replace(/\\(.)/g, (m, c) => ({ n: '\n', r: '\r', t: '\t', '\\': '\\' })[c] ?? m))
    }
  }
  const grammars = Path.join(__dirname, 'grammar')
  for (const file of Fs.readdirSync(grammars)) {
    sources.push(Fs.readFileSync(Path.join(grammars, file), 'utf8'))
  }
  let read = 0
  for (const src of sources) {
    let tree
    try { tree = parseAbnfTree(src) } catch { continue }
    checkRules(tree)
    read++
  }
  Assert.ok(100 < read, `${read} trees read`)
})


// The tree parser reads with the meta-grammar's own alternates, so its rule
// passes are the meta-grammar's: only the actions differ.
test('the tree parser has the meta-grammar\'s alternates', () => {
  // An installed table's alternates are normalized in place (`s` and `g`
  // become lists), so each is read back to its written form.
  const text = (v) => (Array.isArray(v) ? v.join(' ') : v) || undefined
  const shape = (table) => JSON.stringify(Object.fromEntries(
    Object.entries(table).map(([name, spec]) => [name, ['open', 'close'].map((state) =>
      (spec[state] ?? []).map((alt) => ({
        s: text(alt.s), p: alt.p ?? undefined, r: alt.r ?? undefined, b: alt.b ?? undefined,
        g: text(alt.g), a: null != alt.a, c: null != alt.c,
      })))])))
  Assert.equal(shape(abnfTreeRules), shape(abnfRules))
})


test('make() builds a fresh parser whose value is the tree', () => {
  const a = make()
  const b = make()
  Assert.notEqual(a, b)
  Assert.deepEqual(a.parse('x = "y"\n'), parseAbnfTree('x = "y"\n'))
})


// The deepest rule a parse of `src` reaches.
function depth(src) {
  const tn = make()
  let deepest = 0
  tn.sub({
    ruleDone: (rule, _ctx, done) => {
      if (!done.probe && rule.d > deepest) deepest = rule.d
    },
  })
  tn.parse(src)
  return deepest
}

const N = 10000
const repeated = [
  ['rules of a grammar', (n) => Array.from({ length: n }, (_, i) => `r${i} = "x"\n`).join('')],
  ['alternatives of a rule', (n) => 'a = ' + Array.from({ length: n }, (_, i) => `r${i}`).join(' / ') + '\n'],
  ['elements of an alternative', (n) => 'a = ' + Array.from({ length: n }, (_, i) => `*r${i}`).join(' ') + '\n'],
  ['alternatives of a group', (n) => 'a = ( ' + Array.from({ length: n }, (_, i) => `%x${i.toString(16)}`).join(' / ') + ' )\n'],
  ['elements of an option', (n) => 'a = [ ' + '"x" '.repeat(n) + ']\n'],
]

for (const [what, doc] of repeated) {
  // Two items, since a list's first item can sit apart from its repeated
  // ones: the loop's own item is the second.
  test(`the ${what} add no rule depth: ${N} reach what two do`, () => {
    Assert.equal(depth(doc(N)), depth(doc(2)), what)
  })
}
