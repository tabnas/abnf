// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// converter.go — ABNF grammar AST -> tabnas GrammarSpec. The Go port of
// the transformation pipeline in ts/src/converter.ts: parseAbnf,
// mergeIncrementals, core rules, eliminateLeftRecursion (Paull's),
// rewriteProbeDispatches, desugar, FIRST sets, and emitGrammarSpec.

import (
	"fmt"
	"math/big"
	"regexp"
	"sort"
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
	// BEFORE merging, not after. mergeIncrementals drops each `=/`
	// production, keeping only the base's span — so an annotation on an
	// incremental line was resolved against a production list that no
	// longer contained the line it followed, and attached to whatever rule
	// happened to be declared before it instead. `a = "a"`, `b = 1*DIGIT`,
	// `a =/ "c" ; @object` silently annotated **b**. TS moved first; this
	// mirrors it, which also puts an annotation diagnostic ahead of the
	// incremental-merge one in both runtimes.
	if aerr := attachValueAnnotations(src, productions); aerr != nil {
		return nil, aerr
	}
	merged, merr := mergeIncrementals(productions)
	if merr != nil {
		return nil, merr
	}
	// LAST of the three, deliberately — this is TS's order, and error
	// precedence is itself part of the contract. TS reports the numeric
	// diagnostic first (it checks the code point eagerly inside
	// parseNumericValue), then the incremental-merge error, and only then the
	// malformed element (its rejectHoles lives in withCoreRules, after the
	// merge). A rejection naming a different cause in each runtime is exactly
	// the divergence this repair exists to close, so the order is pinned by
	// test/spec/alignment-abnf-errors.tsv, which both suites run.
	if herr := rejectHoles(merged); herr != nil {
		return nil, herr
	}
	withCore := withCoreRules(merged)
	return &abnfGrammar{Productions: withCore}, nil
}

