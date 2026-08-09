/* Copyright (c) 2026 Richard Rodger, MIT License */

/*  conformance.test.js — third-party ABNF conformance, TypeScript half.
 *
 *  WHAT IS MEASURED
 *  ----------------
 *  The README claims "ABNF source — the RFC 5234 dialect" (plus RFC 7405
 *  `%s`/`%i`). That is an unqualified claim on the whole language, so it is
 *  judged against the whole language, not a self-declared subset.
 *
 *  There is NO official IETF conformance corpus for RFC 5234 — unlike TOML
 *  (toml-test) or XML (the W3C suite). What exists is the ABNF that real
 *  RFCs publish, collected as test data by other ABNF implementations. The
 *  corpus is four such collections at pinned commit SHAs, fetched by
 *  test/fetch-abnf-corpus.sh. It is NEVER committed (see .gitignore).
 *
 *  Each corpus file's class (valid / invalid / fragment) lives in
 *  test/corpus/manifest.tsv and was decided by an INDEPENDENT third-party
 *  ABNF parser (npm `abnf` 5.0.4 == hildjj/node-abnf) — not by hand, and not
 *  by this implementation, which would be marking its own homework.
 *  Regenerate it with test/classify-abnf-corpus.sh. `fragment` files parse
 *  but reference rules they never define, so they are neither must-accept nor
 *  must-reject; they are excluded from both halves AND COUNTED, so the
 *  exclusion stays visible instead of quietly shrinking the corpus.
 *
 *  BOTH HALVES ARE EXERCISED:
 *    valid   -> must compile AND yield every declared rule (see below)
 *    invalid -> must be REJECTED
 *
 *  The must-fail half is widened by mutation: test/corpus/mutations.tsv lists
 *  13 classes, each appending one line that violates a named RFC 5234
 *  Appendix B production. Every one of the mutants was confirmed rejected by
 *  the same third-party oracle before its class was admitted, so "invalid" is
 *  a third-party judgement, not ours.
 *
 *  THE VALUE ASSERTION. "It didn't throw" is not a pass. For a grammar
 *  compiler the produced value is the GrammarSpec, so every rulename the
 *  source declares (RFC 5234 s4, `rule = rulename defined-as elements c-nl`)
 *  must be reachable in the compiled spec as a rule, a fixed token or a match
 *  token. A compiler that silently drops productions fails here.
 *
 *  THIS SUITE CANNOT SKIP. `npm test` fetches the corpus through the
 *  `pretest` hook, so CI always has it. If it is nonetheless absent, these
 *  tests FAIL, loudly, with the fetch command. A conformance run that quietly
 *  does not happen is worse than no test, because the green tick is a lie.
 *
 *  WHAT THE ASSERTIONS ARE. Not a ratchet and not a pass mark: the residual
 *  gaps are pinned as an EXACT SET, in test/corpus/known-gaps.tsv, shared
 *  with go/conformance_test.go. Fixing a gap fails the suite just as loudly
 *  as regressing one — the fix is to delete its row. That way the file always
 *  states what is true today, and neither runtime can drift without saying
 *  so. Never edit a row to silence a failure you did not fix.
 */
'use strict'

const { describe, it } = require('node:test')
const assert = require('node:assert')
const fs = require('node:fs')
const path = require('node:path')
const { spawnSync } = require('node:child_process')

const { abnfConvert } = require('../dist/abnf')

const TS_DIR = path.join(__dirname, '..')
const REPO = path.join(TS_DIR, '..')
const CORPUS = path.join(REPO, 'test', 'abnf-corpus')
const CORPUS_DIR = path.join(REPO, 'test', 'corpus')
const COMPILE = path.join(__dirname, 'conformance-compile.js')

// The budget a corpus grammar gets. Two real published grammars in the corpus
// (RFC 5322 email, Dhall) do not terminate in this compiler — they grow the
// heap until V8 aborts, in BOTH runtimes. Exceeding the budget is recorded as
// a failure to accept, and its knock-on exclusion from the mutation half is
// counted and pinned. It is never treated as a pass and never skipped.
const BUDGET_MB = 256
const BUDGET_MS = 60000

const FETCH_HINT =
  '\n  Fetch it with:  sh test/fetch-abnf-corpus.sh   (or: make abnf-corpus)' +
  '\n  `npm test` does this for you via the `pretest` hook.' +
  '\n  This test MUST NOT be skipped — a conformance run that silently does' +
  '\n  not happen is the exact defect this suite exists to prevent.'

