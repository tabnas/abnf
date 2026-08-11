/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Cross-runtime conformance, driven by the shared `test/spec/*.tsv`
// fixtures at the repo root (see ../../test/AGENTS.md).
//
// The fixture loader, the escape codec, the `ERROR:` contract and the row
// loop all come from @tabnas/support, whose Go half `go/parity_test.go`
// uses to run the SAME files — so the two implementations cannot drift
// without one of them going red, and neither can the two loaders. That is
// the check the repo previously lacked: `go/leftrec_test.go` and
// `go/rfc3986_test.go` mirror the TS suite by hand, which catches nothing
// when only one side changes.
//
// What is left here is only what is specific to abnf: four fixtures, each
// asserting a different thing about the same `grammar` column.

const Path = require('node:path')

const { Tabnas } = require('@tabnas/parser')
const { findSpecDir, makeRunner } = require('@tabnas/support')

const { abnf: abnfPlugin } = require('..')
const { abnf } = require('../dist/converter.js')

const SPEC = findSpecDir(__dirname)

// Every fixture's first column is an ABNF grammar, which is multi-line, so
// it always needs escape-decoding. Where the runner's own input column is
// something else, it is read explicitly.
const grammarOf = (row) => row.unescNamed('grammar')

const run = (file, options) =>
  makeRunner({ input: 'grammar', expected: 'expected', ...options })
    .file(Path.join(SPEC, file + '.tsv'))


// The grammar parses the input into the expected AST.
run('alignment-abnf-ast', {
  input: 'input',
  parse: (input, row) => {
    const tn = new Tabnas({ plugins: [abnfPlugin] })
    tn.abnf(grammarOf(row))
    return tn.parse(input)
  },
})


// The grammar declares the expected fixed tokens.
run('alignment-abnf-tokens', {
  parse: (grammar) => ({ ...abnf(grammar).options.fixed.token }),
})


// The grammar declares the expected rules, by name.
run('alignment-abnf-rules', {
  parse: (grammar) => Object.keys(abnf(grammar).rule).sort(),
})


// The grammar is rejected, with exactly this message.
run('alignment-abnf-errors', {
  parse: (grammar) => abnf(grammar),

  // abnf's `ERROR:` cells hold the whole MESSAGE, compared EXACTLY — not a
  // code, and not a substring. These rejections are the converter's own
  // diagnostics, several of them paragraphs that name the offending rule
  // and say what to write instead, and the wording is the thing under
  // test: a diagnostic that stops explaining itself is the regression
  // worth catching.
  matchError: (err, want) => String(err?.message) === want,
})
