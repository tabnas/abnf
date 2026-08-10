/* Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License */

/*  converter.ts
 *  ABNF -> tabnas grammar spec converter: the RFC 5234 FRONT-END.
 *
 *  This file parses ABNF text into the notation-neutral grammar IR that
 *  `@tabnas/bnf` compiles. Everything downstream of that IR — desugaring,
 *  left-recursion elimination, tail repeats, probe dispatch, literal
 *  lifting, token allocation, first-set analysis, chain emission — lives
 *  in `@tabnas/bnf` and is shared with the GBNF and EBNF front-ends.
 *
 *    ABNF text ──parseAbnf──▶ Grammar ──bnf.emitGrammarSpec──▶ GrammarSpec
 *
 *  What stays here is what is genuinely ABNF:
 *
 *    - `abnfRules`, the tabnas grammar that reads ABNF syntax itself,
 *      and `getAbnfParser`, which installs it on a fresh instance;
 *    - the RFC 5234 Appendix B.1 core rules (ALPHA, DIGIT, CRLF, …);
 *    - incremental alternatives (`name =/ alt`);
 *    - numeric values (`%d65`, `%x41-5A`, `%b1010`, dotted sequences);
 *    - case-insensitive quoted strings, the RFC 5234 default.
 *
 *  Prose values (`NR = <number>`) are parsed here into the IR's `prose`
 *  element; `@tabnas/bnf` resolves them, since a prose terminal naming a
 *  built-in lexer token is useful to any notation.
 */

import type { GrammarSpec, Rule } from '@tabnas/parser'
import { util as engineUtil } from '@tabnas/parser'

import {
  emitGrammarSpec,
  eliminateLeftRecursion,
  refsIn,
} from '@tabnas/bnf'

import type {
  ConvertOptions,
  Element,
  Sequence,
  Production,
  Grammar,
} from '@tabnas/bnf'

// The IR types keep their historical names in this package's public API.
type AbnfConvertOptions = ConvertOptions
type AbnfElement = Element
type AbnfSequence = Sequence
type AbnfProduction = Production
type AbnfGrammar = Grammar


// Declarative definition of the ABNF grammar itself, expressed as
// tabnas rules. Each rule names its `open`/`close` alt list and, where
// necessary, a `bo`/`bc` state hook for AST assembly.
//
// Stage 8: incremental alternatives via `name =/ alt` now fold
// into the earlier production with the same name. Quoted strings
// default to case-insensitive (ABNF semantics), `%s` / `%i` force
// sensitivity explicitly, numeric values and repetition prefixes
// work as in previous stages.
//
// Token vocabulary:
//   #DEF   `=`  (rule-definition operator)
//   #DEFA  `=/` (incremental-alternatives operator)
//   #ALT   `/`  (alternation)
//   #STAR  `*`  (repetition separator)
//   #NUM   decimal repetition count (matched via match.token)
//   #NV    `%[xdb]NN[(-NN|(.NN)*)]` numeric value (match.token)
//   #SS    `%s` (case-sensitive string prefix)
//   #SI    `%i` (case-insensitive string prefix — same as default)
//   #LP    `(`
//   #RP    `)`
//   #OB    `[` (optional-group open)
//   #CB    `]` (optional-group close)
//   #TX    bare identifier (tabnas default text token)
//   #ST    quoted string literal (tabnas default string token)
//   #ZZ    end-of-source
//
// Grammar:
//   abnf        = production*
//   production = IDENT ('=' / '=/') alts
//   alts       = seq ('/' seq)*
//   seq        = element*
//   element    = repetition? atom
//   repetition = NUM '*' NUM / NUM '*' / '*' NUM / '*' / NUM
//   atom       = IDENT | STRING | ['%s' | '%i'] STRING | NUMVAL
//              | '(' alts ')' | '[' alts ']' | PROSE
//   prose      = '<' *(%x20-3D / %x3F-7E) '>'
//   numval     = '%' ('x' / 'd' / 'b') DIGITS [ '-' DIGITS | ('.' DIGITS)* ]
const abnfRules: Record<
  string,
  {
    bo?: (r: Rule) => void
    bc?: (r: Rule) => void
    open?: any[]
    close?: any[]
  }
