/* Copyright (c) 2026 Richard Rodger, MIT License */

/*  conformance.test.js — third-party ABNF conformance.
 *
 *  WHAT THIS MEASURES
 *  ------------------
 *  README claims: "Takes ABNF source — the RFC 5234 dialect". That is an
 *  unqualified claim on RFC 5234 (+ RFC 7405 %s/%i, which the README also
 *  advertises), so it is judged against the full language, not against a
 *  self-declared subset.
 *
 *  There is NO official IETF conformance corpus for RFC 5234 — unlike TOML
 *  (toml-test) or XML (the W3C suite). What exists is the ABNF that real
 *  RFCs publish, collected as test data by other ABNF implementations. The
 *  corpus here is four such collections pinned at exact commit SHAs by
 *  scripts/fetch-abnf-corpus.sh. It is NEVER committed (see .gitignore).
 *
 *  Classification of each corpus file (valid / invalid / fragment) lives in
 *  test/corpus/manifest.tsv and was produced by an INDEPENDENT third-party
 *  ABNF parser (npm `abnf` 5.0.4 == hildjj/node-abnf) — not by hand and not
 *  by this implementation. `fragment` files parse as ABNF but reference
 *  rules they do not define, so they are neither must-accept nor must-reject
 *  and are excluded from both halves (and counted, so the exclusion is
 *  visible).
 *
 *  BOTH HALVES ARE EXERCISED:
 *    - valid   -> must compile AND produce the right value (see below)
 *    - invalid -> must be REJECTED with an error
 *
 *  The must-fail half is expanded by mutation: test/corpus/mutations.tsv
 *  lists 13 mutation classes, each appending one line that violates a named
 *  RFC 5234 Appendix B production. Every mutant was confirmed rejected by
 *  the same third-party oracle before the class was admitted, so "invalid"
 *  is a third-party judgement, not ours.
 *
 *  THE VALUE ASSERTION. "It didn't throw" is the json5 mistake and is not
 *  acceptable. For a grammar COMPILER the produced value is the GrammarSpec,
 *  so every rulename the source declares (RFC 5234 `rule = rulename
 *  defined-as elements c-nl`) must be reachable in the compiled spec, as a
 *  rule, a fixed token or a match token. A compiler that silently drops
 *  productions fails here even though it "parsed fine".
 *
 *  THIS SUITE CANNOT SKIP. If the corpus is missing the tests FAIL LOUDLY
 *  with the command to fetch it. A conformance test that quietly does not
 *  run is worse than no test, because the green tick is a lie. `npm test`
 *  runs the fetch script via the `pretest` hook, so CI always has it.
 */
'use strict'

const { describe, it, before } = require('node:test')
const assert = require('node:assert')
const fs = require('node:fs')
const path = require('node:path')

const { abnfConvert } = require('../dist/abnf')

const REPO = path.join(__dirname, '..', '..')
const CORPUS = path.join(REPO, 'test', 'abnf-corpus')
const MANIFEST = path.join(REPO, 'test', 'corpus', 'manifest.tsv')
const MUTATIONS = path.join(REPO, 'test', 'corpus', 'mutations.tsv')

const FETCH_HINT =
  'ABNF conformance corpus missing at ' +
  CORPUS +
  '\n  Fetch it with:  ./scripts/fetch-abnf-corpus.sh' +
  '\n  (`npm test` does this for you via the pretest hook.)' +
  '\n  This test MUST NOT be skipped — a conformance run that silently does' +
  '\n  not happen is the exact defect this suite exists to prevent.'

function loadTSV(file) {
  const text = fs.readFileSync(file, 'utf8')
  return text
    .split(/\r?\n/)
    .slice(1)
    .filter((l) => l.trim() !== '')
    .map((l) => l.split('\t'))
}

// RFC 5234 s4:  rule = rulename defined-as elements c-nl
// A rulename starts in column 1; `=` or `=/` follows.
const RULE_DECL = /^([A-Za-z][A-Za-z0-9-]*)[ \t]*=\/?/gm

function declaredRules(src) {
  const out = new Set()
  let m
  RULE_DECL.lastIndex = 0
  while ((m = RULE_DECL.exec(src))) out.add(m[1].toLowerCase())
  return out
}

