/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// A parse from a reader, a window of the source at a time (admin ADR-30;
// tabnas/parser's doc/chunked-source.md, "Proving it"), against the same
// text parsed whole. `make()`, the tree parser, declares `lex.window`: the
// meta-grammar's match tokens are read behind `abnf-reach`, a matcher that
// reads the window on until the unit that decides where their token ends
// lies inside it, and every other token is the engine's.
//
// Every row of the shared fixtures, every grammar of `test/grammar` and
// every document of the reference corpus (`test/abnf-corpus`, which
// `pretest` fetches) is parsed whole and from a reader that hands its bytes
// over in pieces of 1, 3 and 64 bytes, under the engine's window and under
// windows whose margin and block are a few units, so that every token meets
// the window's edge; a long document, whose long tokens of every kind run
// past the engine's window, in pieces of 4 KiB too. The value, the tokens
// the lexer gives, the rule passes, and an error's code, position, row,
// column and token must be the same. An error's excerpt is not compared: a
// parse from a reader shows a long line only as far back as the window
// reaches (tabnas/parser's DIVERGENCE.md).
//
// go/window_test.go and rs/tests/window_test.rs hold the Go and Rust ports
// to the same.

const Assert = require('node:assert/strict')
const Fs = require('node:fs')
const Path = require('node:path')
const { test } = require('node:test')

const { make } = require('..')

const ROOT = Path.join(__dirname, '..', '..')

// A reader that hands its bytes over at most `size` at a time.
function pieces (bytes, size) {
  let at = 0
  return {
    read (buf) {
      const n = Math.min(size, buf.length, bytes.length - at)
      buf.set(bytes.subarray(at, at + n), 0)
      at += n
      return n
    },
  }
}

// A read of a source: the pieces a reader hands its bytes over in, and the
// window it is read through, the engine's own (64 KiB) where `margin` and
// `block` are absent, or one whose margin and block are a few units, which
// the hidden `parseReaderWithSizes` sets, its reads as small as the pieces.
const WINDOWS = [{}, { margin: 1, block: 1 }, { margin: 3, block: 2 }, { margin: 64, block: 64 }]
const READS = [1, 3, 64].flatMap((size) => WINDOWS.map((window) => ({ size, ...window })))

// A long document in pieces of 1, 3, 64 and 4096 bytes through the
// engine's window, whose edges its long tokens straddle, and through a
// window of 64 and 64 units.
const LONG_READS = [
  { size: 1 },
  { size: 3 },
  { size: 64 },
  { size: 4096 },
  { size: 64, margin: 64, block: 64 },
]

// The tree parser with a log of the tokens the lexer gives and the rule
// passes.
function logged (log) {
  const tn = make()
  tn.sub({
    lex: (tkn) => {
      log.tokens.push(tkn.name + ' ' + JSON.stringify(tkn.src) + ' ' +
        JSON.stringify(tkn.val) + ' ' + tkn.sI + ' ' + tkn.rI + ':' + tkn.cI)
    },
    ruleDone: (rule, _ctx, done) => {
      log.rules.push(rule.name + ' ' + done.state + ' ' + rule.d)
    },
  })
  return tn
}

// What a parse showed: its value, or its error without the excerpt (`src`,
// the line it is on); and the tokens and rule passes `log` gathered, which
// is empty again after.
function seen (log, parse) {
  let outcome
  try {
    const value = parse()
    outcome = 'ok ' + (undefined === value ? 'undefined' : JSON.stringify(value))
  } catch (e) {
    if (null == e || 'function' !== typeof e.toJSON) {
      outcome = 'throw ' + String(e && e.message)
    } else {
      const { code, pos, row, col, token } = e.toJSON()
      outcome = 'err ' + JSON.stringify({ code, pos, row, col, token })
    }
  }
  const tokens = log.tokens
  const rules = log.rules
  log.tokens = []
  log.rules = []
  return { outcome, tokens, rules }
}

