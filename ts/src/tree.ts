/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

/*  tree.ts
 *  An ABNF document read as the schema `grammar-rules`: its rules as the
 *  text writes them, uncompiled and in source order (tabnas/bnf's
 *  docs/grammar-rules.md, and section 13.1 of alchemy's
 *  docs/streaming-plan.md). This is the tree a host translates: the part,
 *  alchemy/transduce.alc, makes the same tree's events from the rule
 *  passes of `make()`, and the render, alchemy/render.alc, writes it back
 *  as ABNF.
 *
 *    { "type": "grammar", "rules": [
 *      { "type": "rule", "name": "a", ["incremental": true],
 *        "alternatives": [ [ element, ... ], ... ] }, ... ] }
 *
 *  The tree parser reads ABNF with the meta-grammar `abnfRules` reads it
 *  with, its lexer and every alternate as they are, and actions of its own
 *  (TREE below) in place of the ones that build the compiler's IR, so the
 *  rule passes a part reads are the ones the meta-grammar makes. A numeric
 *  value (`%x61`, `%x61-7A`, `%x61.62`) is read by its atom's action, which
 *  gives its token the element it spells as its value, since a part reads
 *  a token's value and never the work an action does with it.
 *
 *  An element is an object whose first member names its type:
 *
 *    name            { type: 'reference', name }
 *    "ab", %i"ab"    { type: 'literal', text, caseSensitive: false }
 *    %s"ab", %x61.62 { type: 'literal', text, caseSensitive: true }
 *    %x61            { type: 'char', value }
 *    %x61-7A         { type: 'range', from, to }
 *    <a number>      { type: 'prose', text }
 *    ( ... )         { type: 'group', alternatives }
 *    [ ... ]         { type: 'group', alternatives, repeat: [{ min: 0, max: 1 }] }
 *
 *  and a repetition prefix (`*`, `m*`, `*n`, `m*n`, `n`) is the element's
 *  last member, `repeat`, written after the repetition an option is, so
 *  that `*[ a ]` is a group whose repeat is [{ min: 0, max: 1 }, { min: 0 }].
 *  A prefix is kept as the text writes it, `1a` and `1*1a` among them.
 *  Comments are not kept, and neither is a value annotation, which a
 *  comment carries; the core rules are the compiler's and are not added.
 *
 *  The tree holds what the schema allows, so the parser refuses what it
 *  cannot hold, each with a code of its own: a numeric value past
 *  U+10FFFF (abnf_code_point), a range that ends below where it starts
 *  (abnf_reversed_range), a repetition count past 2^53 - 1 (abnf_count),
 *  a repetition whose most is below its least (abnf_reversed_repetition),
 *  a group or an option left open (abnf_unclosed), and a concatenation
 *  holding half of a surrogate pair alone, which names no character
 *  (abnf_surrogate).
 */

import type { Rule } from '@tabnas/parser'

import {
  abnfRules,
  abnfOptions,
  installAbnfRules,
  NUMERIC_VALUE,
  AbnfParseError,
} from './converter'


// An element, a rule and a grammar of the schema, as this reader builds
// them: tabnas/bnf's `RulesElement`, `RulesRule` and `GrammarRules`.
type AbnfTreeRepeat = { min: number; max?: number }
type AbnfTreeElement = { type: string; [member: string]: any; repeat?: AbnfTreeRepeat[] }
type AbnfTreeRule = {
  type: 'rule'
  name: string
  incremental?: true
  alternatives: AbnfTreeElement[][]
}
type AbnfTree = { type: 'grammar'; rules: AbnfTreeRule[] }