function loadTSV(file) {
  return fs
    .readFileSync(file, 'utf8')
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
  return [...out]
}

function readCorpus(rel) {
  return fs.readFileSync(path.join(CORPUS, rel), 'utf8')
}

// One corpus compile, isolated and budgeted. Returns
//   {ok:true, names:[...]} | {ok:false, error} | {budget:true}
function compileBudgeted(rel, append) {
  const args = ['--max-old-space-size=' + BUDGET_MB, COMPILE, path.join(CORPUS, rel)]
  if (append) args.push(append)
  const r = spawnSync(process.execPath, args, {
    timeout: BUDGET_MS,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'ignore'],
  })
  try {
    return JSON.parse(r.stdout)
  } catch {
    // Aborted (heap) or killed (wall clock): the compiler did not finish.
    return { budget: true }
  }
}

const manifest = loadTSV(path.join(CORPUS_DIR, 'manifest.tsv'))
const mutations = loadTSV(path.join(CORPUS_DIR, 'mutations.tsv'))
const gapRows = loadTSV(path.join(CORPUS_DIR, 'known-gaps.tsv')).filter((r) => r[0] === 'ts')

const VALID = manifest.filter((r) => r[1] === 'valid').map((r) => r[0])
const INVALID = manifest.filter((r) => r[1] === 'invalid').map((r) => r[0])
const FRAGMENT = manifest.filter((r) => r[1] === 'fragment').map((r) => r[0])

const gapsOfKind = (kind) => gapRows.filter((r) => r[1] === kind).map((r) => r[2]).sort()
const PINNED_BUDGET = gapsOfKind('budget-exceeded')
const PINNED_VALID_GAPS = gapsOfKind('valid-not-accepted')
const PINNED_INVALID_GAPS = gapsOfKind('invalid-accepted')
const PINNED_MUTATION_LEAKS = Object.fromEntries(
  gapRows.filter((r) => r[1] === 'mutation-leak').map((r) => [r[2], Number(r[3])]),
)

// Recording mode, for the maintainer who has just changed the compiler:
//   ABNF_CONFORMANCE_RECORD=1 npm test -- ...
// prints the `ts` rows of known-gaps.tsv as measured, to paste back in.
// It does NOT write the file, and it does NOT weaken any assertion below.
const RECORD = process.env.ABNF_CONFORMANCE_RECORD === '1'
const recorded = []
const record = (kind, key, count, note) =>
  recorded.push(['ts', kind, key, String(count), note].join('\t'))

