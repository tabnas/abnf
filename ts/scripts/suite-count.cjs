/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */
'use strict'

// Count the suites the unit pass registers, the way `node --test` counts
// them, without running a single test.
//
// AGENTS.md states that number in prose ("The other N suites finish in
// seconds"), and `doc-counts` in ts/test/docs.test.js holds the sentence
// to this script. Counting `describe(` with a grep is NOT enough: four of
// the suites are registered by `makeRunner().file()` in @tabnas/support,
// one per `test/spec/*.tsv` fixture, and parity.test.js therefore has no
// `describe` of its own at all. A grep reads 49 where node reads 53.
//
// How it works: `node:test` is replaced, for the duration of the load,
// with a stub whose `describe` counts the call and then runs its body so
// nested suites are reached, and whose `it`/`test` and hooks do nothing.
// The test files are then required. Only registration runs, so this is
// fast and cannot be affected by a failing assertion.
//
// `conformance.test.js` is excluded, because the unit pass skips it by
// pattern and a skipped top-level suite is not counted in node's total.

const Fs = require('node:fs')
const Path = require('node:path')
const Module = require('node:module')

const TEST_DIR = Path.join(__dirname, '..', 'test')
const EXCLUDE = 'conformance.test.js'


function testFiles(dir) {
  return Fs.readdirSync(dir)
    .filter((f) => f.endsWith('.test.js') && EXCLUDE !== f)
    .sort()
    .map((f) => Path.join(dir, f))
}


// The stub. `describe` and its `suite` alias count, then invoke the body
// so a nested suite is counted too. A skipped suite is NOT counted, which
// is what node does with `--test-skip-pattern`, so `describe.skip` counts
// nothing. Everything else is inert.
function makeStub(bump) {
  const noop = () => undefined
  noop.skip = noop
  noop.only = noop
  noop.todo = noop

  const describe = (name, opts, fn) => {
    bump()
    const body = 'function' === typeof opts ? opts : fn
    if ('function' === typeof body) {
      body()
    }
  }
  describe.skip = noop
  describe.todo = noop
  describe.only = describe

  return {
    describe,
    suite: describe,
    it: noop,
    test: noop,
    before: noop,
    after: noop,
    beforeEach: noop,
    afterEach: noop,
    mock: { fn: () => noop, method: noop, reset: noop, restoreAll: noop },
  }
}


// Load every file with `node:test` intercepted. The interception is on
// `Module._load` rather than on the module cache, because the files and
// the @tabnas/support runner reach it by both `node:test` and `test`.
function countSuites() {
  let count = 0
  const stub = makeStub(() => { count++ })
  const load = Module._load

  Module._load = function (request, parent, isMain) {
    if ('node:test' === request || 'test' === request) {
      return stub
    }
    return load.call(this, request, parent, isMain)
  }

  try {
    for (const file of testFiles(TEST_DIR)) {
      require(file)
    }
  }
  finally {
    Module._load = load
  }

  return count
}


module.exports = { countSuites, testFiles }

if (require.main === module) {
  process.stdout.write(String(countSuites()) + '\n')
}
