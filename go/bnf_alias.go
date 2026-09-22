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
	"strings"

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

// ValueAnnotation is EXPORTED, unlike the aliases above: it is what a
// caller reads off a production to see what the grammar says it builds,
// and what a test asserts against. The rest of the IR stays internal.
type ValueAnnotation = bnf.ValueAnnotation

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
	g *abnfGrammar, opts *AbnfConvertOptions) (spec *tabnas.GrammarSpec, err error) {
	if opts == nil {
		opts = &AbnfConvertOptions{}
	}
	if opts.Tag == "" {
		clone := *opts
		clone.Tag = "abnf"
		opts = &clone
	}
	// A grammar is untrusted input, so a compiler must never take the
	// process down over one. The shared compiler hands every character
	// class it builds to regexp.MustCompile, and a class Go's regexp
	// rejects, a reversed numeric range such as `%x5A-41` being the one
	// an author can write, PANICS there where TypeScript throws and Rust
	// returns (tabnas/abnf#72, DIVERGENCE.md entry 6). The repair that
	// belongs to the shared compiler is regexp.Compile at that site; this
	// boundary turns the panic into the error return the other two
	// runtimes give until it lands, and it stays useful afterwards
	// because it guards every other way a generated class could fail to
	// compile.
	//
	// ONLY a regexp compile panic is converted. Go's regexp panics with
	// a string that opens `regexp: Compile(`, and nothing else in the
	// pipeline produces one. Every other panic is a compiler BUG, and a
	// bug that returns an error looks like a rejected grammar, so those
	// keep panicking exactly as the shared compiler's own boundary lets
	// them.
	defer func() {
		if r := recover(); nil != r {
			text, isText := r.(string)
			if !isText || !strings.HasPrefix(text, regexpPanicPrefix) {
				panic(r)
			}
			spec, err = nil, &bnf.EmitError{
				Message: opts.Tag + ": invalid regular expression: " +
					strings.TrimPrefix(text, "regexp: "),
			}
		}
	}()
	return bnf.EmitGrammarSpec(g, opts)
}

// regexpPanicPrefix opens the message regexp.MustCompile panics with.
const regexpPanicPrefix = "regexp: Compile("

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
