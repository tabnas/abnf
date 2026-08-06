// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

// Package tabnasabnf is an ABNF -> tabnas GrammarSpec compiler for the
// tabnas parsing engine (github.com/tabnas/parser/go). It is a faithful
// Go port of the @tabnas/abnf TypeScript package.
//
// Given a small ABNF dialect it produces a function-free (when
// requested) GrammarSpec that, installed on a tabnas engine, parses
// inputs in that grammar and builds a {rule, src, kids} AST. It also
// emits "pure-data" jsonic (recognition / pure specs) and supports
// user actions.
//
// The package mirrors the TS sources:
//   - converter.ts -> converter.go (AST, parseAbnf, abnfRules, desugar,
//     core rules, left-recursion elimination, probe-dispatch rewriter,
//     FIRST sets, emitGrammarSpec, token allocation, Abnf entry) and
//     parser_abnf.go (the ABNF-for-ABNF parser grammar).
//   - compile.ts -> compile.go (AbnfCompile, ToRecognitionSpec,
//     ToPureSpec, ToJsonic, AttachActions, MarkListing).
//   - abnf.ts -> facade.go (Abnf, ParseAbnf, EmitGrammarSpec,
//     EliminateLeftRecursion, Install — the public facade).
package tabnasabnf

// Version is the current version of the module.
const Version = "0.3.1"

// ---- ABNF AST -------------------------------------------------------
//
// The parsed ABNF grammar is a list of productions, each an alternation
// of sequences of elements. Element kinds mirror the TS AbnfElement
// union; Go uses a single struct tagged by Kind plus optional fields.

// elemKind is the discriminator for a abnfElement.
type elemKind string

const (
	kindTerm  elemKind = "term"
	kindRef   elemKind = "ref"
	kindRegex elemKind = "regex"
	kindOpt   elemKind = "opt"
	kindStar  elemKind = "star"
	kindPlus  elemKind = "plus"
	kindRep   elemKind = "rep"
	kindGroup elemKind = "group"
	// kindToken is an engine builtin lexer token (e.g. #TX/#NR/#ST/#VL),
	// produced by normalizeBuiltinTokens. Its token name is held in Name and
	// is emitted verbatim into a rule's token sequence (no allocation, unlike
	// a literal term).
	kindToken elemKind = "token"
	// kindProse is an RFC 5234 prose-val (`<free text>`). Prose is
	// informational: it describes a terminal in English rather than defining
	// one. The converter accepts it only as the entire body of a production
	// naming a builtin lexer token (`NR = <number>`), where it documents the
	// token the lexer already provides; resolveProseTerminals then drops the
	// production so refs resolve to that builtin. Anywhere else there is
	// nothing to compile, and it is an error. Text holds the prose body.
	kindProse elemKind = "prose"
)

// abnfElement is one element of an ABNF sequence (a term, ref, regex, or
// EBNF sugar). Mirrors the TS AbnfElement union.
type abnfElement struct {
	Kind elemKind

	// term
	Literal       string
	CaseSensitive bool // explicit %s flag (ABNF strings are insensitive by default)
	hasCaseSens   bool // whether CaseSensitive was set explicitly (TS optional flag)
	// TokenName is the preferred lexer token name, set by liftLiteralTokens
	// when this terminal came from a production that names it (`PL = "+"` ->
	// `#PL`). Without it the emitter derives a name from the literal text,
	// which for punctuation degrades to `#T`, `#T1`, …
	TokenName string

	// prose
	Text string

	// regex
	Pattern string
	Flags   string

	// ref
	Name string

	// opt / star / plus / rep
	Inner *abnfElement
	Min   int
	Max   int // maxInfinity for unbounded

	// group
	Alts []abnfSequence
}

// maxInfinity stands in for the TS `Infinity` upper bound on repetition.
const maxInfinity = 1 << 30

type abnfSequence []*abnfElement

// probeDispatchSpec configures a synthesised dispatcher production for
// an ambiguous `[X D] Y` subsequence.
type probeDispatchSpec struct {
	ProbeRule     string
	Disambiguator *abnfElement
	WithBranch    string
	NoBranch      string
}

// probeHelperSpec carries the vocabulary for a synthesised probe helper.
type probeHelperSpec struct {
	VocabElements []*abnfElement
}

// nodeKind controls how a production contributes to the output AST:
//   - "user": emit a tagged node {rule, src, kids}.
//   - "core": RFC 5234 core rules — flatten into the enclosing src.
//   - "helper": synthetic sugar / dispatcher / chain rules — flatten.
//
// Empty is treated as "user".

type abnfProduction struct {
	Name        string
	Alts        []abnfSequence
	Incremental bool
	ProbeDisp   *probeDispatchSpec
	ProbeHelper *probeHelperSpec
	// TailRepeat is set by rewriteTailRepeats on a production of the
	// shape `X = prefix [ sep X ]` (all-terminal prefix and separator,
	// self-ref last). The opt is removed from Alts (leaving just the
	// prefix) and the separator elements are stashed here; the emitter
	// compiles the production to a same-depth close-phase repeat
	// (`r: X`) instead of the opt→group→push helper chain. Mirrors the
	// TS `tailRepeat` flag.
	TailRepeat  *tailRepeatSpec
	NodeKind    string // "", "user", "core", "helper"
}

type tailRepeatSpec struct {
	Sep abnfSequence
}

func (p *abnfProduction) kind() string {
	if p.NodeKind == "" {
		return "user"
	}
	return p.NodeKind
}

type abnfGrammar struct {
	Productions []*abnfProduction
	Ambiguities []ambiguityReport
}

type ambiguityReport struct {
	Rule     string
	AltIdx   int
	OptIdx   int
	Reason   string
	Resolved bool
}
