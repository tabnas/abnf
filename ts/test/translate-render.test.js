/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// The render, run: a grammar spec written as ABNF by the render the
// package carries (`translate().render`), through the `alchemy` command
// (tabnas/alchemy-cli), which this package does not depend on. Set
// TABNAS_ALCHEMY to the command's path to run these tests; without it
// they skip, as `roundtrip.test.js` does without @tabnas/debug.
//
// Each spec is one the compiler writes, changed where a test says, so
// what the render reads is a tree a host can hand it. The render writes
// what ABNF can say, and the text compiles back to the spec it was
// written from; the rest it refuses with TARGET_VALUE_UNREPRESENTABLE,
// naming what, and never writes a grammar that recognises another
// language.

const Assert = require('node:assert/strict')
const Fs = require('node:fs')
const Os = require('node:os')
const Path = require('node:path')
const { spawn } = require('node:child_process')
const { after, test } = require('node:test')

const { abnfCompile, translate } = require('..')

const ALCHEMY = process.env.TABNAS_ALCHEMY
const skip = ALCHEMY ? false : 'TABNAS_ALCHEMY does not name the alchemy command'

// Every transient task reports progress at least every 30 seconds
// (AGENTS.md), so the command runs asynchronously, and while a run is out
// a line names it and the time it has taken, every BEAT_MS. A run takes
// a second or two; LIMIT_MS only stops a command that has hung.
const BEAT_MS = 25000
const LIMIT_MS = 120000

const compileText = (src) => abnfCompile(src, { recognition: false, strict: true })
const compile = (src) => JSON.parse(compileText(src))

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

// The render over a spec: `{ text }` when it writes one, and the failure
// the command reports (`{ code, message }`) when it does not. `what` names
// the run in the progress line.
function render (what, spec) {
  if (null == dir) {
    dir = Fs.mkdtempSync(Path.join(Os.tmpdir(), 'abnf-render-'))
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
  const input = Path.join(dir, 'spec.json')
  Fs.writeFileSync(input, JSON.stringify(spec, null, 2))
  const now = { n: ++runs, what, start: Date.now() }
  current = now
  const run = spawn(ALCHEMY, ['run', Path.join(dir, 'render.alc'), input],
    { stdio: ['ignore', 'pipe', 'pipe'] })
  let stdout = ''
  let stderr = ''
  run.stdout.setEncoding('utf8').on('data', (d) => { stdout += d })
  run.stderr.setEncoding('utf8').on('data', (d) => { stderr += d })
  // At the limit the command is killed and its output closed, as
  // spawnSync's timeout closed it: a command that started others would
  // leave them holding the output open past the limit.
  let stopped = false
  const stop = setTimeout(() => {
    stopped = true
    run.kill('SIGKILL')
    run.stdout.destroy()
    run.stderr.destroy()
  }, LIMIT_MS)
  return new Promise((resolve) => {
    const done = (out) => {
      clearTimeout(stop)
      if (current === now) current = null
      resolve(out)
    }
    run.on('error', (e) => done({ code: 'NOT_RUN', message: String(e) }))
    run.on('close', (status, signal) => {
      if (0 === status) return done({ text: stdout })
      if (stopped) {
        return done({ code: 'STOPPED', message: `${what}: stopped after ${LIMIT_MS / 1000}s` })
      }
      try {
        done(JSON.parse(stderr))
      } catch (e) {
        done({ code: 'NOT_JSON', message: `${stderr} ${signal ?? ''}` })
      }
    })
  })
}

async function writes (src, text, change = (spec) => spec) {
  const out = await render(`the round trip of ${brief(src)}`, change(compile(src)))
  Assert.equal(out.text, text, JSON.stringify(out))
  Assert.equal(compileText(out.text), compileText(src))
}

async function refuses (src, change, what) {
  const out = await render(`the refusal ${brief(what)}`, change(compile(src)))
  Assert.equal(out.code, 'TARGET_VALUE_UNREPRESENTABLE', JSON.stringify(out))
  Assert.match(out.message, what)
}

const KEYWORD = 'start = "if" word\nword = 1*%x61-7A\n'
const CLASS = '#RX___U0061__U007A'
const alt = (spec, rule = 'start') => spec.rule[rule].open[0]

test('the render writes a compiled grammar back as the text it compiles from', { skip }, async () => {
  await writes(KEYWORD, KEYWORD)
})

// ABNF numeric values concatenate (`%xE9.3A9`), so a literal of several
// characters past ASCII is one terminal, each character a code point.
test('the render writes characters past ASCII side by side as one numeric value', { skip }, async () => {
  await writes('start = %xE9.3A9\n', 'start = %xE9.3A9\n')
  await writes('start = %xE9.3A9.62\n', 'start = %xE9.3A9.62\n')
})

// The engine reads `s` as a string of token names or as a list of them,
// one place each; the two are the same sequence.
test('the render reads the list form of the tokens an alternate matches', { skip }, async () => {
  await writes(KEYWORD, KEYWORD, (spec) => {
    alt(spec).s = ['#IF']
    return spec
  })
})

test('the render refuses a set of tokens at one place', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    alt(spec).s = ['#IF ' + CLASS]
    return spec
  }, /rule start has an alternate whose s holds, at one place, a set of tokens/)
})

