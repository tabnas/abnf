// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// converter.go — ABNF grammar AST -> tabnas GrammarSpec. The Go port of
// the transformation pipeline in ts/src/converter.ts: parseAbnf,
// mergeIncrementals, core rules, eliminateLeftRecursion (Paull's),
// rewriteProbeDispatches, desugar, FIRST sets, and emitGrammarSpec.

import (
	"fmt"
	"math/big"
	"strconv"
	"strings"

	bnf "github.com/tabnas/bnf/go"
	tabnas "github.com/tabnas/parser/go"
)

// AbnfParseError is raised when the ABNF source itself can't be parsed.
type AbnfParseError struct {
	Message string
	Line    int
	Column  int
	Cause   error
}

func (e *AbnfParseError) Error() string { return e.Message }
func (e *AbnfParseError) Unwrap() error { return e.Cause }

// ---- parseAbnf ------------------------------------------------------

// parseAbnf parses ABNF source into a grammar AST via the tabnas-based
// parser, merging incrementals and splicing in referenced core rules.
func parseAbnf(src string) (*abnfGrammar, error) {
	productions, err := parseAbnfRaw(src)
	if err != nil {
		line, col := errLineCol(err)
		loc := ""
		if line != 0 && col != 0 {
			loc = fmt.Sprintf(" at line %d, column %d", line, col)
		}
		raw := strings.SplitN(err.Error(), "\n", 2)[0]
		return nil, &AbnfParseError{
			Message: fmt.Sprintf("abnf: parse error%s: %s", loc, raw),
			Line:    line, Column: col, Cause: err,
		}
	}
	if len(productions) == 0 {
		return nil, &AbnfParseError{Message: "abnf: no productions found"}
	}
	// Surface any deferred numeric-value diagnostic now that the parse is
	// structurally complete (see abnfElement.NumErr).
	if msg := findNumErr(productions); msg != "" {
		return nil, &AbnfParseError{
			Message: "abnf: parse error: " + msg}
	}
	merged, merr := mergeIncrementals(productions)
	if merr != nil {
		return nil, merr
	}
	withCore := withCoreRules(merged)
	return &abnfGrammar{Productions: withCore}, nil
}

// findNumErr returns the first deferred numeric-value diagnostic recorded
// anywhere in the parsed productions, or "" when every numeric value was a
// valid Unicode code point. Walks nested groups and repetitions.
func findNumErr(prods []*abnfProduction) string {
	var walk func(el *abnfElement) string
	walk = func(el *abnfElement) string {
		if el.NumErr != "" {
			return el.NumErr
		}
		switch el.Kind {
		case kindOpt, kindStar, kindPlus, kindRep:
			if el.Inner != nil {
				return walk(el.Inner)
			}
		case kindGroup:
			for _, alt := range el.Alts {
				for _, inner := range alt {
					if msg := walk(inner); msg != "" {
						return msg
					}
				}
			}
		}
		return ""
	}
	for _, p := range prods {
		for _, alt := range p.Alts {
			for _, el := range alt {
				if msg := walk(el); msg != "" {
					return msg
				}
			}
		}
	}
	return ""
}

// errLineCol attempts to pull line/column from a tabnas parse error.
func errLineCol(err error) (int, int) {
	if te, ok := err.(*tabnas.TabnasError); ok {
		return te.Row, te.Col
	}
	return 0, 0
}

// ---- merge incrementals --------------------------------------------

