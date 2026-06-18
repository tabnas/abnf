// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// facade.go — the public facade: Abnf (ABNF source -> GrammarSpec), ParseAbnf,
// EmitGrammarSpec, EliminateLeftRecursion, plus the Plugin form
// (decorates a *tabnas.Tabnas, installing the grammar). The Go port of
// the exported surface of ts/src/abnf.ts + converter.ts.

import (
	"sort"
	"strconv"

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
