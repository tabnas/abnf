// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package abnf

// converter.go — BNF grammar AST -> tabnas GrammarSpec. The Go port of
// the transformation pipeline in ts/src/converter.ts: parseBnf,
// mergeIncrementals, core rules, eliminateLeftRecursion (Paull's),
// rewriteProbeDispatches, desugar, FIRST sets, and emitGrammarSpec.

import (
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"

	tabnas "github.com/tabnas/parser/go"
)

// BnfConvertOptions controls conversion. Mirrors the TS type.
type BnfConvertOptions struct {
	Start    string
	Tag      string
	Builtins bool
	Marks    bool
}

// BnfParseError is raised when the BNF source itself can't be parsed.
type BnfParseError struct {
	Message string
	Line    int
	Column  int
	Cause   error
}

func (e *BnfParseError) Error() string { return e.Message }
func (e *BnfParseError) Unwrap() error { return e.Cause }

// ---- parseBnf ------------------------------------------------------

// parseBnf parses BNF source into a grammar AST via the tabnas-based
// parser, merging incrementals and splicing in referenced core rules.
func parseBnf(src string) (*bnfGrammar, error) {
	productions, err := parseBnfRaw(src)
	if err != nil {
		line, col := errLineCol(err)
		loc := ""
		if line != 0 && col != 0 {
			loc = fmt.Sprintf(" at line %d, column %d", line, col)
		}
		raw := strings.SplitN(err.Error(), "\n", 2)[0]
		return nil, &BnfParseError{
			Message: fmt.Sprintf("bnf: parse error%s: %s", loc, raw),
			Line:    line, Column: col, Cause: err,
		}
	}
	if len(productions) == 0 {
		return nil, &BnfParseError{Message: "bnf: no productions found"}
	}
	merged, merr := mergeIncrementals(productions)
	if merr != nil {
		return nil, merr
	}
	withCore := withCoreRules(merged)
	return &bnfGrammar{Productions: withCore}, nil
}

// errLineCol attempts to pull line/column from a tabnas parse error.
func errLineCol(err error) (int, int) {
	if te, ok := err.(*tabnas.TabnasError); ok {
		return te.Row, te.Col
	}
	return 0, 0
}

// ---- merge incrementals --------------------------------------------

func mergeIncrementals(prods []*bnfProduction) ([]*bnfProduction, error) {
	out := []*bnfProduction{}
	byName := map[string]*bnfProduction{}
	for _, p := range prods {
		if p.Incremental {
			base := byName[p.Name]
			if base == nil {
				return nil, &BnfParseError{Message: fmt.Sprintf(
					"bnf: '%s =/ …' has no earlier '%s = …' to extend", p.Name, p.Name)}
			}
			base.Alts = append(base.Alts, p.Alts...)
			continue
		}
		clean := &bnfProduction{Name: p.Name, Alts: p.Alts}
		if p.NodeKind != "" {
			clean.NodeKind = p.NodeKind
		}
		out = append(out, clean)
		byName[p.Name] = clean
	}
	return out, nil
}

// ---- core rules ----------------------------------------------------

const coreRulesABNF = `
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
`

// coreRuleList returns the parsed core rules (order-preserving) with
// nodeKind=core. Parsed on each call; the parser instance is cached.
func coreRuleList() []*bnfProduction {
	raw, err := parseBnfRaw(coreRulesABNF)
	if err != nil {
		panic("bnf: internal — core rules failed to parse: " + err.Error())
	}
	for _, p := range raw {
		p.NodeKind = "core"
	}
	return raw
}

func refsIn(alt bnfSequence, out map[string]bool) {
	for _, el := range alt {
		switch el.Kind {
		case kindRef:
			out[el.Name] = true
		case kindOpt, kindStar, kindPlus, kindRep:
			refsIn(bnfSequence{el.Inner}, out)
		case kindGroup:
			for _, a := range el.Alts {
				refsIn(a, out)
			}
		}
	}
}