> = {
  // Top-level: accumulates productions into r.node.
  abnf: {
    bo: (r) => { r.node = [] },
    open: [
      { s: '#ZZ', g: 'empty' },
      { p: 'prod' },
    ],
    close: [{ s: '#ZZ' }],
  },

  // One production per invocation; tail-recurses (r:'prod') for the
  // next. Inherits its parent's node (the productions array) and
  // appends to it in `bc` once its `alts` child has returned.
  // Production header is `IDENT =` — a bareword rule name followed
  // by the `=` definition operator.
  prod: {
    open: [
      // Standalone definition:   name = alts
      {
        s: '#TX #DEF',
        a: (r: Rule) => {
          r.u.name = r.o[0].val
          r.u.incremental = false
        },
        p: 'alts',
      },
      // `<all> = <remove>` — the whole-grammar reset. Prose lexes as its
      // own token (#PV), so it cannot be confused with a rule name or
      // with the `*` repetition operator. The name is kept with its
      // angle brackets, which no #TX rulename can produce, so it can
      // never collide with a production actually called `all`.
      {
        s: '#PV #DEF',
        a: (r: Rule) => {
          r.u.name = (r.o[0].src as string)
        },
        p: 'alts',
      },

      // Incremental alternatives:   name =/ alts
      {
        s: '#TX #DEFA',
        a: (r: Rule) => {
          r.u.name = r.o[0].val
          r.u.incremental = true
        },
        p: 'alts',
      },
    ],
    close: [
      // A TX followed by `=` or `=/` means the next production has
      // begun — back up 2 tokens so a fresh `prod` invocation sees
      // them.
      { s: '#TX #DEF', b: 2, r: 'prod' },
      { s: '#TX #DEFA', b: 2, r: 'prod' },
      { s: '#PV #DEF', b: 2, r: 'prod' },
      { b: 1 },
    ],
    bc: (r) => {
      if (r.child && r.child.node !== undefined) {
        const prod: any = { name: r.u.name, alts: r.child.node }
        if (r.u.incremental) prod.incremental = true
        r.node.push(prod)
      }
    },
  },

  // A list of alternative sequences separated by `/` (ABNF
  // alternation). Owns its own array (`bo` resets it) and pushes
  // each seq result in `bc`.
  alts: {
    bo: (r) => { r.node = [] },
    open: [{ p: 'seq' }],
    close: [
      { s: '#ALT', p: 'seq' },
      { b: 1 },
    ],
    bc: (r) => {
      if (r.child && r.child.node !== undefined) {
        r.node.push(r.child.node)
      }
    },
  },

  // A (possibly empty) sequence of elements. The 2-token lookahead
  // `#TX #DEF` detects a following production boundary and bails
  // out without consuming the tokens; a plain `#TX` at the leading
  // position (tried later so the longer alt wins) is a rule
  // reference inside the current sequence.
  seq: {
    bo: (r) => { r.node = [] },
    open: [
      { s: '#TX #DEF', b: 2, g: 'end' },
      { s: '#TX #DEFA', b: 2, g: 'end' },
      // A rulename followed by a repetition count — `a 1*b`, `a 2b`,
      // `simple-key 1*( dot-sep simple-key )`.
      //
      // This alternative exists for its `s:` pattern, not its action:
      // it widens the tcol the lexer uses for the token AFTER a
      // rulename. The `#TX #DEF` / `#TX #DEFA` lookaheads above are
      // the only other two-token patterns starting with `#TX`, so
      // without this one that tcol is just {#DEF, #DEFA} — the `#NUM`
      // match.token matcher is never offered the position, and the
      // digits fall through to the engine's default matchers. `1*b`
      // then arrives as #NR and fails to parse at all, and `2b`
      // arrives as a #TX bareword, silently misparsing as a reference
      // to a rule named `2b`. Same reason `elem.open` spells its
      // atom position `#ATOM` rather than leaving it implicit.
      { s: '#TX #NUM', b: 2, p: 'elem' },
      // Same problem, same fix, for a rulename followed by another
      // atom. `#ATOM` covers every atom-starter, so the `%xNN` (#NV),
      // `%s"…"` / `%i"…"` (#SS/#SI) and `<prose>` (#PV) matchers are
      // all offered the position. Without it `ws %x5B ws` silently
      // became a reference to a rule named `%x5B`, and `a <foo>` a
      // reference to `<foo>` instead of the prose error it should
      // raise. (The Go port gets these right via `match.tokenEager`,
      // which is what exposed the discrepancy.)
      { s: '#TX #ATOM', b: 2, p: 'elem' },
      // `<all> =` starts the next production; without this the prose
      // would be taken as another element of this sequence.
      { s: '#PV #DEF', b: 2, g: 'end' },
      { s: '#ALT', b: 1, g: 'end' },
      { s: '#ZZ', b: 1, g: 'end' },
      { s: '#RP', b: 1, g: 'end' },
      { s: '#CB', b: 1, g: 'end' },
      // Listing element-starter tokens in `s:` here ensures the
      // tcol-driven matcher considers each one when lexing.
      { s: '#ST', b: 1, p: 'elem' },
      { s: '#NV', b: 1, p: 'elem' },
      { s: '#SS', b: 1, p: 'elem' },
      { s: '#SI', b: 1, p: 'elem' },
      { s: '#PV', b: 1, p: 'elem' },
      { s: '#TX', b: 1, p: 'elem' },
      { s: '#LP', b: 1, p: 'elem' },
      { s: '#OB', b: 1, p: 'elem' },
      { s: '#STAR', b: 1, p: 'elem' },
      { s: '#NUM', b: 1, p: 'elem' },
      { p: 'elem' },
    ],
    close: [
      { s: '#TX #DEF', b: 2, g: 'end' },
      { s: '#TX #DEFA', b: 2, g: 'end' },
      // `<all> = …` starts the next production, exactly as in `open`.
      // `open` has always carried this alternative; `close` did not,
      // and only got away with it because the prose was mis-lexed as a
      // bareword here (so the `#TX #DEF` boundary above caught it by
      // accident). Now that `#TX #ATOM` lets the #PV matcher see the
      // position, the boundary has to be checked properly — and before
      // the `{ s: '#PV', p: 'elem' }` alternative further down, which
      // would otherwise take `<all>` as a prose element of this
      // sequence.
      { s: '#PV #DEF', b: 2, g: 'end' },
      // See the matching alternatives in `open` — these widen the tcol
      // for the token after a rulename, so `a 1*b` / `a 2b` lex as
      // #NUM and `ws %x5B` / `a %s"Q"` / `a <foo>` reach their own
      // matchers instead of falling through to a #TX bareword.
      { s: '#TX #NUM', b: 2, p: 'elem' },
      { s: '#TX #ATOM', b: 2, p: 'elem' },
      { s: '#ALT', b: 1, g: 'end' },
      { s: '#ZZ', b: 1, g: 'end' },
      { s: '#RP', b: 1, g: 'end' },
      { s: '#CB', b: 1, g: 'end' },
      { s: '#ST', b: 1, p: 'elem' },
      { s: '#NV', b: 1, p: 'elem' },
      { s: '#SS', b: 1, p: 'elem' },
      { s: '#SI', b: 1, p: 'elem' },
      { s: '#PV', b: 1, p: 'elem' },
      { s: '#TX', b: 1, p: 'elem' },
      { s: '#LP', b: 1, p: 'elem' },
      { s: '#OB', b: 1, p: 'elem' },
      { s: '#STAR', b: 1, p: 'elem' },
      { s: '#NUM', b: 1, p: 'elem' },
      { b: 1 },
    ],
  },

  // One element: an optional ABNF repetition prefix (`*A`, `1*A`,
  // `m*nA`, `*nA`, `m*A`, `nA`) followed by an atom. The prefix is
  // matched up front, stored on `r.u.min`/`r.u.max`; then `atom` is
  // pushed to parse the actual element body, whose result is wrapped
  // into an AST node and appended to the parent seq's array in close.
  elem: {
    bo: (r) => { r.u.min = 1; r.u.max = 1 },
    open: [
      // NUM '*' NUM — bounded repetition, followed by the atom
      // itself (listed via the ATOM tokenset so every atom-starter
      // tin — including `#NV` — is in tcol for this position).
      {
        s: '#NUM #STAR #NUM #ATOM',
        b: 1,
        a: (r: Rule) => {
          r.u.min = parseInt(r.o[0].src, 10)
          r.u.max = parseInt(r.o[2].src, 10)
        },
        p: 'atom',
      },
      // NUM '*' — at-least-NUM repetition followed by an atom.
      {
        s: '#NUM #STAR #ATOM',
        b: 1,
        a: (r: Rule) => {
          r.u.min = parseInt(r.o[0].src, 10)
          r.u.max = Infinity
        },
        p: 'atom',
      },
      // '*' NUM — at-most-NUM repetition.
      {
        s: '#STAR #NUM #ATOM',
        b: 1,
        a: (r: Rule) => {
          r.u.min = 0
          r.u.max = parseInt(r.o[1].src, 10)
        },
        p: 'atom',
      },
      // '*' — zero-or-more.
      {
        s: '#STAR #ATOM',
        b: 1,
        a: (r: Rule) => { r.u.min = 0; r.u.max = Infinity },
        p: 'atom',
      },
      // NUM — exact repetition count.
      {
        s: '#NUM #ATOM',
        b: 1,
        a: (r: Rule) => {
          const n = parseInt(r.o[0].src, 10)
          r.u.min = n
          r.u.max = n
        },
        p: 'atom',
      },
      // No prefix — push atom directly (min = max = 1).
      { p: 'atom' },
    ],
    close: [{
      // Wrap the returned atom (r.child.node) based on r.u.min/max
      // and append to the parent seq's array.
      a: (r: Rule) => {
        const item = r.child.node
        const { min, max } = r.u
        if (min === 1 && max === 1) {
          r.node.push(item)
        } else if (min === 0 && max === Infinity) {
          r.node.push({ kind: 'star', inner: item })
        } else if (min === 1 && max === Infinity) {
          r.node.push({ kind: 'plus', inner: item })
        } else if (min === 0 && max === 1) {
          r.node.push({ kind: 'opt', inner: item })
        } else {
          r.node.push({ kind: 'rep', min, max, inner: item })
        }
      },
    }],
  },

  // The atom body — a bareword ref, quoted-string terminal,
  // parenthesised group, or bracketed optional. Sets its OWN r.node
  // to the AST element so the enclosing `elem` rule can read it
  // from `r.child.node` in its close state.
  atom: {
    bo: (r) => { r.node = undefined },
    open: [
      // Case-sensitive string:   %s"foo"
      {
        s: '#SS #ST',
        a: (r: Rule) => {
          r.node = {
            kind: 'term',
            literal: r.o[1].val,
            caseSensitive: true,
          }
        },
      },
      // Case-insensitive string: %i"foo" (same as bare "foo" below,
      // but spelled explicitly).
      {
        s: '#SI #ST',
        a: (r: Rule) => {
          r.node = { kind: 'term', literal: r.o[1].val }
        },
      },
      // Bare quoted string — case-insensitive per ABNF default.
      {
        s: '#ST',
        a: (r: Rule) => {
          r.node = { kind: 'term', literal: r.o[0].val }
        },
      },
      {
        s: '#NV',
        a: (r: Rule) => {
          r.node = parseNumericValue(r.o[0].src as string)
        },
      },
      // Prose terminal `<free text>` — carried through as-is; the
      // `resolveProseTerminals` pass decides what it means.
      {
        s: '#PV',
        a: (r: Rule) => {
          const src = r.o[0].src as string
          r.node = { kind: 'prose', text: src.slice(1, -1) }
        },
      },
      {
        s: '#TX',
        a: (r: Rule) => {
          r.node = { kind: 'ref', name: r.o[0].val }
        },
      },
      {
        s: '#LP',
        a: (r: Rule) => { r.u.groupKind = 'group' },
        p: 'alts',
      },
      {
        s: '#OB',
        a: (r: Rule) => { r.u.groupKind = 'opt' },
        p: 'alts',
      },
    ],
    close: [
      {
        s: '#RP',
        c: (r: Rule) => r.u.groupKind === 'group',
        a: (r: Rule) => {
          r.node = { kind: 'group', alts: r.child.node }
        },
      },
      {
        s: '#CB',
        c: (r: Rule) => r.u.groupKind === 'opt',
        a: (r: Rule) => {
          r.node = {
            kind: 'opt',
            inner: { kind: 'group', alts: r.child.node },
          }
        },
      },
      // For simple atoms (string/ref), r.node is already set by
      // open; we want to pop without consuming the next token.
      // List every token that can legitimately follow an atom so
      // the lexer's tcol-driven match-matcher emits #NUM, #STAR,
      // and friends as their proper types here — otherwise the
      // default number-matcher would lex `1` as #NR and the
      // enclosing seq.close wouldn't recognise the digit as the
      // start of a repetition prefix.
      { s: '#TX', b: 1 },
      { s: '#ST', b: 1 },
      { s: '#NV', b: 1 },
      { s: '#SS', b: 1 },
      { s: '#SI', b: 1 },
      { s: '#PV', b: 1 },
      { s: '#NUM', b: 1 },
      { s: '#STAR', b: 1 },
      { s: '#LP', b: 1 },
      { s: '#OB', b: 1 },
      { s: '#RP', b: 1 },
      { s: '#CB', b: 1 },
      { s: '#ALT', b: 1 },
      { s: '#DEF', b: 1 },
      { s: '#ZZ', b: 1 },
      { b: 1 },
    ],
  },
}