func mergeIncrementals(prods []*abnfProduction) ([]*abnfProduction, error) {
	out := []*abnfProduction{}
	byName := map[string]*abnfProduction{}
	for _, p := range prods {
		if p.Incremental {
			base := byName[p.Name]
			if base == nil {
				return nil, &AbnfParseError{Message: fmt.Sprintf(
					"abnf: '%s =/ …' has no earlier '%s = …' to extend", p.Name, p.Name)}
			}
			base.Alts = append(base.Alts, p.Alts...)
			continue
		}
		// Rebuilt field by field, so every field carried on a production
		// has to be listed here or it is silently dropped — Sp included.
		clean := &abnfProduction{Name: p.Name, Alts: p.Alts, Sp: p.Sp}
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
LWSP   = *( WSP / CRLF WSP )
`

// coreRuleList returns the parsed core rules (order-preserving) with
// nodeKind=core. Parsed on each call; the parser instance is cached.
func coreRuleList() []*abnfProduction {
	raw, err := parseAbnfRaw(coreRulesABNF)
	if err != nil {
		panic("abnf: internal — core rules failed to parse: " + err.Error())
	}
	for _, p := range raw {
		p.NodeKind = "core"
		// Strip source spans. These are parsed from coreRulesABNF, a
		// string in THIS FILE, so their offsets index a document the user
		// never wrote — an editor asked to reveal one would jump to a
		// position in the user's grammar that has nothing to do with
		// ALPHA or DIGIT. A missing span means "nowhere to point", which
		// is exactly right for a rule the library supplied; a wrong one
		// is worse than none.
		//
		// A reference TO a core rule still carries a span: that reference
		// is in the user's source, and it is what a diagnostic points at.
		stripSpans(p)
	}
	return raw
}

// withCoreRules adds each RFC 5234 core rule that the user references
// but doesn't define locally. Resolution is transitive.
func withCoreRules(user []*abnfProduction) []*abnfProduction {
	core := coreRuleList()
	coreByName := map[string]*abnfProduction{}
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
	scan := func(prods []*abnfProduction) {
		for _, p := range prods {
			for _, alt := range p.Alts {
				refsIn(alt, needed)
			}
		}
	}
	scan(user)
	out := []*abnfProduction{}
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
			scan([]*abnfProduction{prod})
			added = true
		}
	}
	return append(append([]*abnfProduction{}, user...), out...)
}

// ---- numeric value -------------------------------------------------

func parseNumericValue(src string, tkn *tabnas.Token) *abnfElement {
	sp := spanOf(tkn)
	base := strings.ToLower(string(src[1]))
	radix := 16
	if base == "d" {
		radix = 10
	} else if base == "b" {
		radix = 2
	}
	body := src[2:]

	// RFC 5234 puts no ceiling on a numeric value, but Unicode does: nothing
	// above U+10FFFF is a code point. Check it here so an out-of-range
	// grammar gets an ABNF diagnostic naming the offending value, rather than
	// the silent U+FFFD that `string(rune(n))` yields. The message is
	// recorded on the element rather than returned, because the caller is an
	// engine alt-action with no error return — see abnfElement.NumErr.
	// Mirrors the TS parseNumericValue check, whose message it reproduces
	// byte for byte.
	NumErr := ""
	codePoint := func(text string) int64 {
		n, err := strconv.ParseInt(text, radix, 64)
		if err != nil || n < 0 || 0x10FFFF < n {
			shown := strconv.FormatInt(n, 10)
			if err != nil {
				// Overflowed int64 — report the digits as written, in the
				// same base-10 form the TS side prints.
				if v, ok := new(big.Int).SetString(text, radix); ok {
					shown = v.String()
				} else {
					shown = text
				}
			}
			if NumErr == "" {
				NumErr = fmt.Sprintf(
					"numeric value '%%%s%s' is %s, which is not a Unicode code "+
						"point (the maximum is %%x10FFFF).",
					string(src[1]), text, shown)
			}
			return 0
		}
		return n
	}

	if strings.Contains(body, "-") {
		parts := strings.SplitN(body, "-", 2)
		lo := codePoint(parts[0])
		hi := codePoint(parts[1])
		if lo == hi {
			return &abnfElement{
				Kind: kindTerm, Literal: string(rune(lo)), NumErr: NumErr, Sp: sp}
		}
		toEsc := func(n int64) string {
			return fmt.Sprintf("\\x{%04x}", n)
		}
		return &abnfElement{
			Kind:    kindRegex,
			Pattern: "[" + toEsc(lo) + "-" + toEsc(hi) + "]",
			Flags:   "",
			NumErr:  NumErr,
			Sp:      sp,
		}
	}

	parts := strings.Split(body, ".")
	var sb strings.Builder
	for _, n := range parts {
		sb.WriteRune(rune(codePoint(n)))
	}
	return &abnfElement{Kind: kindTerm, Literal: sb.String(), NumErr: NumErr, Sp: sp}
}

// spanOf is the source span of a token, for the IR (bnf.SrcSpan). Every
// field is copied straight off the token: the compiler stores whatever
// units the front-end's own engine tokens use, precisely so that no
// arithmetic — and so no off-by-one — happens at this boundary. Go
// tokens carry no length, so the end comes from the matched source.
func spanOf(tkn *tabnas.Token) *bnf.SrcSpan {
	if tkn == nil {
		return nil
	}
	return &bnf.SrcSpan{
		S: tkn.SI, E: tkn.SI + len(tkn.Src), R: tkn.RI, C: tkn.CI,
	}
}

// spanTo is one span covering two tokens — a group runs from its `(` to
// its `)`, a bracketed optional from `[` to `]`. Falls back to whichever
// end is known when the other is not.
func spanTo(from, to *tabnas.Token) *bnf.SrcSpan {
	a := spanOf(from)
	b := spanOf(to)
	if a == nil {
		return b
	}
	if b == nil {
		return a
	}
	return &bnf.SrcSpan{S: a.S, E: b.E, R: a.R, C: a.C}
}

// stripSpans removes every span from a production and everything under
// it. Used for the RFC 5234 core rules, which are parsed from a string
// in this file rather than from the user's grammar.
func stripSpans(prod *abnfProduction) {
	prod.Sp = nil
	var walk func(el *abnfElement)
	walk = func(el *abnfElement) {
		if el == nil {
			return
		}
		el.Sp = nil
		walk(el.Inner)
		for _, alt := range el.Alts {
			for _, e := range alt {
				walk(e)
			}
		}
	}
	for _, alt := range prod.Alts {
		for _, el := range alt {
			walk(el)
		}
	}
}