// The codes the tree parser refuses a source with, beside the engine's:
// the `error` and `hint` options `make()` gives the parser.
const TREE_OPTIONS = {
  error: {
    abnf_code_point:
      'the numeric value {src} names no Unicode code point (the largest is %x10FFFF)',
    abnf_reversed_range:
      'the numeric range {src} ends below where it starts',
    abnf_reversed_repetition:
      'the repetition allows fewer than it requires: its most is below its least',
    abnf_count:
      'the repetition count {src} is past 9007199254740991, the largest whole number the tree holds',
    abnf_unclosed:
      'a group or an option is not closed',
    abnf_surrogate:
      'the numeric value {src} holds half of a surrogate pair standing alone, which names no character',
  },
  hint: {
    abnf_code_point:
      'A numeric value names a code point from %x0 to %x10FFFF.',
    abnf_reversed_range:
      'Write the range from its lower end to its higher, as %x61-7A.',
    abnf_reversed_repetition:
      'In m*n the n is the most and the m the least, so n is not below m.',
    abnf_count:
      'A repetition count is a whole number no larger than 9007199254740991.',
    abnf_unclosed:
      'Close the group with ) and the option with ].',
    abnf_surrogate:
      'A concatenation holds a surrogate as one half of a pair, as %xD83D.DE00; one value alone, as %xD800, is a code point of its own.',
  },
}


// The element a numeric value spells, or the code it is refused with.
// The digits are read as the IR parser's `parseNumericValue` reads them,
// the longest prefix valid in the stated base, so the tree and the
// compiler read one value alike.
function numericElement(src: string): AbnfTreeElement | string {
  const base = src[1].toLowerCase()
  const radix = 'x' === base ? 16 : 'd' === base ? 10 : 2
  const body = src.slice(2)
  const point = (text: string): number => {
    const n = parseInt(text, radix)
    return Number.isFinite(n) && 0 <= n && n <= 0x10ffff ? n : -1
  }

  if (body.includes('-')) {
    const [lo, hi] = body.split('-')
    const from = point(lo)
    const to = point(hi)
    if (from < 0 || to < 0) return 'abnf_code_point'
    if (to < from) return 'abnf_reversed_range'
    return { type: 'range', from, to }
  }

  const points = body.split('.').map(point)
  if (points.some((n) => n < 0)) return 'abnf_code_point'
  // One value is a character, its code point a surrogate's or not.
  if (1 === points.length) return { type: 'char', value: points[0] }
  // A concatenation's code points are one UTF-16 sequence, as the compiler
  // reads them, so a surrogate pair in it is the character it encodes; a
  // half standing alone names no character, and a literal's text is
  // characters. Built a point at a time: a concatenation can hold more
  // points than a call takes arguments.
  let text = ''
  for (const n of points) text += String.fromCodePoint(n)
  if (/\p{Cs}/u.test(text)) return 'abnf_surrogate'
  // A concatenation that spells one character (a surrogate pair's two
  // halves) is that character.
  const chars = [...text]
  if (1 === chars.length) return { type: 'char', value: chars[0].codePointAt(0) as number }
  return { type: 'literal', text, caseSensitive: true }
}


// A repetition count, read as the IR parser reads it.
const count = (tkn: any): number => parseInt(tkn.src, 10)

// The repetition a prefix's counts make, or the error token of the first
// count past what the tree holds exactly.
function repetition(r: Rule, min: number | null, max: number | null): any {
  for (const i of [0, 1, 2]) {
    const tkn = r.o[i]
    if (null != tkn && '#NUM' === tkn.name && !Number.isSafeInteger(count(tkn))) {
      return tkn.bad('abnf_count')
    }
  }
  return null == max ? { min } : { min, max }
}

// An action, as the meta-grammar's alternates and states hold one.
type Act = (r: Rule, ctx: any) => any

