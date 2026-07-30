/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Cross-runtime conformance, driven by the shared `test/spec/*.tsv`
// fixtures at the repo root — the same convention @tabnas/parser uses (see
// ../../test/AGENTS.md).
//
// `go/parity_test.go` runs the SAME files, so the two implementations cannot
// drift without one of them going red. That is the check the repo previously
// lacked: `go/leftrec_test.go` and `go/rfc3986_test.go` mirror the TS suite
// by hand, which catches nothing when only one side changes.

const { describe, it } = require('node:test')
const assert = require('node:assert')
const Fs = require('node:fs')
const Path = require('node:path')

const { Tabnas } = require('@tabnas/parser')
const { abnf: abnfPlugin } = require('..')
const { abnf } = require('../dist/converter.js')

const SPEC = Path.join(__dirname, '..', '..', 'test', 'spec')

// Mirrors the loader in @tabnas/parser's ts/test/utility.js: skip the header
// row, split on tabs, unescape line breaks in every column. ABNF grammars are
// multi-line, so the `grammar` column relies on `\n` unescaping.
function unescape(str) {
  return str.replace(/\\r\\n|\\n|\\r/g, (m) =>
    m === '\\r\\n' ? '\r\n' : m === '\\n' ? '\n' : '\r')
}

function loadTSV(name) {
  const file = Path.join(SPEC, name + '.tsv')
  if (!Fs.existsSync(file)) throw new Error('spec file not found: ' + file)
  const lines = Fs.readFileSync(file, 'utf8').split(/\r?\n/).filter(Boolean)
  return lines.slice(1).map((line, i) => ({
    cols: line.split('\t').map(unescape),
    row: i + 2,
  }))
}

// A grammar is the case label; truncated so a failure names it readably.
const label = (g) => {
  const one = g.replace(/\n/g, ' ; ')
  return 60 < one.length ? one.slice(0, 57) + '...' : one
}


describe('spec: alignment-abnf-ast', () => {
  for (const { cols, row } of loadTSV('alignment-abnf-ast')) {
    const [grammar, input, expected] = cols
    it(`row ${row}: ${label(grammar)}`, () => {
      const tn = new Tabnas({ plugins: [abnfPlugin] })
      tn.abnf(grammar)
      assert.deepStrictEqual(
        JSON.parse(JSON.stringify(tn.parse(input))), JSON.parse(expected))
    })
  }
})


describe('spec: alignment-abnf-tokens', () => {
  for (const { cols, row } of loadTSV('alignment-abnf-tokens')) {
    const [grammar, expected] = cols
    it(`row ${row}: ${label(grammar)}`, () => {
      const spec = abnf(grammar)
      const fixed = {}
      for (const [k, v] of Object.entries(spec.options.fixed.token || {})) {
        fixed[k] = v
      }
      assert.deepStrictEqual(fixed, JSON.parse(expected))
    })
  }
})


describe('spec: alignment-abnf-rules', () => {
  for (const { cols, row } of loadTSV('alignment-abnf-rules')) {
    const [grammar, expected] = cols
    it(`row ${row}: ${label(grammar)}`, () => {
      const spec = abnf(grammar)
      assert.deepStrictEqual(Object.keys(spec.rule).sort(), JSON.parse(expected))
    })
  }
})


describe('spec: alignment-abnf-errors', () => {
  for (const { cols, row } of loadTSV('alignment-abnf-errors')) {
    const [grammar, expected] = cols
    it(`row ${row}: ${label(grammar)}`, () => {
      assert.ok(expected.startsWith('ERROR:'), 'expected column must be ERROR:…')
      const want = expected.slice('ERROR:'.length)
      assert.throws(
        () => abnf(grammar),
        (err) => err.message === want,
        `expected: ${want}`,
      )
    })
  }
})
