// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// tree.go — an ABNF document read as the schema grammar-rules: its rules
// as the text writes them, uncompiled and in source order (tabnas/bnf's
// docs/grammar-rules.md, and section 13.1 of alchemy's
// docs/streaming-plan.md). The Go port of ts/src/tree.ts: the tree a host
// translates, whose events the part, alchemy/transduce.alc, makes from the
// rule passes of Make().
//
// The tree parser reads ABNF with the meta-grammar AbnfRules, its lexer
// and every alternate as they are, and actions of its own (treeRef below)
// under the names the IR parser's take, so the rule passes a part reads
// are the meta-grammar's. A numeric value is read by its atom's action,
// which gives its token the element it spells as its value. Objects are
// *tabnas.OrderedMap, so that their members keep the schema's order;
// numbers are float64.
//
// The tree holds what the schema allows, so the parser refuses what it
// cannot hold, each with a code of its own: a numeric value past U+10FFFF
// (abnf_code_point), a range that ends below where it starts
// (abnf_reversed_range), a repetition count past 2^53 - 1 (abnf_count), a
// repetition whose most is below its least (abnf_reversed_repetition) and
// a group or an option left open (abnf_unclosed).

import (
	"strconv"
	"strings"
	"sync"
	"unicode/utf16"

	tabnas "github.com/tabnas/parser/go"
)

// maxCount is the largest repetition count the tree holds exactly: 2^53 - 1.
const maxCount = 9007199254740991

// treeErrors and treeHints are the codes the tree parser refuses a source
// with, beside the engine's.
var treeErrors = map[string]string{
	"abnf_code_point":          "the numeric value {src} names no Unicode code point (the largest is %x10FFFF)",
	"abnf_reversed_range":      "the numeric range {src} ends below where it starts",
	"abnf_reversed_repetition": "the repetition allows fewer than it requires: its most is below its least",
	"abnf_count":               "the repetition count {src} is past 9007199254740991, the largest whole number the tree holds",
	"abnf_unclosed":            "a group or an option is not closed",
}

var treeHints = map[string]string{
	"abnf_code_point":          "A numeric value names a code point from %x0 to %x10FFFF.",
	"abnf_reversed_range":      "Write the range from its lower end to its higher, as %x61-7A.",
	"abnf_reversed_repetition": "In m*n the n is the most and the m the least, so n is not below m.",
	"abnf_count":               "A repetition count is a whole number no larger than 9007199254740991.",
	"abnf_unclosed":            "Close the group with ) and the option with ].",
}

// treeObject is an object of the tree, its members in the order given.
func treeObject(kv ...any) *tabnas.OrderedMap {
	m := tabnas.NewOrderedMap()
	for i := 0; i+1 < len(kv); i += 2 {
		m.Set(kv[i].(string), kv[i+1])
	}
	return m
}

// treeList is a list a rule builds, held by pointer so that a child rule,
// which inherits the node, appends to its parent's.
type treeList = []any

// leadingInt reads the longest prefix of text that is digits in radix, as
// the canonical parseInt(text, radix) does, and whether there was one and
// it is no larger than limit.
func leadingInt(text string, radix int, limit uint64) (uint64, bool) {
	var n uint64
	digits := 0
	for _, c := range strings.ToLower(text) {
		d := -1
		switch {
		case '0' <= c && c <= '9':
			d = int(c - '0')
		case 'a' <= c && c <= 'z':
			d = int(c-'a') + 10
		}
		if d < 0 || radix <= d {
			break
		}
		digits++
		if n > (limit-uint64(d))/uint64(radix) {
			return 0, false
		}
		n = n*uint64(radix) + uint64(d)
	}
	return n, 0 < digits
}