// `text` parsed whole and from a reader in each of `reads`; the count of
// reader parses compared.
function differential (tn, log, where, text, reads) {
  const bytes = new Uint8Array(Buffer.from(text, 'utf8'))
  const decoded = new TextDecoder('utf-8', { ignoreBOM: true }).decode(bytes)
  const whole = seen(log, () => tn.parse(decoded))
  for (const { size, margin, block } of reads) {
    const read = seen(log, () => undefined === margin
      ? tn.parseReader(pieces(bytes, size))
      : tn.parseReaderWithSizes(pieces(bytes, size), { margin, block, chunk: size }))
    const at = where + ' in pieces of ' + size +
      (undefined === margin ? '' : ', margin ' + margin + ', block ' + block)
    Assert.equal(read.outcome, whole.outcome, at)
    Assert.deepEqual(read.tokens, whole.tokens, at + ': the tokens')
    Assert.deepEqual(read.rules, whole.rules, at + ': the rule passes')
  }
  return reads.length
}

// The grammar cell of every row of every fixture file, its `\n`, `\r`, `\t`
// and `\\` the characters they name.
function rows () {
  const spec = Path.join(ROOT, 'test', 'spec')
  const out = []
  for (const file of Fs.readdirSync(spec).filter((f) => f.endsWith('.tsv')).sort()) {
    Fs.readFileSync(Path.join(spec, file), 'utf8').split('\n').forEach((line, i) => {
      if (0 === i || '' === line || line.startsWith('#')) return
      const grammar = line.split('\t')[0]
        .replace(/\\([nrt\\])/g, (_, c) => ({ n: '\n', r: '\r', t: '\t', '\\': '\\' })[c])
      out.push([file + ':' + (i + 1), grammar])
    })
  }
  return out
}

// Every file under `dir` whose name ends with `ext`, in a stable order.
function files (dir, ext, out = []) {
  for (const name of Fs.readdirSync(dir).sort()) {
    const path = Path.join(dir, name)
    if (Fs.statSync(path).isDirectory()) files(path, ext, out)
    else if (name.endsWith(ext)) out.push(path)
  }
  return out
}

// A grammar of every kind of token, the long ones longer than the engine's
// 64 KiB window, and characters past ASCII in its comments, so that tokens
// of every kind straddle the window's edges as a reader hands the text
// over. The same document as `long_document` in rs/tests/window_test.rs and
// `longDocument` in go/window_test.go.
function longDocument () {
  const parts = []
  parts.push('; ' + 'a comment past the window, é \u{1D11E} ; '.repeat(2000) + '\r\n')
  parts.push('long = <' + 'prose past the window '.repeat(3500) + '>\r\n')
  parts.push('quoted = "' + 'a string past the window '.repeat(3000) + '"\r\n')
  parts.push('named = ' + 'n'.repeat(70000) + '\r\n')
  parts.push('n'.repeat(70000) + ' = %x41\r\n')
  for (let i = 0; i < 4000; i++) {
    const hex = (n) => n.toString(16).toUpperCase()
    parts.push(`r${i} = %x${hex(0x41 + (i % 20))}-${hex(0x5A)} / %d${65 + (i % 26)}.66.67 / ` +
      `%b1000001 / %s"Ab${i}" / %i"cD" ; régle ${i}\r\n`)
    parts.push(`q${i} = 1*3( r${i} / "x" ) [ <prose ${i}> ] *q${i} 2r${i}\r\n    / %x30-39 2*"y"\r\n`)
    parts.push(`q${i} =/ "z" ; more\r\n`)
  }
  parts.push('end = "done"\r\n')
  return parts.join('')
}

test('make() reads a reader a window at a time', () => {
  Assert.equal(make().options.lex.window, true)
})