// What each action of `abnfRules` builds in the tree instead: the states'
// own (`bo`, `bc`), and the alternates', keyed by their `s` pattern, an
// alternate with none under ''. Every alternate that carries an action in
// `abnfRules` has one here, and no other.
const TREE: Record<string, {
  bo?: Act
  bc?: Act
  open?: Record<string, Act>
  close?: Record<string, Act>
}> = {
  abnf: {
    bo: (r) => { r.node = { type: 'grammar', rules: [] } },
  },

  prod: {
    open: {
      '#TX #DEF': (r) => { r.u.name = r.o[0].val },
      // `<all> = <remove>`: the name keeps its angle brackets.
      '#PV #DEF': (r) => { r.u.name = r.o[0].src },
      '#TX #DEFA': (r) => {
        r.u.name = r.o[0].val
        r.u.incremental = true
      },
    },
    bc: (r) => {
      if (r.child && undefined !== r.child.node) {
        const rule: any = { type: 'rule', name: r.u.name }
        if (r.u.incremental) rule.incremental = true
        rule.alternatives = r.child.node
        r.node.rules.push(rule)
      }
    },
  },

  alts: {
    bo: (r) => { r.node = [] },
    bc: (r) => {
      if (r.child && undefined !== r.child.node) {
        r.node.push(r.child.node)
      }
    },
  },

  seq: {
    bo: (r) => { r.node = [] },
  },

  elem: {
    open: {
      '#NUM #STAR #NUM #ATOM': (r) => {
        const rep = repetition(r, count(r.o[0]), count(r.o[2]))
        if (rep.isToken) return rep
        if (rep.max < rep.min) return r.o[0].bad('abnf_reversed_repetition')
        r.u.repeat = rep
      },
      '#NUM #STAR #ATOM': (r) => {
        const rep = repetition(r, count(r.o[0]), null)
        if (rep.isToken) return rep
        r.u.repeat = rep
      },
      '#STAR #NUM #ATOM': (r) => {
        const rep = repetition(r, 0, count(r.o[1]))
        if (rep.isToken) return rep
        r.u.repeat = rep
      },
      '#STAR #ATOM': (r) => { r.u.repeat = { min: 0 } },
      '#NUM #ATOM': (r) => {
        const rep = repetition(r, count(r.o[0]), count(r.o[0]))
        if (rep.isToken) return rep
        r.u.repeat = rep
      },
    },
    close: {
      '': (r, ctx) => {
        const el = r.child.node
        // An atom that built nothing is a group or an option the source
        // ended inside.
        if (null == el) return ctx.t0.bad('abnf_unclosed')
        if (r.u.repeat) {
          if (el.repeat) el.repeat.push(r.u.repeat)
          else el.repeat = [r.u.repeat]
        }
        r.node.push(el)
      },
    },
  },

  atom: {
    bo: (r) => { r.node = undefined },
    open: {
      '#SS #ST': (r) => {
        r.node = { type: 'literal', text: r.o[1].val, caseSensitive: true }
      },
      '#SI #ST': (r) => {
        r.node = { type: 'literal', text: r.o[1].val, caseSensitive: false }
      },
      '#ST': (r) => {
        r.node = { type: 'literal', text: r.o[0].val, caseSensitive: false }
      },
      // The element the value spells, which its token carries from here.
      '#NV': (r) => {
        const read = numericElement(String(r.o[0].src))
        if ('string' === typeof read) return r.o[0].bad(read)
        r.o[0].val = read
        r.node = { ...read }
      },
      '#PV': (r) => {
        r.node = { type: 'prose', text: String(r.o[0].src).slice(1, -1) }
      },
      '#TX': (r) => { r.node = { type: 'reference', name: r.o[0].val } },
      '#LP': (r) => { r.u.groupKind = 'group' },
      '#OB': (r) => { r.u.groupKind = 'opt' },
    },
    close: {
      '#RP': (r) => {
        r.node = { type: 'group', alternatives: r.child.node }
      },
      '#CB': (r) => {
        r.node = {
          type: 'group',
          alternatives: r.child.node,
          repeat: [{ min: 0, max: 1 }],
        }
      },
    },
  },
}


// The meta-grammar's rule table with the tree's actions: every alternate
// of `abnfRules`, its `s`, `p`, `r`, `b`, `g` and conditions as they are,
// and each action replaced by the one TREE gives it.
function treeRules(): typeof abnfRules {
  const table: any = {}
  let used = 0
  for (const name of Object.keys(abnfRules)) {
    const ir = abnfRules[name]
    const tree = TREE[name] ?? {}
    const bind = (alts: any[] | undefined, acts: Record<string, Act> | undefined,
      state: string) =>
      alts?.map((alt) => {
        const { a, ...rest } = alt
        if (null == a) return rest
        const act = acts?.[alt.s ?? '']
        if (null == act) {
          throw new Error(`abnf tree: ${name}.${state} has no action for ${alt.s ?? '(no s)'}`)
        }
        used++
        return { ...rest, a: act }
      })
    table[name] = {
      bo: tree.bo,
      bc: tree.bc,
      open: bind(ir.open, tree.open, 'open'),
      close: bind(ir.close, tree.close, 'close'),
    }
  }
  const declared = Object.values(TREE).reduce(
    (n, t) => n + Object.keys(t.open ?? {}).length + Object.keys(t.close ?? {}).length, 0)
  if (used !== declared) {
    throw new Error(`abnf tree: ${declared} actions declared, ${used} bound`)
  }
  return table
}