// withCoreRules adds each RFC 5234 core rule that the user references
// but doesn't define locally. Resolution is transitive.
func withCoreRules(user []*bnfProduction) []*bnfProduction {
	core := coreRuleList()
	coreByName := map[string]*bnfProduction{}
	coreOrder := []string{}
	for _, p := range core {
		coreByName[p.Name] = p
		coreOrder = append(coreOrder, p.Name)
	}
	defined := map[string]bool{}
	for _, p := range user {
		defined[p.Name] = true
	}
	needed := map[string]bool{}
	scan := func(prods []*bnfProduction) {
		for _, p := range prods {
			for _, alt := range p.Alts {
				refsIn(alt, needed)
			}
		}
	}
	scan(user)
	out := []*bnfProduction{}
	added := true
	for added {
		added = false
		for _, name := range coreOrder {
			if defined[name] || !needed[name] {
				continue
			}
			prod := coreByName[name]
			defined[name] = true
			out = append(out, prod)
			scan([]*bnfProduction{prod})
			added = true
		}
	}
	return append(append([]*bnfProduction{}, user...), out...)
}

// ---- left-recursion elimination (Paull's) --------------------------

func eliminateLeftRecursion(grammar *bnfGrammar) *bnfGrammar {
	originalOrder := make([]string, len(grammar.Productions))
	for i, p := range grammar.Productions {
		originalOrder[i] = p.Name
	}

	// Copy productions (shallow-copy alts) before reordering.
	copies := make([]*bnfProduction, len(grammar.Productions))
	for i, p := range grammar.Productions {
		alts := make([]bnfSequence, len(p.Alts))
		for j, a := range p.Alts {
			alts[j] = append(bnfSequence{}, a...)
		}
		copies[i] = &bnfProduction{Name: p.Name, Alts: alts, NodeKind: p.NodeKind}
	}
	prods := topoOrderForPaull(copies)

	for i := 0; i < len(prods); i++ {
		for j := 0; j < i; j++ {
			prods[i] = substituteLeadingRef(prods[i], prods[j])
		}
		prods[i] = eliminateDirectLeftRec(prods[i])
	}

	byName := map[string]*bnfProduction{}
	for _, p := range prods {
		byName[p.Name] = p
	}
	ordered := []*bnfProduction{}
	for _, name := range originalOrder {
		if p, ok := byName[name]; ok {
			ordered = append(ordered, p)
			delete(byName, name)
		}
	}
	// Any remaining (generated) productions, in stable order.
	remaining := []string{}
	for name := range byName {
		remaining = append(remaining, name)
	}
	sort.Strings(remaining)
	for _, name := range remaining {
		ordered = append(ordered, byName[name])
	}
	return &bnfGrammar{Productions: ordered}
}

// topoOrderForPaull orders over the leading-position reference graph.
func topoOrderForPaull(prods []*bnfProduction) []*bnfProduction {
	byName := map[string]*bnfProduction{}
	for _, p := range prods {
		byName[p.Name] = p
	}
	colour := map[string]int{} // 0 unseen, 1 in-progress, 2 done
	order := []*bnfProduction{}
	var visit func(name string)
	visit = func(name string) {
		if colour[name] != 0 {
			return
		}
		colour[name] = 1
		p := byName[name]
		if p != nil {
			for _, alt := range p.Alts {
				if len(alt) > 0 && alt[0].Kind == kindRef {
					if _, ok := byName[alt[0].Name]; ok {
						visit(alt[0].Name)
					}
				}
			}
			colour[name] = 2
			order = append(order, p)
		} else {
			colour[name] = 2
		}
	}
	for _, p := range prods {
		visit(p.Name)
	}
	return order
}

func substituteLeadingRef(target, source *bnfProduction) *bnfProduction {
	newAlts := []bnfSequence{}
	for _, alt := range target.Alts {
		if len(alt) > 0 && alt[0].Kind == kindRef && alt[0].Name == source.Name {
			tail := append(bnfSequence{}, alt[1:]...)
			for _, srcAlt := range source.Alts {
				combined := append(append(bnfSequence{}, srcAlt...), tail...)
				newAlts = append(newAlts, combined)
			}
		} else {
			newAlts = append(newAlts, alt)
		}
	}
	return &bnfProduction{Name: target.Name, Alts: newAlts, NodeKind: target.NodeKind}
}