test('every fixture row parses the same from a reader', () => {
  const log = { tokens: [], rules: [] }
  const tn = logged(log)
  const all = rows()
  Assert.ok(150 < all.length, String(all.length))
  let compared = 0
  for (const [where, grammar] of all) {
    compared += differential(tn, log, where, grammar, READS)
  }
  Assert.equal(compared, all.length * READS.length)
})

test('every grammar and corpus document parses the same from a reader', () => {
  const log = { tokens: [], rules: [] }
  const tn = logged(log)
  const grammars = files(Path.join(__dirname, 'grammar'), '')
  const corpus = Path.join(ROOT, 'test', 'abnf-corpus')
  Assert.ok(Fs.existsSync(corpus), 'test/abnf-corpus is missing: sh test/fetch-abnf-corpus.sh')
  const documents = [...grammars, ...files(corpus, '.abnf')]
  Assert.ok(5 <= grammars.length && 68 <= documents.length - grammars.length,
    `${grammars.length} grammars, ${documents.length - grammars.length} corpus documents`)
  let compared = 0
  for (const path of documents) {
    compared += differential(tn, log, Path.relative(ROOT, path), Fs.readFileSync(path, 'utf8'), READS)
  }
  Assert.equal(compared, documents.length * READS.length)
})

test('a long document parses the same from a reader', () => {
  const log = { tokens: [], rules: [] }
  const tn = logged(log)
  const text = longDocument()
  Assert.ok(512 * 1024 < text.length, String(text.length))
  Assert.equal(make().parse(text).rules.length, 12005)
  Assert.equal(differential(tn, log, 'the long document', text, LONG_READS), LONG_READS.length)
})

test('a long document fails the same from a reader', () => {
  // A token that runs past the engine's window and is then refused, or
  // never ends: the error is the whole text's, code, place and token.
  const log = { tokens: [], rules: [] }
  const tn = logged(log)
  const pad = '; ' + 'padding ü '.repeat(9000) + '\r\na = "x"\r\n'
  const bad = [
    pad + 'b = <' + 'prose past the window '.repeat(3500) + '> )\r\n',
    pad + 'b = "' + 'unclosed string '.repeat(6000) + '\r\n',
    pad + 'b = ( c / ' + 'd '.repeat(40000) + '\r\n',
    pad + 'b = %x' + '4'.repeat(70000) + '\r\n',
    pad + 'b = %x62-61 ' + 'e '.repeat(40000) + '\r\n',
    pad + 'b = ' + 'f '.repeat(40000) + '%x41.D800\r\n',
    pad + 'b = ' + '9'.repeat(70000) + '*1x\r\n',
    pad + 'j'.repeat(70000) + ' = "g" ]\r\n',
  ]
  bad.forEach((text, index) => {
    Assert.throws(() => make().parse(text), `long error document ${index} parses`)
    differential(tn, log, `long error document ${index}`, text, LONG_READS)
  })
})

test('a long document parses from a reader in time linear in its length', () => {
  // Ten times the rules, read through the engine's window in pieces of 4
  // KiB, take about ten times as long, never the square of it.
  const tn = make()
  const document = (n) => Array.from({ length: n },
    (_, i) => `r${i} = %x41-5A / "b${i}" / <c ${i}> ; d\r\n`).join('')
  const timed = (text) => {
    const bytes = new Uint8Array(Buffer.from(text, 'utf8'))
    const started = process.hrtime.bigint()
    tn.parseReader(pieces(bytes, 4096))
    return Number(process.hrtime.bigint() - started) / 1e9
  }
  const small = document(2000)
  const large = document(20000)
  timed(small)
  const one = timed(small)
  const ten = timed(large)
  process.stderr.write(`window linear time: 2000 rules ${one.toFixed(3)}s, ` +
    `20000 rules ${ten.toFixed(3)}s, ${(ten / one).toFixed(1)} times\n`)
  Assert.ok(ten < 30 * one, `${one}s and ${ten}s`)
})