// Every name the compiled spec can reach: rules, fixed tokens, match tokens.
function specNames(spec) {
  const out = new Set()
  const add = (k) => out.add(String(k).replace(/^#/, '').toLowerCase())
  for (const k of Object.keys(spec.rule || {})) add(k)
  for (const k of Object.keys(spec.options?.fixed?.token || {})) add(k)
  for (const k of Object.keys(spec.options?.match?.token || {})) add(k)
  return out
}

function readCorpus(rel) {
  return fs.readFileSync(path.join(CORPUS, rel), 'utf8')
}

function applyMutation(base, append) {
  // RFC 5234 lines are CRLF-terminated.
  return base.replace(/\r?\n*$/, '') + '\r\n' + append + '\r\n'
}

describe('conformance: third-party ABNF corpus', () => {
  // --- corpus presence: fail loudly, never skip -----------------------
  it('the third-party corpus is present', () => {
    assert.ok(fs.existsSync(CORPUS), FETCH_HINT)
    const files = []
    const walk = (d) => {
      for (const e of fs.readdirSync(d)) {
        const p = path.join(d, e)
        if (fs.statSync(p).isDirectory()) walk(p)
        else if (p.endsWith('.abnf')) files.push(p)
      }
    }
    walk(CORPUS)
    assert.ok(files.length >= 60,
      `expected >=60 .abnf files in the corpus, found ${files.length}. ` + FETCH_HINT)
  })

  const manifest = fs.existsSync(MANIFEST) ? loadTSV(MANIFEST) : []
  const mutations = fs.existsSync(MUTATIONS) ? loadTSV(MUTATIONS) : []
  const VALID = manifest.filter((r) => r[1] === 'valid').map((r) => r[0])
  const INVALID = manifest.filter((r) => r[1] === 'invalid').map((r) => r[0])
  const FRAGMENT = manifest.filter((r) => r[1] === 'fragment').map((r) => r[0])

  it('the manifest and mutation table are non-degenerate', () => {
    assert.ok(VALID.length >= 50, `valid corpus too small: ${VALID.length}`)
    assert.ok(INVALID.length >= 10, `invalid corpus too small: ${INVALID.length}`)
    assert.ok(mutations.length >= 13, `mutation table too small: ${mutations.length}`)
    // Every manifest row must name a file that actually exists — a typo'd
    // path would otherwise silently shrink the corpus.
    for (const [rel] of manifest) {
      assert.ok(fs.existsSync(path.join(CORPUS, rel)),
        `manifest names a missing corpus file: ${rel}. ` + FETCH_HINT)
    }
    assert.equal(VALID.length + INVALID.length + FRAGMENT.length, manifest.length)
  })

  // --- half 1: valid grammars must compile AND produce the right value -
  describe('valid: must compile and yield every declared rule', () => {
    for (const rel of VALID) {
      it(rel, () => {
        const src = readCorpus(rel)
        const spec = abnfConvert(src) // throws => FAIL, and that is honest
        const names = specNames(spec)
        const missing = [...declaredRules(src)].filter((n) => !names.has(n))
        assert.deepEqual(missing, [],
          `compiled, but ${missing.length} declared rule(s) vanished from the ` +
          `GrammarSpec: ${missing.slice(0, 8).join(', ')}`)
      })
    }
  })

  // --- half 2a: grammars the oracle rejects must be rejected ----------
  describe('invalid: corpus grammars that are not RFC 5234', () => {
    for (const rel of INVALID) {
      it(rel, () => {
        assert.throws(() => abnfConvert(readCorpus(rel)),
          `accepted a grammar the third-party oracle rejects as non-RFC-5234`)
      })
    }
  })

  // --- half 2b: mutants must be rejected ------------------------------
  describe('invalid: mutants violating a named RFC 5234 production', () => {
    for (const [name, append, violates] of mutations) {
      it(`${name} — ${violates}`, () => {
        const leaked = []
        for (const rel of VALID) {
          const src = applyMutation(readCorpus(rel), append)
          let threw = false
          try {
            abnfConvert(src)
          } catch (e) {
            threw = true
          }
          if (!threw) leaked.push(rel)
        }
        assert.deepEqual(leaked, [],
          `${leaked.length}/${VALID.length} bases accepted the appended ` +
          `line \`${append}\`, which RFC 5234 cannot derive (${violates}). ` +
          `e.g. ${leaked.slice(0, 3).join(', ')}`)
      })
    }
  })

  // --- the dial: printed, and asserted against a recorded baseline ----
  // The numbers below are the TRUE Phase-1 baseline, observed by running
  // this suite. They are recorded so the figure cannot silently regress.
  // They are NOT a pass mark: the suite above is already red, deliberately.
  it('records the observed conformance rate (baseline pin)', () => {
    let vOk = 0
    for (const rel of VALID) {
      try {
        const spec = abnfConvert(readCorpus(rel))
        const names = specNames(spec)
        if ([...declaredRules(readCorpus(rel))].every((n) => names.has(n))) vOk++
      } catch (e) { /* counted as not-accepted */ }
    }
    let iOk = 0
    let iTotal = 0
    for (const rel of INVALID) {
      iTotal++
      try { abnfConvert(readCorpus(rel)) } catch (e) { iOk++ }
    }
    for (const rel of VALID) {
      const base = readCorpus(rel)
      for (const [, append] of mutations) {
        iTotal++
        try { abnfConvert(applyMutation(base, append)) } catch (e) { iOk++ }
      }
    }
    console.log(
      `\n  ABNF conformance dial (TS):` +
      `\n    valid   accepted+value-correct : ${vOk}/${VALID.length}` +
      `\n    invalid rejected               : ${iOk}/${iTotal}` +
      `\n    excluded fragments             : ${FRAGMENT.length}\n`)

    // Ratchet: these must never go DOWN. Raising them is a real fix.
    assert.ok(vOk >= 39, `valid-accepted regressed: ${vOk} < 39`)
    assert.ok(iOk >= 620, `invalid-rejected regressed: ${iOk} < 620`)
  })
})