const abnfTreeRules = treeRules()


// Where ABNF's own matchers stop reading, in `w`, the text from the cursor:
// the index of the unit that decides how far their token runs, past which
// none of them reads. A numeric value and the `%s` and `%i` prefixes read
// the run of a base letter, hex digits, `.` and `-` that follows `%`, and
// the unit after it; prose reads to the first unit no prose holds, `>`
// among them; a repetition count reads its digits and the unit after.
// Every other token is the engine's, whose matchers read the window
// themselves.
function abnfReach(w: string): number {
  const c = w[0]
  const run =
    '%' === c ? /^%[A-Za-z]?[0-9A-Fa-f.-]*/.exec(w)
      : '<' === c ? /^<[\x20-\x3D\x3F-\x7E]*/.exec(w)
        : c >= '0' && c <= '9' ? /^[0-9]*/.exec(w)
          : null
  return null == run ? 0 : run[0].length
}


// A lexer matcher that runs before every other one and cuts no token: in a
// parse from a reader a window at a time (`lex.window`, admin ADR-30), the
// text after the cursor is the window's, which may end inside a token, so
// that a match token's expression would match a shorter token, or none,
// where the whole text has a longer one. It reads the window on, twice as
// far each time, until the unit that decides where the token ends lies
// inside it (`reach`) or the source has ended, so that every matcher after
// it sees what the whole text shows it. A whole text is all read.
function reachMatcher(reach: (w: string) => number): any {
  return {
    order: 9e5,
    make: () => (lex: any) => {
      if (!lex.windowOpen) return undefined
      let ahead = 1
      for (;;) {
        lex.fill(ahead)
        const w: string = lex.window()
        const decides = reach(w)
        if (decides < w.length || !lex.windowOpen) return undefined
        ahead = Math.max(decides + 1, 2 * w.length)
      }
    },
  }
}


// The engine options the tree parser reads with: the meta-grammar's, the
// codes above, and the window. The tree parser reads a source a window at
// a time when it is parsed from a reader: its matchers read the text only
// through the window, the meta-grammar's match tokens behind `abnfReach`.
function treeOptions(): any {
  const options = abnfOptions(NUMERIC_VALUE)
  options.error = { ...TREE_OPTIONS.error }
  options.hint = { ...TREE_OPTIONS.hint }
  options.lex = { window: true, match: { 'abnf-reach': reachMatcher(abnfReach) } }
  return options
}


// A fresh parser that reads an ABNF document as the tree: the parser a
// host streams a document through with the part, whose rule passes the
// part reads, and whose value is the tree.
function make(): any {
  const { Tabnas } = require('@tabnas/parser')
  const j = new Tabnas(treeOptions())
  installAbnfRules(j, abnfTreeRules)
  return j
}


let _treeParser: any = null

// An ABNF document as the tree, read whole. A source the meta-grammar
// refuses, or the tree cannot hold, fails with an AbnfParseError naming
// the line and column, its `cause` the engine's error with its code.
function parseAbnfTree(src: string): AbnfTree {
  if (null == _treeParser) _treeParser = make()
  try {
    // An empty source makes no rule pass, and the engine answers nothing
    // for it: it is the grammar of no rules, as a source of comments is.
    return (_treeParser.parse(src) ?? { type: 'grammar', rules: [] }) as AbnfTree
  } catch (e: any) {
    const line = e?.lineNumber ?? e?.row
    const column = e?.columnNumber ?? e?.col
    const loc = (line != null && column != null)
      ? ` at line ${line}, column ${column}`
      : ''
    const raw = e?.message ? String(e.message).split('\n')[0] : String(e)
    throw new AbnfParseError(`abnf: parse error${loc}: ${raw}`, { line, column }, e)
  }
}


export { make, parseAbnfTree, abnfTreeRules, numericElement }

export type { AbnfTree, AbnfTreeRule, AbnfTreeElement, AbnfTreeRepeat }