test('the render refuses a rule name ABNF cannot spell, the empty name among them', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.rule[''] = spec.rule.word
    return spec
  }, /the rule "" has a name ABNF cannot spell/)
})

// A class pattern is one class: `[`, its members, and `]` at the end.
test('the render refuses a pattern that only begins with a class', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token[CLASS] = '@~/^[\\u0061-\\u007a]b/'
    return spec
  }, /the token #RX___U0061__U007A is the pattern .*, which is not one class the render reads/)
})

test('the render refuses a class whose flags change what it matches', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token[CLASS] = '@~/^[\\u0061-\\u007a]/i'
    return spec
  }, /whose flags \(i\) change what it matches/)
})

// A literal's pattern escapes every character a pattern reads otherwise;
// one that does not is a pattern, not a literal.
test('the render refuses a case-insensitive pattern that is not an escaped literal', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token['#IF'] = '@~/^if+/i'
    return spec
  }, /the token #IF is the pattern .*, which is not a literal the render reads/)
})

test('the render refuses the form that edits a rule already installed', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.rule.word.open = { alts: spec.rule.word.open, inject: { append: true } }
    return spec
  }, /rule word gives its open alternates as alts and an inject/)
})

test('the render refuses an error generator and an alternate modifier', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    alt(spec).e = '@stop'
    return spec
  }, /rule start has an alternate carrying e \(an error generator\)/)
  await refuses(KEYWORD, (spec) => {
    alt(spec).h = '@change'
    return spec
  }, /rule start has an alternate carrying h \(an alternate modifier\)/)
})

test('the render refuses a function reference where a rule or a count is due', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    alt(spec).p = '@choose'
    return spec
  }, /rule start has an alternate whose p is the function reference @choose/)
  await refuses(KEYWORD, (spec) => {
    alt(spec).b = '@back'
    return spec
  }, /rule start has an alternate whose b is not a count/)
})

// A repetition's loop guards and counts with `c` and `n`; anywhere else
// they would make an alternate conditional.
test('the render refuses a condition or a counter outside a repetition\'s loop', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    alt(spec).c = { 'n.rep': 0 }
    return spec
  }, /rule start carries a condition or a counter outside a repetition's loop/)
  await refuses(KEYWORD, (spec) => {
    alt(spec).n = { rep: 1 }
    return spec
  }, /rule start carries a condition or a counter outside a repetition's loop/)
})

// Go writes a spec's keys in name order and its match tokens' order in a
// list of its own, so the rules' order, which ranks the tokens and so
// decides which of two that both match wins, is gone.
test('the render refuses a spec that gives its match tokens\' order as a list', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.tokenOrder = Object.keys(spec.options.match.token)
    return spec
  }, /tokenOrder/)
})

// A class is matched by code points with `u` and by UTF-16 code units
// without it, which differ for a negated class, a class past U+FFFF and
// `[\s\S]`, and for a class holding a surrogate. Every compiler here gives
// a class the flags its text compiles back with, so a class whose flags
// are others is refused, and a class of other characters, matched alike
// either way, is written whatever its flags.
test('the render refuses a class whose flags are not the ones its text compiles back with', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token[CLASS] = '@~/^[\\s\\S]/'
    return spec
  }, /is the class @~\/\^\[\\s\\S\]\/, whose flags \(none\) change what it matches/)
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token[CLASS] = '@~/^[\\ud800-\\udbff]/u'
    return spec
  }, /whose flags \(u\) change what it matches/)
  await writes(KEYWORD, KEYWORD, (spec) => {
    spec.options.match.token[CLASS] = '@~/^[\\u0061-\\u007a]/u'
    return spec
  })
})

// GBNF's `.` compiles to `[\s\S]` with `u`, every code point, which ABNF
// writes as the range of them all.
test('the render writes the class of every code point as the range of them all', { skip }, async () => {
  const out = await render('the class of every code point', (() => {
    const spec = compile(KEYWORD)
    spec.options.match.token[CLASS] = '@~/^[\\s\\S]/u'
    return spec
  })())
  Assert.equal(out.text, 'start = "if" word\nword = 1*%x0-10FFFF\n', JSON.stringify(out))
})

// `meta.provenance` names the rules the compiler made; where a spec has
// none, the start wrapper `options.rule.start` names is read by its shape.
test('the render reads the start wrapper by its shape where the spec names no provenance', { skip }, async () => {
  await writes(KEYWORD, KEYWORD, (spec) => {
    delete spec.meta
    return spec
  })
})