describe('conformance: third-party ABNF corpus', () => {
  it('the corpus is present and non-degenerate', () => {
    assert.ok(
      fs.existsSync(CORPUS),
      'ABNF conformance corpus missing at ' + CORPUS + FETCH_HINT,
    )
    const files = []
    const walk = (d) => {
      for (const e of fs.readdirSync(d)) {
        const p = path.join(d, e)
        if (fs.statSync(p).isDirectory()) walk(p)
        else if (p.endsWith('.abnf')) files.push(p)
      }
    }
    walk(CORPUS)
    assert.ok(
      files.length >= 60,
      `expected >=60 .abnf corpus files, found ${files.length}.` + FETCH_HINT,
    )
    assert.ok(VALID.length >= 50, `valid corpus too small: ${VALID.length}`)
    assert.ok(INVALID.length >= 10, `invalid corpus too small: ${INVALID.length}`)
    assert.ok(mutations.length >= 13, `mutation table too small: ${mutations.length}`)
    assert.equal(VALID.length + INVALID.length + FRAGMENT.length, manifest.length)
    // A typo'd manifest path would silently shrink the corpus.
    for (const [rel] of manifest) {
      assert.ok(
        fs.existsSync(path.join(CORPUS, rel)),
        `manifest names a corpus file that is not there: ${rel}.` + FETCH_HINT,
      )
    }
  })

  // --- half 1: valid grammars compile, and yield every declared rule ---
  const validGaps = []
  const overBudget = []

  it('valid: every declared rule survives into the GrammarSpec', () => {
    for (const rel of VALID) {
      const src = readCorpus(rel)
      const r = compileBudgeted(rel)
      if (r.budget) {
        overBudget.push(rel)
        validGaps.push(rel)
        if (RECORD) {
          record('budget-exceeded', rel, 1,
            `compiler did not finish within ${BUDGET_MB}MB / ${BUDGET_MS}ms`)
          record('valid-not-accepted', rel, 1, 'budget exceeded')
        }
        continue
      }
      if (!r.ok) {
        validGaps.push(rel)
        if (RECORD) record('valid-not-accepted', rel, 1, 'rejected: ' + r.error)
        continue
      }
      const names = new Set(r.names)
      const missing = declaredRules(src).filter((n) => !names.has(n))
      if (missing.length) {
        validGaps.push(rel)
        if (RECORD) {
          record('valid-not-accepted', rel, 1,
            'compiled, but declared rules vanished: ' + missing.slice(0, 6).join(','))
        }
      }
    }
    if (RECORD) return
    assert.deepEqual(
      validGaps.sort(), PINNED_VALID_GAPS,
      'the set of valid RFC 5234 grammars this compiler does not fully accept has ' +
        'changed. If you FIXED one, delete its row from test/corpus/known-gaps.tsv. ' +
        'If you BROKE one, that is a regression.',
    )
    assert.deepEqual(
      overBudget.sort(), PINNED_BUDGET,
      `the set of grammars the compiler cannot finish within ${BUDGET_MB}MB / ` +
        `${BUDGET_MS}ms has changed (see test/corpus/known-gaps.tsv).`,
    )
  })

  // --- half 2a: corpus grammars the oracle rejects must be rejected ----
  const invalidGaps = []

  it('invalid: grammars the third-party oracle rejects are rejected', () => {
    const leaked = invalidGaps
    for (const rel of INVALID) {
      const r = compileBudgeted(rel)
      if (r.ok) {
        leaked.push(rel)
        if (RECORD) record('invalid-accepted', rel, 1, 'accepted; oracle rejects it')
      }
    }
    if (RECORD) return
    assert.deepEqual(
      leaked.sort(), PINNED_INVALID_GAPS,
      'the set of non-RFC-5234 corpus grammars this compiler accepts has changed. ' +
        'If you FIXED one, delete its row from test/corpus/known-gaps.tsv.',
    )
  })

  // --- half 2b: mutants violating a named RFC 5234 production ----------
  //
  // Bases the compiler cannot finish at all are excluded here and ONLY here:
  // a mutant of a base that never compiles measures nothing about the
  // mutation. The exclusion is pinned by name above and counted below, so it
  // cannot quietly grow.
  const mutationLeaks = {}

  it('invalid: mutants violating a named RFC 5234 production are rejected', () => {
    const bases = VALID.filter((rel) => !overBudget.includes(rel))
    const leaks = mutationLeaks
    for (const [name, append] of mutations) {
      let n = 0
      for (const rel of bases) {
        const src = readCorpus(rel).replace(/\r?\n*$/, '') + '\r\n' + append + '\r\n'
        try {
          abnfConvert(src)
          n++
        } catch {
          /* rejected: correct */
        }
      }
      if (n > 0) {
        leaks[name] = n
        if (RECORD) {
          record('mutation-leak', name, n,
            `${n}/${bases.length} bases accepted \`${append}\``)
        }
      }
    }
    if (RECORD) return
    assert.deepEqual(
      leaks, PINNED_MUTATION_LEAKS,
      'the per-class mutation leak counts have changed. Each count is the number ' +
        'of corpus bases that accepted an appended line RFC 5234 cannot derive. ' +
        'Lower is better; update test/corpus/known-gaps.tsv when you improve one.',
    )
  })

  // --- the dial: what was actually measured, printed ------------------
  it('reports the measured conformance dial', () => {
    const bases = VALID.filter((rel) => !overBudget.includes(rel))
    const mutantTotal = bases.length * mutations.length
    const mutantLeaks = Object.values(mutationLeaks).reduce((a, b) => a + b, 0)
    const vOk = VALID.length - validGaps.length
    const iOk = INVALID.length - invalidGaps.length + (mutantTotal - mutantLeaks)
    const iTotal = INVALID.length + mutantTotal
    console.log(
      '\n  ABNF conformance dial (TS), as measured by this run:' +
        `\n    valid   accepted + value-correct : ${vOk}/${VALID.length}` +
        `\n    invalid rejected                 : ${iOk}/${iTotal}` +
        `\n    excluded fragments               : ${FRAGMENT.length}` +
        `\n    over budget (counted as failures): ${overBudget.length}\n`,
    )
    if (RECORD) {
      console.log('# paste the `ts` rows of test/corpus/known-gaps.tsv:')
      console.log(recorded.join('\n'))
      return
    }
    // The dial is derived from the pinned sets, so it cannot drift from them;
    // this only guards against the corpus itself being gutted.
    assert.ok(vOk > 0 && iOk > 0, 'the dial measured nothing')
  })
})
