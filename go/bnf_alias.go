// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

// bnf_alias.go — the seam between this front-end and the shared
// compiler.
//
// Everything downstream of the grammar IR now lives in
// github.com/tabnas/bnf/go, shared with the GBNF and EBNF front-ends.
// This file re-establishes every name that used to be declared locally,
// so the ABNF package's public API is unchanged: a consumer sees the
// same types and functions under the same names as before the
// extraction.
//
// Type ALIASES (`=`), not definitions: `AbnfConvertOptions` must BE
// `bnf.ConvertOptions`, not merely look like it, or a value could not
// cross the package boundary.
package tabnasabnf

import (
	bnf "github.com/tabnas/bnf/go"
	tabnas "github.com/tabnas/parser/go"
)

// ---- The grammar IR ------------------------------------------------

type abnfElement = bnf.Element
type abnfSequence = bnf.Sequence
type abnfProduction = bnf.Production
type abnfGrammar = bnf.Grammar
type elemKind = bnf.ElemKind
type ambiguityReport = bnf.AmbiguityReport
type probeDispatchSpec = bnf.ProbeDispatchSpec
type probeHelperSpec = bnf.ProbeHelperSpec
type tailRepeatSpec = bnf.TailRepeatSpec

const maxInfinity = bnf.MaxInfinity

const (
	kindTerm  = bnf.KindTerm
	kindRef   = bnf.KindRef
	kindRegex = bnf.KindRegex
	kindOpt   = bnf.KindOpt
	kindStar  = bnf.KindStar
	kindPlus  = bnf.KindPlus
	kindRep   = bnf.KindRep
	kindGroup = bnf.KindGroup
	kindToken = bnf.KindToken
	kindProse = bnf.KindProse
)

// ---- Public surface, unchanged -------------------------------------

// AbnfConvertOptions is the shared compiler's ConvertOptions under this
// package's historical name.
type AbnfConvertOptions = bnf.ConvertOptions

// AbnfCompileOptions and the two error types likewise.
type AbnfCompileOptions = bnf.CompileOptions
type AbnfCompileError = bnf.CompileError
type AbnfActionError = bnf.ActionError

// ActionFn and ActionsMap are the user-action surface.
type ActionFn = bnf.ActionFn
type ActionsMap = bnf.ActionsMap

// Spec transforms. These only ever took a GrammarSpec, so they were
// already notation-neutral.
var (
	ToRecognitionSpec = bnf.ToRecognitionSpec
	ToPureSpec        = bnf.ToPureSpec
	ToJsonic          = bnf.ToJsonic
	SpecToData        = bnf.SpecToData
	SpecToJSON        = bnf.SpecToJSON
	AttachActions     = bnf.AttachActions
	MarkListing       = bnf.MarkListing
)

// ---- Internals this front-end still calls --------------------------

// Diagnostics must keep saying "abnf:", not the shared package's name:
// a user who wrote ABNF should never see an error mentioning a package
// they did not import. The shared compiler takes the prefix from Tag,
// so this front-end supplies its own whenever the caller did not.
func emitGrammarSpec(
	g *abnfGrammar, opts *AbnfConvertOptions) (*tabnas.GrammarSpec, error) {
	if opts == nil {
		opts = &AbnfConvertOptions{}
	}
	if opts.Tag == "" {
		clone := *opts
		clone.Tag = "abnf"
		opts = &clone
	}
	return bnf.EmitGrammarSpec(g, opts)
}

func eliminateLeftRecursion(g *abnfGrammar) *abnfGrammar {
	return bnf.EliminateLeftRecursion(g)
}

func refsIn(alt abnfSequence, out map[string]bool) { bnf.RefsIn(alt, out) }

func isProseName(name string) bool { return bnf.IsProseName(name) }

// findProd is a lookup the front-end's own tests use; it travelled with
// the emitter during the extraction, so it is restated here rather than
// widening the shared package's surface for a test helper.
func findProd(grammar *abnfGrammar, name string) *abnfProduction {
	for _, p := range grammar.Productions {
		if p.Name == name {
			return p
		}
	}
	return nil
}