func eliminateDirectLeftRec(prod *bnfProduction) *bnfProduction {
	recursive := []bnfSequence{}
	seeds := []bnfSequence{}
	for _, alt := range prod.Alts {
		if len(alt) > 0 && alt[0].Kind == kindRef && alt[0].Name == prod.Name {
			recursive = append(recursive, append(bnfSequence{}, alt[1:]...))
		} else {
			seeds = append(seeds, alt)
		}
	}
	nonTrivial := []bnfSequence{}
	for _, t := range recursive {
		if len(t) > 0 {
			nonTrivial = append(nonTrivial, t)
		}
	}
	if len(nonTrivial) == 0 {
		return &bnfProduction{Name: prod.Name, Alts: seeds, NodeKind: prod.NodeKind}
	}
	if len(seeds) == 0 {
		panic(fmt.Sprintf("bnf: rule '%s' is purely left-recursive "+
			"(no seed alternative); cannot eliminate", prod.Name))
	}

	var seedElement *bnfElement
	if len(seeds) == 1 && len(seeds[0]) == 1 {
		seedElement = seeds[0][0]
	} else {
		seedElement = &bnfElement{Kind: kindGroup, Alts: seeds}
	}
	var tailInner *bnfElement
	if len(nonTrivial) == 1 && len(nonTrivial[0]) == 1 {
		tailInner = nonTrivial[0][0]
	} else {
		tailInner = &bnfElement{Kind: kindGroup, Alts: nonTrivial}
	}
	return &bnfProduction{
		Name: prod.Name,
		Alts: []bnfSequence{{seedElement, {Kind: kindStar, Inner: tailInner}}},
		NodeKind: prod.NodeKind,
	}
}

// ---- desugar -------------------------------------------------------

func desugar(grammar *bnfGrammar) *bnfGrammar {
	extra := []*bnfProduction{}
	used := map[string]bool{}
	for _, p := range grammar.Productions {
		used[p.Name] = true
	}
	freshName := func(hint string) string {
		i := len(extra)
		var name string
		for {
			i++
			name = fmt.Sprintf("_gen%d_%s", i, hint)
			if !used[name] {
				break
			}
		}
		used[name] = true
		return name
	}

	var desugarElement func(el *bnfElement) *bnfElement
	desugarAlt := func(alt bnfSequence) bnfSequence {
		out := make(bnfSequence, len(alt))
		for i, el := range alt {
			out[i] = desugarElement(el)
		}
		return out
	}
	desugarElement = func(el *bnfElement) *bnfElement {
		switch el.Kind {
		case kindTerm, kindRef, kindRegex:
			return el
		case kindGroup:
			innerAlts := make([]bnfSequence, len(el.Alts))
			for i, a := range el.Alts {
				innerAlts[i] = desugarAlt(a)
			}
			name := freshName("group")
			extra = append(extra, &bnfProduction{Name: name, Alts: innerAlts, NodeKind: "helper"})
			return &bnfElement{Kind: kindRef, Name: name}
		}

		inner := desugarElement(el.Inner)
		hint := "x"
		if inner.Kind == kindRef {
			hint = inner.Name
		} else if inner.Kind == kindTerm {
			hint = "term"
		}

		switch el.Kind {
		case kindOpt:
			name := freshName("opt_" + hint)
			extra = append(extra, &bnfProduction{
				Name: name, Alts: []bnfSequence{{inner}, {}}, NodeKind: "helper"})
			return &bnfElement{Kind: kindRef, Name: name}
		case kindStar:
			name := freshName("star_" + hint)
			selfRef := &bnfElement{Kind: kindRef, Name: name}
			extra = append(extra, &bnfProduction{
				Name: name, Alts: []bnfSequence{{inner, selfRef}, {}}, NodeKind: "helper"})
			return &bnfElement{Kind: kindRef, Name: name}
		case kindPlus:
			tailName := freshName("star_" + hint)
			plusName := freshName("plus_" + hint)
			tailRef := &bnfElement{Kind: kindRef, Name: tailName}
			extra = append(extra, &bnfProduction{
				Name: tailName, Alts: []bnfSequence{{inner, tailRef}, {}}, NodeKind: "helper"})
			extra = append(extra, &bnfProduction{
				Name: plusName, Alts: []bnfSequence{{inner, tailRef}}, NodeKind: "helper"})
			return &bnfElement{Kind: kindRef, Name: plusName}
		}

		// kindRep — bounded m*n.
		min, max := el.Min, el.Max
		repName := freshName("rep_" + hint)
		repAlt := bnfSequence{}
		for i := 0; i < min; i++ {
			repAlt = append(repAlt, inner)
		}
		if max == maxInfinity {
			tailStarName := freshName("star_" + hint)
			tailStarRef := &bnfElement{Kind: kindRef, Name: tailStarName}
			extra = append(extra, &bnfProduction{
				Name: tailStarName, Alts: []bnfSequence{{inner, tailStarRef}, {}}, NodeKind: "helper"})
			repAlt = append(repAlt, tailStarRef)
		} else {
			var nested bnfSequence
			for i := 0; i < max-min; i++ {
				if len(nested) == 0 {
					nested = bnfSequence{{Kind: kindOpt,
						Inner: &bnfElement{Kind: kindGroup, Alts: []bnfSequence{{inner}}}}}
				} else {
					inside := append(bnfSequence{inner}, nested...)
					nested = bnfSequence{{Kind: kindOpt,
						Inner: &bnfElement{Kind: kindGroup, Alts: []bnfSequence{inside}}}}
				}
			}
			repAlt = append(repAlt, nested...)
		}
		extra = append(extra, &bnfProduction{
			Name: repName, Alts: []bnfSequence{desugarAlt(repAlt)}, NodeKind: "helper"})
		return &bnfElement{Kind: kindRef, Name: repName}
	}

	rewritten := []*bnfProduction{}
	for _, p := range grammar.Productions {
		out := &bnfProduction{Name: p.Name, NodeKind: p.NodeKind}
		alts := make([]bnfSequence, len(p.Alts))
		for i, a := range p.Alts {
			alts[i] = desugarAlt(a)
		}
		out.Alts = alts
		if p.ProbeDisp != nil {
			out.ProbeDisp = p.ProbeDisp
		}
		if p.ProbeHelper != nil {
			out.ProbeHelper = p.ProbeHelper
		}
		rewritten = append(rewritten, out)
	}
	return &bnfGrammar{Productions: append(rewritten, extra...)}
}

