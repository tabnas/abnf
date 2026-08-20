/* Copyright (c) 2026 Richard Rodger, MIT License */

/*  conformance-compile.js — one corpus compile, in its own process.
 *
 *  Not a test (the `test/ ** / *.test.js` glob does not pick it up). It is
 *  the child half of ts/test/conformance.test.js.
 *
 *  WHY A CHILD PROCESS. Some real published ABNF grammars in the corpus do
 *  not terminate in this compiler — they grow the heap until V8 aborts. An
 *  in-process compile of one of those takes the WHOLE `node --test` run down
 *  with it, which would leave the conformance suite unrunnable and therefore
 *  unrun. Isolating each corpus compile turns "the compiler ran away" into an
 *  ordinary recorded outcome (`budget`) instead of a dead test process.
 *
 *  usage:  node --max-old-space-size=<MB> conformance-compile.js <file> [append]
 *  stdout: one JSON object —
 *    {ok:true,names:[...]}                  compiled
 *    {ok:false,rejected:true,error:"..."}   the converter said no
 *    {ok:false,rejected:false,error:"..."}  the converter FELL OVER
 *  A non-zero exit with no JSON means the budget was blown; the parent reads
 *  that as `budget`, which is a FAILURE to accept, never a skip.
 *
 *  WHY `rejected` IS ON THE WIRE. The parent's invalid-corpus half asks only
 *  "did it compile?", so without this flag every internal TypeError in that
 *  half counts as a correct rejection — the same laundering the in-process
 *  mutation loop had, one process boundary away. An Error does not survive
 *  serialisation, so the classification has to happen HERE, where the real
 *  exception object still exists, and travel as data.
 */
'use strict'

const fs = require('node:fs')

const { abnfConvert } = require('../dist/abnf')
const { AbnfParseError } = require('../dist/converter')
const { EmitError } = require('@tabnas/bnf')

// Must stay in step with `isRejection` in conformance.test.js.
const isRejection = (e) => e instanceof AbnfParseError || e instanceof EmitError

// Every name the compiled spec can reach: rules, fixed tokens, match tokens.
function specNames(spec) {
  const out = new Set()
  const add = (k) => out.add(String(k).replace(/^#/, '').toLowerCase())
  for (const k of Object.keys((spec && spec.rule) || {})) add(k)
  for (const k of Object.keys((spec && spec.options && spec.options.fixed && spec.options.fixed.token) || {})) add(k)
  for (const k of Object.keys((spec && spec.options && spec.options.match && spec.options.match.token) || {})) add(k)
  return [...out]
}

const [file, append] = process.argv.slice(2)

let src = fs.readFileSync(file, 'utf8')
if (append) {
  // RFC 5234 lines are CRLF-terminated.
  src = src.replace(/\r?\n*$/, '') + '\r\n' + append + '\r\n'
}

try {
  const spec = abnfConvert(src)
  process.stdout.write(JSON.stringify({ ok: true, names: specNames(spec) }))
} catch (e) {
  const name = (e && e.constructor && e.constructor.name) || 'Error'
  const msg = String((e && e.message) || e).split('\n')[0].slice(0, 160)
  process.stdout.write(
    JSON.stringify({
      ok: false,
      rejected: isRejection(e),
      error: isRejection(e) ? msg : name + ': ' + msg,
    }),
  )
}
