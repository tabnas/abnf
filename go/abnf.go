// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

// Package abnf is a BNF/ABNF -> tabnas GrammarSpec compiler for the
// tabnas parsing engine (github.com/tabnas/parser/go). It is a faithful
// Go port of the @tabnas/bnf TypeScript package.
//
// Given a small BNF/ABNF dialect it produces a function-free (when
// requested) GrammarSpec that, installed on a tabnas engine, parses
// inputs in that grammar and builds a {rule, src, kids} AST. It also
// emits "pure-data" jsonic (recognition / pure specs) and supports
// user actions.
//
// The package mirrors the TS sources:
//   - converter.ts -> converter.go (AST, parseBnf, bnfRules, desugar,
//     core rules, left-recursion elimination, probe-dispatch rewriter,
//     FIRST sets, emitGrammarSpec, token allocation, Bnf entry).
//   - compile.ts -> compile.go (BnfCompile, ToRecognitionSpec,
//     ToPureSpec, ToJsonic, AttachActions, MarkListing).
//   - bnf.ts -> plugin.go (the tn.bnf plugin facade).
package abnf

// Version is the current version of the module.
const Version = "0.1.0"

// ---- BNF AST -------------------------------------------------------
//
// The parsed BNF grammar is a list of productions, each an alternation
// of sequences of elements. Element kinds mirror the TS BnfElement
// union; Go uses a single struct tagged by Kind plus optional fields.

// elemKind is the discriminator for a bnfElement.
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
)

// bnfElement is one element of a BNF sequence (a term, ref, regex, or
// EBNF sugar). Mirrors the TS BnfElement union.
type bnfElement struct {
	Kind elemKind

	// term
	Literal       string
	CaseSensitive bool // explicit %s flag (ABNF strings are insensitive by default)
	hasCaseSens   bool // whether CaseSensitive was set explicitly (TS optional flag)

	// regex
	Pattern string
	Flags   string

	// ref
	Name string

	// opt / star / plus / rep
	Inner *bnfElement
	Min   int
	Max   int // maxInfinity for unbounded

	// group
	Alts []bnfSequence
}

// maxInfinity stands in for the TS `Infinity` upper bound on repetition.
const maxInfinity = 1 << 30

type bnfSequence []*bnfElement

// probeDispatchSpec configures a synthesised dispatcher production for
// an ambiguous `[X D] Y` subsequence.
type probeDispatchSpec struct {
	ProbeRule     string
	Disambiguator *bnfElement
	WithBranch    string
	NoBranch      string
}

// probeHelperSpec carries the vocabulary for a synthesised probe helper.
type probeHelperSpec struct {
	VocabElements []*bnfElement
}

// nodeKind controls how a production contributes to the output AST:
//   - "user": emit a tagged node {rule, src, kids}.
//   - "core": RFC 5234 core rules — flatten into the enclosing src.
//   - "helper": synthetic sugar / dispatcher / chain rules — flatten.
//
// Empty is treated as "user".

type bnfProduction struct {
	Name        string
	Alts        []bnfSequence
	Incremental bool
	ProbeDisp   *probeDispatchSpec
	ProbeHelper *probeHelperSpec
	NodeKind    string // "", "user", "core", "helper"
}

func (p *bnfProduction) kind() string {
	if p.NodeKind == "" {
		return "user"
	}
	return p.NodeKind
}

type bnfGrammar struct {
	Productions []*bnfProduction
	Ambiguities []ambiguityReport
}

type ambiguityReport struct {
	Rule     string
	AltIdx   int
	OptIdx   int
	Reason   string
	Resolved bool
}
