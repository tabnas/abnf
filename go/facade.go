// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// facade.go — the public facade: Abnf (ABNF source -> GrammarSpec), ParseAbnf,
// EmitGrammarSpec, EliminateLeftRecursion, AbnfTransduce and TransduceAlc
// (the grammar's transducer part), plus the Plugin form
// (decorates a *tabnas.Tabnas, installing the grammar). The Go port of
// the exported surface of ts/src/abnf.ts + converter.ts.

import (
	"sort"
	"strconv"

	bnf "github.com/tabnas/bnf/go"
	tabnas "github.com/tabnas/parser/go"
)

// Abnf converts ABNF source into a tabnas GrammarSpec.
func Abnf(src string, opts *AbnfConvertOptions) (*tabnas.GrammarSpec, error) {
	grammar, err := parseAbnf(src)
	if err != nil {
		return nil, err
	}
	return emitGrammarSpec(grammar, opts)
}

// ParseAbnf parses ABNF source into a grammar AST (exported for callers
// inspecting productions). Returns *abnfGrammar via the internal type;
// for external use prefer Abnf.
func ParseAbnf(src string) (*abnfGrammar, error) {
	return parseAbnf(src)
}

// EmitGrammarSpec converts an already-parsed grammar into a GrammarSpec.
func EmitGrammarSpec(grammar *abnfGrammar, opts *AbnfConvertOptions) (*tabnas.GrammarSpec, error) {
	return emitGrammarSpec(grammar, opts)
}

// EliminateLeftRecursion rewrites direct + indirect left recursion via
// Paull's algorithm (exported to mirror the TS export).
func EliminateLeftRecursion(grammar *abnfGrammar) *abnfGrammar {
	return eliminateLeftRecursion(grammar)
}

// Install converts src and installs the resulting grammar on j. With
// actions supplied, conversion runs in closure mode with marks and the
// actions are attached. Mirrors the TS `tn.abnf(src, opts)` callable.
func Install(j *tabnas.Tabnas, src string, opts *AbnfConvertOptions, actions ActionsMap) (*tabnas.GrammarSpec, error) {
	useOpts := opts
	if actions != nil {
		base := AbnfConvertOptions{}
		if opts != nil {
			base = *opts
		}
		base.Builtins = false
		base.Marks = true
		useOpts = &base
	}
	spec, err := Abnf(src, useOpts)
	if err != nil {
		return nil, err
	}
	if actions != nil {
		if err := AttachActions(spec, actions); err != nil {
			return nil, err
		}
	}
	if err := j.Grammar(spec); err != nil {
		return nil, err
	}
	return spec, nil
}

// Plugin is the tabnas Plugin form: it installs a default grammar only
// if AbnfSource is set via options; primarily Abnf/Install are used
// directly. Provided for parity with the TS plugin shape.
func Plugin(j *tabnas.Tabnas, _ map[string]any) error {
	return nil
}

// --- tiny shared utils referenced across files ---

func intToStr(n int) string { return strconv.Itoa(n) }

func sortStrings(s []string) { sort.Strings(s) }

var _ = tabnas.Undefined

// TransducePart is the transducer part of an emitted grammar, or why its
// documents are read whole: bnf's TransducePart.
type TransducePart = bnf.TransducePart

// TransduceAlc is the transducer part of an emitted grammar, defining
// `<name>-transduce`, or why its documents are read whole: bnf's
// TransduceAlc, which reads the value actions the `; @object` and
// `; @array` annotations attach.
func TransduceAlc(spec *tabnas.GrammarSpec, name string) TransducePart {
	return bnf.TransduceAlc(spec, name)
}

// AbnfTransduce converts src as Abnf does, with opts, and writes the
// transducer part of the grammar: the alchemy program that streams the
// JSON events of the value its annotations build, from its rule passes.
// Mirrors the TS `abnfTransduce`.
func AbnfTransduce(src string, name string, opts *AbnfConvertOptions) (TransducePart, error) {
	spec, err := Abnf(src, opts)
	if err != nil {
		return TransducePart{}, err
	}
	return TransduceAlc(spec, name), nil
}