// The compiler lays a contested class over the atoms of a partition, a
// token set named for the class; the render writes the class the name
// gives, so the set's tokens must be that class.
const SETS = 'start = a / b\na = %x61-7A\nb = %x30-39 / %x61-66\n'
test('the render refuses a token set whose tokens are not the class its name gives', { skip }, async () => {
  await writes(SETS, SETS)
  await refuses(SETS, (spec) => {
    spec.options.tokenSet[CLASS.slice(1)].push('#RX___U0030__U0039')
    return spec
  }, /the token set #RX___U0061__U007A lays tokens over its class \[\\u0061-\\u007a\] that match other characters than the class does/)
  await refuses(SETS, (spec) => {
    spec.options.tokenSet[CLASS.slice(1)].pop()
    return spec
  }, /the token set #RX___U0061__U007A lays tokens over its class/)
})

test('the render refuses a rule with no alternate to open with', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.rule.word.open = []
    spec.rule.word.close = []
    return spec
  }, /rule word has no alternate to open with/)
})

// A sequence compiles to a chain of steps, each one open alternate and at
// most one close alternate naming the next step.
test('the render refuses a sequence\'s step with more alternates than a step', { skip }, async () => {
  const SEQ = 'start = "a" w "b" w\nw = %x61-7A\n'
  await writes(SEQ, SEQ)
  await refuses(SEQ, (spec) => {
    const other = JSON.parse(JSON.stringify(spec.rule.start.open[0]))
    other.s = '#B'
    spec.rule.start.open.push(other)
    return spec
  }, /rule start replaces itself in its close as a sequence's step does, and has more alternates than a step/)
})

// A token named for a rule the compiler lifted is written as that rule,
// and two rules of one name would be one.
test('the render refuses a lifted rule whose name another rule holds', { skip }, async () => {
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token['#word'] = spec.options.match.token['#IF']
    delete spec.options.match.token['#IF']
    alt(spec).s = '#word'
    return spec
  }, /the token #word is written as a rule named word, the name of another rule the spec holds/)
})

// A dispatch's lookahead copies of one alternative consume and push the
// same and differ in what they look ahead at; alternates alike in every
// token are as many alternatives.
test('the render keeps alternatives that are alike', { skip }, async () => {
  await writes('start = "a" / "a"\n', 'start = "a" / "a"\n')
  await writes('start = "a" "b" / "a" "b"\n', 'start = "a" "b" / "a" "b"\n')
})

// The compiler reads a numeric value as a case-insensitive literal where
// it holds a letter of ASCII, with the folding the pattern has, and as an
// exact one where it holds none; so a case-insensitive literal past ASCII
// with no such letter would come back matching the one case it spells.
test('the render refuses a case-insensitive literal whose numeric value matches exactly', { skip }, async () => {
  const MIXED = 'start = %x41.E9 word\nword = 1*%x61-7A\n'
  await writes(MIXED, MIXED)
  await refuses(KEYWORD, (spec) => {
    spec.options.match.token['#IF'] = '@~/^\u00e9/i'
    return spec
  }, /the case-insensitive literal "\u00e9" holds a character past ASCII that may have cases and no ASCII letter/)
})

// A spec without `meta.provenance` is read by the names the compiler gives
// its helpers, `_gen<n>_<kind>`, each with the shape the compiler gives
// its kind. ABNF spells no name that begins with `_`, so a rule the author
// named `_general`, or named as a helper and shaped otherwise, is refused
// by its name rather than read as a helper or left out.
test('the render reads the compiler\'s helpers by name and shape where the spec names no provenance', { skip }, async () => {
  const noProvenance = (spec) => {
    delete spec.meta
    return spec
  }
  const HELPERS = 'start = ( "a" / "b" ) word [ "c" ] word *"d" word 1*"e" word 2*3"f" word\nword = %x61-7A\n'
  await writes(HELPERS, HELPERS, noProvenance)
  for (const name of ['_general', '_gen9_opt_x', '_gen9_plus_x']) {
    await refuses('start = word other\nword = %x61-7A\nother = "x" "y"\n',
      (spec) => JSON.parse(JSON.stringify(noProvenance(spec)).split('"other"').join(`"${name}"`)),
      new RegExp(`the rule "${name}" has a name ABNF cannot spell`))
  }
})

// Every grammar text compiles to a start wrapper closed by the end of the
// source, and that close lets the end come after what the lexer skips. A
// spec that starts at another rule, or at a wrapper without its close,
// would come back accepting what it refuses, so it is refused, with its
// provenance or without.
test('the render refuses a start that is not the wrapper every grammar text compiles to', { skip }, async () => {
  for (const provenance of [true, false]) {
    await refuses(KEYWORD, (spec) => {
      spec.rule.__start__.close = []
      if (!provenance) delete spec.meta
      return spec
    }, /the start rule __start__ is not the start wrapper every grammar text compiles to/)
  }
  await refuses(KEYWORD, (spec) => {
    spec.options.rule.start = 'start'
    return spec
  }, /the start rule start is not the start wrapper every grammar text compiles to/)
})
