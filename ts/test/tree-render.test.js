/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// The render of the tree a host translates, run: a grammar-rules tree
// written as ABNF by the render the package carries (`translate().render`,
// alchemy/render.alc), through the `dmog` command (tabnas/dmog), which this
// package does not depend on. Set TABNAS_ALCHEMY to the command's path to
// run these tests; without it they skip, as `translate-render.test.js`
// does.
//
// The contract is the round trip: the tree of an ABNF document, written,
// reads back as that tree, and writes as the same text again. A tree of
// another notation is written as ABNF spells it, and what ABNF cannot say
// is refused with TARGET_VALUE_UNREPRESENTABLE, naming the rule and what
// it met, as is a tree that is not the schema's.

const Assert = require('node:assert/strict')
const Fs = require('node:fs')
const Os = require('node:os')
const Path = require('node:path')
const { spawn } = require('node:child_process')
const { after, test } = require('node:test')

const { findSpecDir } = require('@tabnas/support')

const { parseAbnfTree, translate } = require('..')

const ALCHEMY = process.env.TABNAS_ALCHEMY
const skip = ALCHEMY ? false : 'TABNAS_ALCHEMY does not name the alchemy command'

// Every transient task reports progress at least every 30 seconds
// (AGENTS.md), so the command runs asynchronously, and while a run is out
// a line names it and the time it has taken, every BEAT_MS. A run takes
// a second or two; LIMIT_MS only stops a command that has hung.
const BEAT_MS = 25000
const LIMIT_MS = 120000

// The most a run may write to either stream: 1 MiB, the default maxBuffer
// spawnSync had. A command that floods its output fails its case rather
// than filling the runner's heap.
const MAX_OUTPUT = 1024 * 1024

let dir = null
let runs = 0
let current = null
let beat = null
after(() => {
  if (beat) clearInterval(beat)
  if (dir) Fs.rmSync(dir, { recursive: true, force: true })
})

// A text short enough to name a run by, on one line.
function brief (text) {
  const s = String(text).replace(/\s+/g, ' ').trim()
  return 48 < s.length ? s.slice(0, 47) + '…' : s
}

// The render over a tree: `{ text }` when it writes one, and the failure
// the command reports (`{ code, message }`) when it does not. `what` names
// the run in the progress line.
function render (what, tree) {
  if (null == dir) {
    dir = Fs.mkdtempSync(Path.join(Os.tmpdir(), 'abnf-tree-render-'))
    Fs.writeFileSync(Path.join(dir, 'render.alc'),
      translate().render.source + '\ndef export [input] (abnf-render input)\n')
  }
  if (null == beat) {
    beat = setInterval(() => {
      if (current) {
        console.log(`render ${current.n}, ${current.what}: ` +
          `${Math.round((Date.now() - current.start) / 1000)}s so far, percentage unknown`)
      }
    }, BEAT_MS)
    beat.unref()
  }
  const input = Path.join(dir, 'tree.json')
  Fs.writeFileSync(input, JSON.stringify(tree, null, 2))
  const now = { n: ++runs, what, start: Date.now() }
  current = now
  const run = spawn(ALCHEMY, ['run', Path.join(dir, 'render.alc'), input],
    { stdio: ['ignore', 'pipe', 'pipe'] })
  // A run is stopped by killing the command and closing its output, as
  // spawnSync stopped one at its timeout and its maxBuffer: a command that
  // started others would leave them holding the output open.
  const kill = () => {
    run.kill('SIGKILL')
    run.stdout.destroy()
    run.stderr.destroy()
  }
  let flooded = false
  const collect = (stream) => {
    const chunks = []
    let bytes = 0
    stream.on('data', (d) => {
      chunks.push(d)
      bytes += d.length
      if (MAX_OUTPUT < bytes && !flooded) {
        flooded = true
        kill()
      }
    })
    return () => Buffer.concat(chunks).toString('utf8')
  }
  const stdout = collect(run.stdout)
  const stderr = collect(run.stderr)
  let stopped = false
  const stop = setTimeout(() => {
    stopped = true
    kill()
  }, LIMIT_MS)
  return new Promise((resolve) => {
    const done = (out) => {
      clearTimeout(stop)
      if (current === now) current = null
      resolve(out)
    }
    run.on('error', (e) => done({ code: 'NOT_RUN', message: String(e) }))
    run.on('close', (status, signal) => {
      if (flooded) {
        return done({ code: 'OUTPUT_LIMIT',
          message: `${what}: wrote more than ${MAX_OUTPUT / (1024 * 1024)} MiB` })
      }
      if (0 === status) return done({ text: stdout() })
      if (stopped) {
        return done({ code: 'STOPPED', message: `${what}: stopped after ${LIMIT_MS / 1000}s` })
      }
      try {
        done(JSON.parse(stderr()))
      } catch (e) {
        done({ code: 'NOT_JSON', message: `${stderr()} ${signal ?? ''}` })
      }
    })
  })
}

async function roundTrip (src) {
  const tree = parseAbnfTree(src)
  const out = await render(`the round trip of ${brief(src)}`, tree)
  Assert.ok(undefined !== out.text, JSON.stringify(out))
  Assert.equal(JSON.stringify(parseAbnfTree(out.text)), JSON.stringify(tree), out.text)
  const again = await render(`the text again of ${brief(src)}`, parseAbnfTree(out.text))
  Assert.equal(again.text, out.text)
  return out.text
}

async function writes (tree, text) {
  const out = await render(`the text of ${brief(JSON.stringify(tree))}`, tree)
  Assert.equal(out.text, text, JSON.stringify(out))
}

async function refuses (tree, what) {
  const out = await render(`the refusal ${brief(what)}`, tree)
  Assert.equal(out.code, 'TARGET_VALUE_UNREPRESENTABLE', JSON.stringify(out))
  Assert.match(out.message, what)
}