// numericElement is the element a numeric value spells, or the code it is
// refused with. The digits are read as the canonical runtime reads them,
// the longest prefix valid in the stated base, and a concatenation's code
// points as one UTF-16 sequence, so that a surrogate pair in it is the
// character it encodes.
func numericElement(src string) (*tabnas.OrderedMap, string) {
	radix := 16
	switch strings.ToLower(src[1:2]) {
	case "d":
		radix = 10
	case "b":
		radix = 2
	}
	body := src[2:]
	point := func(text string) (float64, bool) {
		n, ok := leadingInt(text, radix, 0x10ffff)
		return float64(n), ok
	}

	if strings.Contains(body, "-") {
		parts := strings.Split(body, "-")
		from, okFrom := point(parts[0])
		to, okTo := point(parts[1])
		if !okFrom || !okTo {
			return nil, "abnf_code_point"
		}
		if to < from {
			return nil, "abnf_reversed_range"
		}
		return treeObject("type", "range", "from", from, "to", to), ""
	}

	parts := strings.Split(body, ".")
	points := make([]float64, len(parts))
	for i, p := range parts {
		n, ok := point(p)
		if !ok {
			return nil, "abnf_code_point"
		}
		points[i] = n
	}
	if 1 == len(points) {
		return treeObject("type", "char", "value", points[0]), ""
	}
	units := make([]uint16, 0, len(points))
	for _, n := range points {
		units = append(units, utf16.Encode([]rune{rune(n)})...)
		if 0xd800 <= n && n <= 0xdfff {
			units[len(units)-1] = uint16(n)
		}
	}
	text := utf16.Decode(units)
	// A concatenation that spells one character (a surrogate pair's two
	// halves) is that character.
	if 1 == len(text) {
		return treeObject("type", "char", "value", float64(text[0])), ""
	}
	return treeObject("type", "literal", "text", string(text), "caseSensitive", true), ""
}

// copyObject is a shallow copy of an object of the tree.
func copyObject(m *tabnas.OrderedMap) *tabnas.OrderedMap {
	out := tabnas.NewOrderedMap()
	for _, k := range m.Keys {
		out.Set(k, m.Vals[k])
	}
	return out
}

// countOf reads a repetition count from its token, or raises abnf_count.
func countOf(tkn *tabnas.Token, ctx *tabnas.Context) (float64, bool) {
	n, ok := leadingInt(tkn.Src, 10, maxCount)
	if !ok {
		ctx.ParseErr = tkn.Bad("abnf_count")
		return 0, false
	}
	return float64(n), true
}

// listOf reads the list a child rule built.
func listOf(r *tabnas.Rule) (treeList, bool) {
	if r == nil || r == tabnas.NoRule {
		return nil, false
	}
	p, ok := r.Node.(*treeList)
	if !ok {
		return nil, false
	}
	return append(treeList{}, (*p)...), true
}