// Cached tabnas instance for the ABNF grammar above; built on first use.
let _abnfParser: ((src: string) => Production[]) | null = null

function getAbnfParser(): (src: string) => AbnfProduction[] {
  if (_abnfParser) return _abnfParser

  const { Tabnas } = require('@tabnas/parser')

  // ABNF defines its own grammar from scratch, so we don't load any
  // grammar plugin — just use the bare engine with default tokens.
  const j = new Tabnas({
    rule: { start: 'abnf' },
    fixed: {
      token: {
        // Clear JSON-oriented defaults we're not using so `:`, `,`
        // and `{` have no special meaning inside ABNF source.
        '#OS': null,
        '#CS': null,
        '#CL': null,
        '#CA': null,
        // Re-map `#OB` / `#CB` from JSON's `{` / `}` to ABNF's
        // `[` / `]` optional-group brackets.
        '#OB': '[',
        '#CB': ']',
        '#DEF': '=',
        // `=/` — ABNF's incremental-alternatives operator. Longer
        // than `=`, so tabnas's longest-match-wins fixed matcher
        // tries it first.
        '#DEFA': '=/',
        '#ALT': '/',
        '#STAR': '*',
        '#LP': '(',
        '#RP': ')',
      },
    },
    match: {
      token: {
        // ABNF repetition counts: decimal integers.
        '#NUM': /^[0-9]+/,
        // ABNF numeric value notation:
        //   %xNN        single hex code point
        //   %dNN        single decimal code point
        //   %bNN        single binary code point
        //   %xNN-NN     hex range
        //   %xNN.NN.NN  concatenated hex code points (= string)
        // Digits are permissive (hex covers the decimal / binary
        // subsets); `parseNumericValue` re-validates against the
        // actual base.
        '#NV': /^%[xdbXDB][0-9a-fA-F]+(?:[-.][0-9a-fA-F]+)*/,
        // `%s` / `%i` prefixes on a quoted string. The lookahead
        // requires `"` so they don't steal the `%` of `%xNN`.
        '#SS': /^%[sS](?=")/,
        '#SI': /^%[iI](?=")/,
        // RFC 5234 prose-val: `<` free text `>`. The body is every
        // printable char except `>` itself (%x20-3D / %x3F-7E).
        '#PV': /^<[\x20-\x3D\x3F-\x7E]*>/,
      },
    },
    value: {
      // RFC 5234 rulename is `ALPHA *(ALPHA / DIGIT / "-")` — nothing is
      // reserved, so `true`, `false` and `null` are ordinary rule names.
      // JSON's grammar uses all three (`value = false / null / true /
      // object / array / number / string`), and with the engine's default
      // keyword-value lexing they arrived as `#VL` value tokens instead of
      // `#TX` barewords, so no ABNF rendering of JSON would compile.
      // The ABNF meta-grammar has no use for `#VL` at all — this switch
      // only affects the parser that reads ABNF source, not the grammars
      // it emits, where `VL` remains a built-in token name.
      lex: false,
    },
    string: {
      // RFC 5234 char-val has NO escape sequences at all:
      //   char-val = DQUOTE *(%x20-21 / %x23-7E) DQUOTE
      // A backslash is just %x5C, an ordinary member of that range, so
      // `"\"` is a one-character literal — and it is a common one, since
      // every RFC that defines `quoted-pair` writes it that way
      // (RFC 5322, RFC 3261, RFC 8259, …). With the engine's default
      // JSON-style escaping the backslash swallowed the closing quote
      // and the grammar died with `unterminated_string`, while `"a\b"`
      // silently became `a<BS>b` instead of the three characters
      // `a`, `\`, `b`.
      //
      // The engine offers no "escaping off" switch that both runtimes
      // share (TS takes `escapeChar: null`, Go falls back to `\` on an
      // empty string), so instead point the escape character at DEL
      // (%x7F) — outside char-val's %x20-21 / %x23-7E body, hence
      // unreachable in any legal ABNF literal.
      escapeChar: '\x7F',
    },
    tokenSet: {
      // Tokens that can legitimately open an atom. Declaring this
      // as a set lets elem.open use `#ATOM` inside its `s:` patterns
      // — that way the tcol at the atom-starter position includes
      // every matcher tin (notably #NV), so the lexer doesn't fall
      // through to #TX when the actual atom is `%xNN`.
      ATOM: ['#ST', '#NV', '#TX', '#LP', '#OB', '#SS', '#SI', '#PV'],
    },
    comment: {
      // ABNF uses `;` to start a line comment. Override tabnas's
      // default `hash` definition (which used `#`) and disable the
      // other comment styles so `//` and `/* */` aren't confused
      // with the alternation operator.
      def: {
        hash: { line: true, start: ';', lex: true, eatline: false },
        slash: null as any,
        multi: null as any,
      },
    },
  })

  // Drop the default JSON rules — they would otherwise compete with
  // ours for the starting token set.
  const existing = j.rule()
  for (const name of Object.keys(existing)) {
    j.rule(name, null)
  }

  for (const name of Object.keys(abnfRules)) {
    const spec = abnfRules[name]
    j.rule(name, (rs: any) => {
      if (spec.bo) rs.bo(spec.bo)
      if (spec.bc) rs.bc(spec.bc)
      if (spec.open) rs.open(spec.open)
      if (spec.close) rs.close(spec.close)
    })
  }

  _abnfParser = (src: string) => j.parse(src) as AbnfProduction[]
  return _abnfParser
}


// Error raised when the ABNF source itself can't be parsed.  Surfaces
// line and column from the underlying tabnas error so the caller can
// report them directly. The original error is kept on `.cause`.
class AbnfParseError extends Error {
  readonly line?: number
  readonly column?: number
  readonly cause?: unknown
  constructor(message: string, location?: { line?: number; column?: number }, cause?: unknown) {
    super(message)
    this.name = 'AbnfParseError'
    this.line = location?.line
    this.column = location?.column
    this.cause = cause
  }
}


// Parse ABNF source into a grammar AST via the tabnas-based parser.
function parseAbnf(src: string): AbnfGrammar {
  const parser = getAbnfParser()
  let productions: AbnfProduction[]
  try {
    productions = parser(src) ?? []
  } catch (e: any) {
    // TabnasError carries `lineNumber` / `columnNumber`; fall back to
    // ad-hoc extraction from the error message otherwise.
    const line = e?.lineNumber ?? e?.row
    const column = e?.columnNumber ?? e?.col
    const loc = (line != null && column != null)
      ? ` at line ${line}, column ${column}`
      : ''
    const raw = e?.message ? String(e.message).split('\n')[0] : String(e)
    throw new AbnfParseError(
      `abnf: parse error${loc}: ${raw}`,
      { line, column },
      e,
    )
  }
  if (!Array.isArray(productions) || productions.length === 0) {
    throw new AbnfParseError('abnf: no productions found')
  }
  const merged = mergeIncrementals(productions)
  return { productions: withCoreRules(merged) }
}


// RFC 5234 Appendix B.1 core rules. Parsed lazily on first use
// and spliced into any user grammar that references them but
// doesn't define them locally.
const CORE_RULES_ABNF = `
ALPHA  = %x41-5A / %x61-7A
BIT    = "0" / "1"
CHAR   = %x01-7F
CR     = %x0D
LF     = %x0A
CRLF   = CR LF
CTL    = %x00-1F / %x7F
DIGIT  = %x30-39
DQUOTE = %x22
HEXDIG = DIGIT / "A" / "B" / "C" / "D" / "E" / "F"
HTAB   = %x09
OCTET  = %x00-FF
SP     = %x20
VCHAR  = %x21-7E
WSP    = SP / HTAB
LWSP   = *( WSP / CRLF WSP )
`

let _coreRules: Map<string, AbnfProduction> | null = null

function getCoreRules(): Map<string, AbnfProduction> {
  if (_coreRules) return _coreRules
  const parser = getAbnfParser()
  const raw = parser(CORE_RULES_ABNF) as AbnfProduction[]
  // Core rules flatten to `src` in the output AST — they're
  // character-class bricks, not structural nodes users want to see
  // one-per-matched-character.
  for (const p of raw) p.nodeKind = 'core'
  _coreRules = new Map(raw.map((p) => [p.name, p]))
  return _coreRules
}


// Add each RFC 5234 core rule that the user's grammar references
// but doesn't define locally. Resolution is transitive: if the
// user mentions HEXDIG, DIGIT is pulled in too. User definitions
// always win — a local `DIGIT = …` is left untouched.
function withCoreRules(user: AbnfProduction[]): AbnfProduction[] {
  const core = getCoreRules()
  const defined = new Set(user.map((p) => p.name))
  const needed = new Set<string>()

  const scan = (prods: AbnfProduction[]) => {
    for (const p of prods) {
      for (const alt of p.alts) refsIn(alt, needed)
    }
  }

  scan(user)
  const out: AbnfProduction[] = []
  // Transitively add core rules, in declaration order.
  let added = true
  while (added) {
    added = false
    for (const [name, prod] of core) {
      if (defined.has(name)) continue
      if (!needed.has(name)) continue
      defined.add(name)
      out.push(prod)
      scan([prod])
      added = true
    }
  }
  return [...user, ...out]
}


// Fold every `name =/ alt` production into the earlier production
// with the same name by appending its alternatives. Throws if an
// incremental references a name that hasn't been defined yet — ABNF
// requires the base production to appear first.
function mergeIncrementals(prods: AbnfProduction[]): AbnfProduction[] {
  const out: AbnfProduction[] = []
  const byName = new Map<string, AbnfProduction>()
  for (const p of prods) {
    if (p.incremental) {
      const base = byName.get(p.name)
      if (!base) {
        throw new AbnfParseError(
          `abnf: '${p.name} =/ …' has no earlier '${p.name} = …' to extend`,
        )
      }
      base.alts.push(...p.alts)
      continue
    }
    // Strip the (absent) flag on a cleanly-written production so
    // downstream code never sees it.
    const clean: AbnfProduction = { name: p.name, alts: p.alts }
    if (p.nodeKind) clean.nodeKind = p.nodeKind
    out.push(clean)
    byName.set(p.name, clean)
  }
  return out
}


// Decode an ABNF numeric value (`%xNN`, `%dNN`, `%bNN`, or one of
// the range/concatenation forms) into a `AbnfElement`.
//
//   %x61            => single-char term "a"
//   %x66.6f.6f      => concatenated term "foo"
//   %x30-39         => regex character class [\u0030-\u0039]
//
// Hex is case-insensitive; decimal and binary accept only digits
// in their respective ranges. Range endpoints must be the same
// base as the prefix (RFC 5234 doesn't allow mixing).
function parseNumericValue(src: string): AbnfElement {
  const base = src[1].toLowerCase()
  const radix = base === 'x' ? 16 : base === 'd' ? 10 : 2
  const body = src.slice(2)

  // RFC 5234 puts no ceiling on a numeric value, but Unicode does:
  // nothing above U+10FFFF is a code point. Check it here so an
  // out-of-range grammar gets an ABNF diagnostic naming the offending
  // value, rather than a bare `RangeError: Invalid code point` from
  // String.fromCodePoint (or, as before, a silently truncated
  // character from String.fromCharCode).
  const codePoint = (text: string): number => {
    const n = parseInt(text, radix)
    if (!Number.isFinite(n) || n < 0 || 0x10FFFF < n) {
      throw new Error(
        `numeric value '%${src[1]}${text}' is ${n}, which is not a ` +
        `Unicode code point (the maximum is %x10FFFF).`)
    }
    return n
  }

  if (body.includes('-')) {
    const [loStr, hiStr] = body.split('-')
    const lo = codePoint(loStr)
    const hi = codePoint(hiStr)
    if (lo === hi) {
      return { kind: 'term', literal: String.fromCodePoint(lo) }
    }
    // `\uXXXX` only reaches U+FFFF: `%xE000-10FFFF` (JSONPath, TOML)
    // became `[-ჿff]`, which JS reads as the range E000–10FF
    // plus a literal `ff` — "Range out of order". Above the BMP the
    // escape has to be `\u{…}`, which in turn requires the `u` flag.
    // Stay on the plain form below U+FFFF so existing output is
    // unchanged. (The Go port emits `\x{…}`, whose length is already
    // variable, and never had the bug.)
    const astral = 0xFFFF < lo || 0xFFFF < hi
    const toEsc = (n: number) =>
      astral
        ? '\\u{' + n.toString(16) + '}'
        : '\\u' + n.toString(16).padStart(4, '0')
    return {
      kind: 'regex',
      pattern: '[' + toEsc(lo) + '-' + toEsc(hi) + ']',
      flags: astral ? 'u' : '',
    }
  }

  const parts = body.split('.')
  const chars = parts.map((n) => String.fromCodePoint(codePoint(n)))
  return { kind: 'term', literal: chars.join('') }
}


// Public entry point: take ABNF source and return a tabnas GrammarSpec.


// Convert ABNF source into a tabnas grammar spec: parse this notation,
// then hand the IR to the shared compiler. `tag` defaults to 'abnf' so
// the emitted alts keep their historical group tag.
function abnf(src: string, opts?: AbnfConvertOptions): GrammarSpec {
  const grammar = parseAbnf(src)
  return emitGrammarSpec(grammar, { ...opts, tag: opts?.tag ?? 'abnf' })
}


export {
  abnf,
  parseAbnf,
  emitGrammarSpec,
  eliminateLeftRecursion,
  abnfRules,
  AbnfParseError,
}

export type {
  AbnfConvertOptions,
  AbnfElement,
  AbnfSequence,
  AbnfProduction,
  AbnfGrammar,
}