// A grammar of one rule `a`, its one alternative the elements given.
const one = (...elements) => ({
  type: 'grammar',
  rules: [{ type: 'rule', name: 'a', alternatives: [elements] }],
})
const ref = (name, repeat) => (repeat ? { type: 'reference', name, repeat } : { type: 'reference', name })

test('the render writes every fixture grammar back as its tree', { skip }, async () => {
  const sources = []
  for (const line of Fs.readFileSync(Path.join(findSpecDir(__dirname), 'tree.tsv'), 'utf8').split('\n').slice(1)) {
    if ('' === line || line.startsWith('#') || line.split('\t')[1].startsWith('ERROR:')) continue
    sources.push(line.split('\t')[0]
      .replace(/\\(.)/g, (m, c) => ({ n: '\n', r: '\r', t: '\t', '\\': '\\' })[c] ?? m))
  }
  const grammars = Path.join(__dirname, 'grammar')
  for (const file of Fs.readdirSync(grammars)) {
    sources.push(Fs.readFileSync(Path.join(grammars, file), 'utf8'))
  }
  for (const src of sources) await roundTrip(src)
})

test('the render writes a rule a line, its alternatives and elements spaced', { skip }, async () => {
  Assert.equal(await roundTrip('a = b c / d\na =/ e\n'), 'a = b c / d\na =/ e\n')
  Assert.equal(await roundTrip('a = / b /\n'), 'a = / b /\n')
  Assert.equal(await roundTrip(''), '')
})

test('the render writes ABNF\'s terminals as RFC 5234 spells them', { skip }, async () => {
  Assert.equal(await roundTrip('a = "x" %s"Y" %i"z" %d65 %x61-7A %b1.10 <prose>\n'),
      'a = "x" %s"Y" "z" %x41 %x61-7A %x01.02 <prose>\n')
  Assert.equal(await roundTrip('a = *b 1*c 2*3d *4e 5f 1g [ h ] *[ i ] *( j k )\n'),
    'a = *b 1*c 2*3d *4e 5f 1g [ h ] *[ i ] *( j k )\n')
})

test('the render writes another notation\'s elements as ABNF spells them', { skip }, async () => {
  await writes(one({ type: 'class', negated: false, members: [{ from: 97, to: 122 }, { from: 95, to: 95 }] }),
    'a = ( %x61-7A / %x5F )\n')
  await writes(one({ type: 'class', negated: false, members: [{ from: 48, to: 57 }], repeat: [{ min: 1 }] }),
    'a = 1*%x30-39\n')
  await writes(one({ type: 'any' }), 'a = %x00-10FFFF\n')
  await writes(one({ type: 'literal', text: 'a\nb', caseSensitive: true }), 'a = %x61.0A.62\n')
  await writes(one(ref('x', [{ min: 0, max: 1 }])), 'a = *1x\n')
  await writes(one(ref('x', [{ min: 1 }, { min: 0, max: 1 }])), 'a = [ 1*x ]\n')
  await writes(one(ref('x', [{ min: 0 }, { min: 2, max: 3 }])), 'a = 2*3( *x )\n')
  // A character past U+FFFF of no script with cases matches as it would
  // without regard to case, so its numeric value says the same.
  await writes(one({ type: 'literal', text: '\u{1F600}\n', caseSensitive: false }), 'a = %x1F600.0A\n')
})

test('the render refuses what ABNF cannot say, naming the rule', { skip }, async () => {
  await refuses(one({ type: 'token', negated: false, text: 'think' }), /the rule "a" holds the token terminal "think"/)
  await refuses(one({ type: 'class', negated: true, members: [{ from: 34, to: 34 }] }), /the rule "a" holds a negated class/)
  await refuses(one({ type: 'class', negated: false, members: [] }), /the rule "a" holds an empty class/)
  await refuses({ type: 'grammar', rules: [{ type: 'rule', name: 'a_b', alternatives: [[]] }] },
    /the rule "a_b" has a name ABNF cannot spell/)
  await refuses({ type: 'grammar', rules: [{ type: 'rule', name: '', alternatives: [[]] }] },
    /the empty name/)
  await refuses(one({ type: 'prose', text: 'a > b' }), /holds the prose "a > b"/)
  await refuses(one({ type: 'literal', text: 'a\n', caseSensitive: false }),
    /holds the case-insensitive literal "a\\n"/)
  // Deseret has cases: U+10400 and U+10428 are one letter's two.
  await refuses(one({ type: 'literal', text: '\u{10400}\n', caseSensitive: false }),
    /holds the case-insensitive literal/)
  // A name in angle brackets is a rule's only: ABNF reads it as prose
  // where an element is due.
  await refuses(one(ref('<all>')), /the rule "a" references "<all>", a name ABNF cannot spell/)
  await refuses(one(ref('')), /the rule "a" references the empty name/)
})

test('the render refuses a tree that is not the schema\'s', { skip }, async () => {
  await refuses({ type: 'rules', rules: [] }, /the root's type is not grammar/)
  await refuses(one({ name: 'x', type: 'reference' }), /first member is not its type/)
  await refuses(one({ type: 'thing' }), /an element of the type "thing", which no element has/)
  await refuses(one({ type: 'reference', name: 1 }), /a reference's name is not of its kind/)
  await refuses(one({ type: 'literal', text: 'x' }), /a literal with no caseSensitive/)
  await refuses(one({ type: 'range', to: 2, from: 1 }), /a range's from comes after what it precedes/)
  await refuses(one({ type: 'range', from: 2, to: 1 }), /a range whose to is below its from/)
  await refuses(one(ref('x', [{ min: 2, max: 1 }])), /a repetition whose max is below its min/)
})