// treeRef is the tree parser's actions, under the names the IR parser's
// take in AbnfRules.
func treeRef() map[tabnas.FuncRef]any {
	return map[tabnas.FuncRef]any{
		"@abnf-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = treeObject("type", "grammar", "rules", []any{})
		}),

		"@prod-bc": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			alts, ok := listOf(r.Child)
			if !ok {
				return
			}
			rule := treeObject("type", "rule", "name", r.EnsureU()["name"])
			if inc, _ := r.EnsureU()["incremental"].(bool); inc {
				rule.Set("incremental", true)
			}
			rule.Set("alternatives", alts)
			if g, ok := r.Node.(*tabnas.OrderedMap); ok {
				rules, _ := g.Get("rules")
				list, _ := rules.([]any)
				g.Set("rules", append(list, rule))
			}
		}),
		"@prod-name": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["name"] = tokString(r.O[0], r, ctx)
		}),
		// `<all> = <remove>`: the name keeps its angle brackets.
		"@prod-name-prose": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["name"] = r.O[0].Src
		}),
		"@prod-name-inc": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["name"] = tokString(r.O[0], r, ctx)
			r.EnsureU()["incremental"] = true
		}),

		"@alts-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = &treeList{}
		}),
		"@alts-bc": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			seq, ok := listOf(r.Child)
			if !ok {
				return
			}
			if p, ok := r.Node.(*treeList); ok {
				*p = append(*p, seq)
			}
		}),

		"@seq-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = &treeList{}
		}),

		"@elem-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			delete(r.EnsureU(), "repeat")
		}),
		"@elem-rep-bounded": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			min, ok := countOf(r.O[0], ctx)
			if !ok {
				return
			}
			max, ok := countOf(r.O[2], ctx)
			if !ok {
				return
			}
			if max < min {
				ctx.ParseErr = r.O[0].Bad("abnf_reversed_repetition")
				return
			}
			r.EnsureU()["repeat"] = treeObject("min", min, "max", max)
		}),
		"@elem-rep-atleast": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			if min, ok := countOf(r.O[0], ctx); ok {
				r.EnsureU()["repeat"] = treeObject("min", min)
			}
		}),
		"@elem-rep-atmost": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			if max, ok := countOf(r.O[1], ctx); ok {
				r.EnsureU()["repeat"] = treeObject("min", float64(0), "max", max)
			}
		}),
		"@elem-rep-star": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["repeat"] = treeObject("min", float64(0))
		}),
		"@elem-rep-exact": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			if n, ok := countOf(r.O[0], ctx); ok {
				r.EnsureU()["repeat"] = treeObject("min", n, "max", n)
			}
		}),
		"@elem-close": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			var el *tabnas.OrderedMap
			if r.Child != nil && r.Child != tabnas.NoRule {
				el, _ = r.Child.Node.(*tabnas.OrderedMap)
			}
			// An atom that built nothing is a group or an option the
			// source ended inside.
			if el == nil {
				ctx.ParseErr = ctx.T0.Bad("abnf_unclosed")
				return
			}
			if rep, ok := r.EnsureU()["repeat"].(*tabnas.OrderedMap); ok {
				reps, _ := el.Get("repeat")
				list, _ := reps.([]any)
				el.Set("repeat", append(list, rep))
			}
			if p, ok := r.Node.(*treeList); ok {
				*p = append(*p, el)
			}
		}),

		"@atom-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = nil
		}),
		"@atom-ss": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = treeObject("type", "literal", "text", tokString(r.O[1], r, ctx), "caseSensitive", true)
		}),
		"@atom-si": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = treeObject("type", "literal", "text", tokString(r.O[1], r, ctx), "caseSensitive", false)
		}),
		"@atom-st": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = treeObject("type", "literal", "text", tokString(r.O[0], r, ctx), "caseSensitive", false)
		}),
		// The element the value spells, which its token carries from here.
		"@atom-nv": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			el, code := numericElement(r.O[0].Src)
			if "" != code {
				ctx.ParseErr = r.O[0].Bad(code)
				return
			}
			r.O[0].Val = el
			r.Node = copyObject(el)
		}),
		"@atom-pv": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			src := r.O[0].Src
			r.Node = treeObject("type", "prose", "text", src[1:len(src)-1])
		}),
		"@atom-tx": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = treeObject("type", "reference", "name", tokString(r.O[0], r, ctx))
		}),
		"@atom-lp": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["groupKind"] = "group"
		}),
		"@atom-ob": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["groupKind"] = "opt"
		}),
		"@atom-group-c": tabnas.AltCond(func(r *tabnas.Rule, _ *tabnas.Context) bool {
			return r.EnsureU()["groupKind"] == "group"
		}),
		"@atom-group-close": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			alts, _ := listOf(r.Child)
			r.Node = treeObject("type", "group", "alternatives", alts)
		}),
		"@atom-opt-c": tabnas.AltCond(func(r *tabnas.Rule, _ *tabnas.Context) bool {
			return r.EnsureU()["groupKind"] == "opt"
		}),
		"@atom-opt-close": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			alts, _ := listOf(r.Child)
			r.Node = treeObject("type", "group", "alternatives", alts,
				"repeat", []any{treeObject("min", float64(0), "max", float64(1))})
		}),
	}
}

// Make builds a fresh parser that reads an ABNF document as the tree: the
// parser a host streams a document through with the part, whose rule
// passes the part reads, and whose value is the tree, an
// *tabnas.OrderedMap (nil for an empty source, which makes no pass).
func Make() *tabnas.Tabnas {
	j, err := newAbnfParser(treeRef(), tabnas.Options{Error: treeErrors, Hint: treeHints})
	if err != nil {
		// The meta-grammar is this package's own, so it always installs.
		panic("abnf: the tree parser does not install: " + err.Error())
	}
	return j
}

// parseError is an engine error as the tree's reader reports it.
func parseError(err error) *AbnfParseError {
	line, col := errLineCol(err)
	loc := ""
	if line != 0 && col != 0 {
		loc = " at line " + strconv.Itoa(line) + ", column " + strconv.Itoa(col)
	}
	raw := strings.SplitN(err.Error(), "\n", 2)[0]
	return &AbnfParseError{
		Message: "abnf: parse error" + loc + ": " + raw,
		Line:    line, Column: col, Cause: err,
	}
}

var (
	treeParserOnce sync.Once
	treeParserInst *tabnas.Tabnas
)

// ParseAbnfTree reads an ABNF document whole as the tree. A source the
// meta-grammar refuses, or the tree cannot hold, fails with an
// *AbnfParseError naming the line and column, its Cause the engine's error
// with its code.
func ParseAbnfTree(src string) (*tabnas.OrderedMap, error) {
	treeParserOnce.Do(func() { treeParserInst = Make() })
	out, err := treeParserInst.Parse(src)
	if err != nil {
		return nil, parseError(err)
	}
	if tree, ok := out.(*tabnas.OrderedMap); ok {
		return tree, nil
	}
	// An empty source makes no rule pass, and the engine answers nothing
	// for it: it is the grammar of no rules, as a source of comments is.
	return treeObject("type", "grammar", "rules", []any{}), nil
}