// ---- numeric value -------------------------------------------------

func parseNumericValue(src string) *bnfElement {
	base := strings.ToLower(string(src[1]))
	radix := 16
	if base == "d" {
		radix = 10
	} else if base == "b" {
		radix = 2
	}
	body := src[2:]

	if strings.Contains(body, "-") {
		parts := strings.SplitN(body, "-", 2)
		lo, _ := strconv.ParseInt(parts[0], radix, 64)
		hi, _ := strconv.ParseInt(parts[1], radix, 64)
		if lo == hi {
			return &bnfElement{Kind: kindTerm, Literal: string(rune(lo))}
		}
		toEsc := func(n int64) string {
			return fmt.Sprintf("\\x{%04x}", n)
		}
		return &bnfElement{
			Kind:    kindRegex,
			Pattern: "[" + toEsc(lo) + "-" + toEsc(hi) + "]",
			Flags:   "",
		}
	}

	parts := strings.Split(body, ".")
	var sb strings.Builder
	for _, n := range parts {
		v, _ := strconv.ParseInt(n, radix, 64)
		sb.WriteRune(rune(v))
	}
	return &bnfElement{Kind: kindTerm, Literal: sb.String()}
}

// ---- key / case-sensitivity helpers --------------------------------

var letterRe = regexp.MustCompile(`[A-Za-z]`)

func isEffectivelyCaseSensitive(el *bnfElement) bool {
	if el.hasCaseSens && el.CaseSensitive {
		return true
	}
	return !letterRe.MatchString(el.Literal)
}

func termKey(el *bnfElement) string {
	prefix := "ci:"
	if isEffectivelyCaseSensitive(el) {
		prefix = "cs:"
	}
	return prefix + el.Literal
}

func regexKey(el *bnfElement) string {
	return "/" + el.Pattern + "/" + el.Flags
}

var nonIdentRe = regexp.MustCompile(`[^A-Za-z0-9]`)
var trimUnderscore = regexp.MustCompile(`^_+|_+$`)

func allocTokenName(literal string, used map[string]bool) string {
	base := nonIdentRe.ReplaceAllString(literal, "_")
	base = strings.ToUpper(base)
	base = trimUnderscore.ReplaceAllString(base, "")
	candidate := "#T"
	if len(base) > 0 {
		candidate = "#" + base
	}
	if !used[candidate] {
		used[candidate] = true
		return candidate
	}
	i := 1
	for used[candidate+strconv.Itoa(i)] {
		i++
	}
	chosen := candidate + strconv.Itoa(i)
	used[chosen] = true
	return chosen
}

// escapeRegexp mirrors the TS escapeRegExp for case-insensitive literal
// match tokens.
var escapeRe = regexp.MustCompile(`[\\^$.*+?()[\]{}|]`)

func escapeRegexp(s string) string {
	return escapeRe.ReplaceAllString(s, `\$0`)
}