// A trailing comment claiming a value annotation:
//
//	ver = maj "." min "." pat    ; @object maj min pat
//	tags = tag *("," tag)        ; @array
//
// RFC 5234 has nowhere else to put this. A comment is the only place in
// the notation that carries no meaning of its own, which is exactly why it
// can carry one here without changing what the grammar accepts: strip
// every annotation and the same language parses, just into a tree instead
// of a value.
//
// ONLY "@object" and "@array" are claimed. Any other `; @…` comment is
// left alone — the notation has no directive namespace, so this must not
// assume one, and a reader's own `; @deprecated` has to keep meaning
// nothing. Mirrors ts/src/converter.ts.
var annotationRe = regexp.MustCompile(`^@(object|array)\b\s*(.*)$`)
var memberNameRe = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9-]*$`)

type annotationComment struct {
	at   int
	body string
}

// annotationComments finds every `;` comment in the source that claims an
// annotation, with the offset it starts at.
//
// Quoted strings and prose are skipped: a `;` inside `"a;b"` or `<a;b>` is
// CONTENT, not a comment, and treating it as one would silently attach an
// annotation the author did not write.
func annotationComments(src string) []annotationComment {
	var out []annotationComment
	for i := 0; i < len(src); i++ {
		switch src[i] {
		case '"', '<':
			// RFC 5234 char-val and prose-val have no escapes, so the next
			// closing mark ends them.
			closer := byte('"')
			if src[i] == '<' {
				closer = '>'
			}
			j := strings.IndexByte(src[i+1:], closer)
			if j < 0 {
				return out
			}
			i += 1 + j
			continue
		case ';':
			end := strings.IndexByte(src[i:], '\n')
			if end < 0 {
				end = len(src)
			} else {
				end += i
			}
			body := strings.TrimSpace(src[i+1 : end])
			if strings.HasPrefix(body, "@") {
				out = append(out, annotationComment{at: i, body: body})
			}
			i = end
		}
	}
	return out
}

// attachValueAnnotations attaches each annotation to the production it
// FOLLOWS — the last one that begins before it.
//
// Not "the production on the same line": a rule may be written across
// several lines, and an author putting the annotation on the last of them
// means the same thing. Following the definition is the rule that reads
// the same either way.
func attachValueAnnotations(src string, prods []*abnfProduction) error {
	ordered := make([]*abnfProduction, 0, len(prods))
	for _, p := range prods {
		if p.Sp != nil {
			ordered = append(ordered, p)
		}
	}
	if len(ordered) == 0 {
		return nil
	}
	sort.SliceStable(ordered, func(i, j int) bool {
		return ordered[i].Sp.S < ordered[j].Sp.S
	})

	idx := 0
	for _, c := range annotationComments(src) {
		m := annotationRe.FindStringSubmatch(c.body)
		if m == nil {
			continue
		}

		// Both `ordered` and the comments are in source order, so the
		// search only ever moves FORWARD — idx is not reset per comment.
		// Restarting it made attachment quadratic in the number of
		// annotated rules, which a generated grammar can make expensive
		// for nothing.
		for idx < len(ordered) && ordered[idx].Sp.S < c.at {
			idx++
		}
		var owner *abnfProduction
		if idx > 0 {
			owner = ordered[idx-1]
		}
		if owner == nil {
			return &AbnfParseError{Message: fmt.Sprintf(
				"abnf: '; %s' appears before any rule, so there is nothing for "+
					"it to annotate. A value annotation goes after the rule it "+
					"describes.", c.body)}
		}

		kind := m[1]
		var members []string
		for _, w := range strings.FieldsFunc(m[2], func(r rune) bool {
			return r == ' ' || r == '\t' || r == ','
		}) {
			if w != "" {
				members = append(members, w)
			}
		}

		if kind == "array" {
			if len(members) > 0 {
				return &AbnfParseError{Message: fmt.Sprintf(
					"abnf: rule '%s': '@array' names no members — every part "+
						"that produces a value becomes an element, in order. Got "+
						"'%s'.", owner.Name, strings.Join(members, " "))}
			}
		} else {
			seen := map[string]bool{}
			for _, name := range members {
				if !memberNameRe.MatchString(name) {
					return &AbnfParseError{Message: fmt.Sprintf(
						"abnf: rule '%s': '%s' is not a rule name, so it cannot "+
							"name a member of '@object'.", owner.Name, name)}
				}
				// Each member is a separate KEY. Two parts named the same
				// thing both write to it, so the second silently overwrites
				// the first and that much of the input is gone.
				if seen[name] {
					return &AbnfParseError{Message: fmt.Sprintf(
						"abnf: rule '%s': '@object' names '%s' twice. Each "+
							"member is a separate key, so the second part would "+
							"overwrite the first. Give them different names.",
						owner.Name, name)}
				}
				seen[name] = true
			}
		}

		if owner.Value != nil {
			return &AbnfParseError{Message: fmt.Sprintf(
				"abnf: rule '%s' has more than one value annotation. A rule "+
					"builds one thing.", owner.Name)}
		}
		if kind == "array" {
			owner.Value = &ValueAnnotation{Kind: kind}
		} else {
			owner.Value = &ValueAnnotation{Kind: kind, Members: members}
		}
	}
	return nil
}

// kindHole marks an element the parser could not build. It NEVER escapes the
// converter: rejectHoles below runs before any other pass and always errors
// when one is present, so no downstream walker has to know about it. It exists
// only so a hole can carry the diagnostic rescued from the subtree that was
// dropped with it — see @elem-close in parser_abnf.go.
const kindHole = bnf.ElemKind("abnf-hole")

// firstNumErrInAlts finds the first deferred numeric diagnostic in a subtree
// that is about to be discarded. `bad = ( %x110000` drops the whole group when
// it never closes, taking the offending element with it — and TS, which checks
// the code point eagerly inside parseNumericValue, reports the numeric fault
// rather than the unclosed group. Carrying the message out on the hole is what
// lets the deferred Go check reach the same verdict.
func firstNumErrInAlts(alts []abnfSequence) string {
	for _, alt := range alts {
		for _, el := range alt {
			if msg := walkNumErr(el); "" != msg {
				return msg
			}
		}
	}
	return ""
}

// rejectHoles refuses a production containing an element the parser could
// not build. It is the Go counterpart of `rejectHoles` in ts/src/converter.ts
// and walks the same shape for the same reason: `bad = *( "a"` leaves the hole
// in a repetition's Inner rather than directly in the sequence, so a top-level
// scan misses it.
//
// The two runtimes reached this from opposite directions. TS left the hole in
// place and bnf's refsIn nil-dereferenced it — a crash the conformance harness
// scored as a correct rejection. Go dropped the element silently, so the same
// input COMPILED. ADR-13: TypeScript defines the language, so Go rejects too,
// and the `unbalanced-group` / `unbalanced-option` rows come out of
// known-gaps.tsv.
func rejectHoles(prods []*abnfProduction) *AbnfParseError {
	var holeIn func(alt abnfSequence) bool
	holeIn = func(alt abnfSequence) bool {
		for _, el := range alt {
			if nil == el || kindHole == el.Kind {
				return true
			}
			switch el.Kind {
			case kindOpt, kindStar, kindPlus, kindRep:
				if holeIn(abnfSequence{el.Inner}) {
					return true
				}
			case kindGroup:
				for _, a := range el.Alts {
					if holeIn(a) {
						return true
					}
				}
			}
		}
		return false
	}
	for _, p := range prods {
		for _, alt := range p.Alts {
			if holeIn(alt) {
				return &AbnfParseError{Message: fmt.Sprintf(
					"abnf: rule '%s' is malformed — an element could not be "+
						"built. The usual cause is an unclosed group or option.",
					p.Name)}
			}
		}
	}
	return nil
}

// findNumErr returns the first deferred numeric-value diagnostic recorded
// anywhere in the parsed productions, or "" when every numeric value was a
// valid Unicode code point. Walks nested groups and repetitions.
func findNumErr(prods []*abnfProduction) string {
	for _, p := range prods {
		for _, alt := range p.Alts {
			for _, el := range alt {
				if msg := walkNumErr(el); "" != msg {
					return msg
				}
			}
		}
	}
	return ""
}

// walkNumErr is findNumErr for a single element and its descendants.
func walkNumErr(el *abnfElement) string {
	// A hole. rejectHoles reports it, but only after this walk, so skip
	// rather than dereference.
	if nil == el {
		return ""
	}
	if "" != el.NumErr {
		return el.NumErr
	}
	switch el.Kind {
	case kindOpt, kindStar, kindPlus, kindRep:
		if nil != el.Inner {
			return walkNumErr(el.Inner)
		}
	case kindGroup:
		for _, alt := range el.Alts {
			for _, inner := range alt {
				if msg := walkNumErr(inner); "" != msg {
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
			// This production is about to be dropped, and annotations are
			// now attached before that happens — so an annotation on the
			// `=/` line has to move to the base, which IS the rule it
			// describes.
			if p.Value != nil {
				if base.Value != nil {
					return nil, &AbnfParseError{Message: fmt.Sprintf(
						"abnf: rule '%s' has more than one value annotation. A "+
							"rule builds one thing.", p.Name)}
				}
				base.Value = p.Value
			}
			continue
		}
		// Rebuilt field by field, so every field carried on a production
		// has to be listed here or it is silently dropped — Sp included.
		clean := &abnfProduction{Name: p.Name, Alts: p.Alts, Sp: p.Sp}
		if p.NodeKind != "" {
			clean.NodeKind = p.NodeKind
		}
		// Annotations are attached BEFORE this runs, so this carry is live:
		// without it every annotation in the grammar would vanish here
		// without a word. This rebuild is exactly the shape that drops a
		// field silently, and the compiler downstream had six of them and
		// shipped with all six dropping it.
		if p.Value != nil {
			clean.Value = p.Value
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
