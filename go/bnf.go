// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package abnf

// bnf.go — the public facade: Bnf (BNF source -> GrammarSpec), ParseBnf,
// EmitGrammarSpec, EliminateLeftRecursion, plus the Plugin form
// (decorates a *tabnas.Tabnas, installing the grammar). The Go port of
// the exported surface of ts/src/bnf.ts + converter.ts.

import (
	"sort"
	"strconv"

	tabnas "github.com/tabnas/parser/go"
)

// Bnf converts BNF/ABNF source into a tabnas GrammarSpec.
func Bnf(src string, opts *BnfConvertOptions) (*tabnas.GrammarSpec, error) {
	grammar, err := parseBnf(src)
	if err != nil {
		return nil, err
	}
	return emitGrammarSpec(grammar, opts)
}

// ParseBnf parses BNF source into a grammar AST (exported for callers
// inspecting productions). Returns *bnfGrammar via the internal type;
// for external use prefer Bnf.
func ParseBnf(src string) (*bnfGrammar, error) {
	return parseBnf(src)
}

// EmitGrammarSpec converts an already-parsed grammar into a GrammarSpec.
func EmitGrammarSpec(grammar *bnfGrammar, opts *BnfConvertOptions) (*tabnas.GrammarSpec, error) {
	return emitGrammarSpec(grammar, opts)
}

// EliminateLeftRecursion rewrites direct + indirect left recursion via
// Paull's algorithm (exported to mirror the TS export).
func EliminateLeftRecursion(grammar *bnfGrammar) *bnfGrammar {
	return eliminateLeftRecursion(grammar)
}

// Install converts src and installs the resulting grammar on j. With
// actions supplied, conversion runs in closure mode with marks and the
// actions are attached. Mirrors the TS `tn.bnf(src, opts)` callable.
func Install(j *tabnas.Tabnas, src string, opts *BnfConvertOptions, actions ActionsMap) (*tabnas.GrammarSpec, error) {
	useOpts := opts
	if actions != nil {
		base := BnfConvertOptions{}
		if opts != nil {
			base = *opts
		}
		base.Builtins = false
		base.Marks = true
		useOpts = &base
	}
	spec, err := Bnf(src, useOpts)
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
// if BnfSource is set via options; primarily Bnf/Install are used
// directly. Provided for parity with the TS plugin shape.
func Plugin(j *tabnas.Tabnas, _ map[string]any) error {
	return nil
}

// --- tiny shared utils referenced across files ---

func intToStr(n int) string { return strconv.Itoa(n) }

func sortStrings(s []string) { sort.Strings(s) }

var _ = tabnas.Undefined
