// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// parser_abnf.go — the ABNF grammar itself, expressed as a tabnas
// GrammarSpec installed on a bare engine. This is the Go port of the TS
// `abnfRules` table + `getAbnfParser`. The ABNF source is parsed by a
// tabnas instance whose grammar is defined here; AST assembly happens in
// the bo/bc/a closures registered in the Ref map.
//
// Token vocabulary (mirrors the TS comment):
//
//	#DEF   `=`  (rule-definition operator)
//	#DEFA  `=/` (incremental-alternatives operator)
//	#ALT   `/`  (alternation)
//	#STAR  `*`  (repetition separator)
//	#NUM   decimal repetition count (match.token)
//	#NV    `%[xdb]NN[(-NN|(.NN)*)]` numeric value (match.token)
//	#SS    `%s` (case-sensitive string prefix)
//	#SI    `%i` (case-insensitive string prefix)
//	#LP    `(`
//	#RP    `)`
//	#OB    `[` (optional-group open)
//	#CB    `]` (optional-group close)
//	#TX    bare identifier (default text token)
//	#ST    quoted string literal (default string token)
//	#ZZ    end-of-source

import (
	"fmt"
	"regexp"
	"strconv"
	"sync"

	tabnas "github.com/tabnas/parser/go"
)

// asString resolves a matched token's value to a Go string, calling
// ResolveVal so a lazy value func is unwrapped (mirrors the TS `.val`
// getter).
func tokString(t *tabnas.Token, r *tabnas.Rule, ctx *tabnas.Context) string {
	v := t.ResolveVal(r, ctx)
	if s, ok := v.(string); ok {
		return s
	}
	return t.Src
}

// productionList is the slice type accumulated on the abnf rule's node.
type productionList = []*abnfProduction

// childAlts reads the alts ([]abnfSequence) a child `alts` rule produced.
// The child node is a *[]abnfSequence (pointer for reference semantics).
func childAlts(r *tabnas.Rule) []abnfSequence {
	if r.Child == nil || r.Child == tabnas.NoRule {
		return nil
	}
	if p, ok := r.Child.Node.(*[]abnfSequence); ok {
		return *p
	}
	return nil
}

// abnfParseRef builds the Ref map of AST-assembly closures for the ABNF
// parser grammar. State actions are wired by the engine via the
// @<rule>-bo / @<rule>-bc reserved names.
func abnfParseRef() map[tabnas.FuncRef]any {
	// Node types use POINTERS to slices so that an append performed by a
	// child rule (which inherits the parent's node by reference, as the
	// engine passes r.Node down on push/replace) is visible to the
	// parent — matching the TS reference-array semantics. abnf.node is a
	// *productionList; alts.node a *[]abnfSequence; seq.node a *abnfSequence.
	return map[tabnas.FuncRef]any{
		// --- abnf (top level) ---
		"@abnf-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = &productionList{}
		}),

		// --- prod ---
		"@prod-bc": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			if r.Child != nil && r.Child != tabnas.NoRule && r.Child.Node != nil {
				if altsPtr, ok := r.Child.Node.(*[]abnfSequence); ok {
					nameTkn, _ := r.EnsureU()["nameTkn"].(*tabnas.Token)
					prod := &abnfProduction{
						Name: asStr(r.EnsureU()["name"]),
						Alts: *altsPtr,
						// The name, not the body: that is what an outline
						// entry, go-to-definition and a whole-rule
						// diagnostic want, and an ABNF body can run over
						// many folded lines.
						Sp: spanOf(nameTkn),
					}
					if b, _ := r.EnsureU()["incremental"].(bool); b {
						prod.Incremental = true
					}
					if listPtr, ok := r.Node.(*productionList); ok {
						*listPtr = append(*listPtr, prod)
					}
				}
			}
		}),
		"@prod-name": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["name"] = tokString(r.O[0], r, ctx)
			r.EnsureU()["nameTkn"] = r.O[0]
			r.EnsureU()["incremental"] = false
		}),
		// `<all> = …`: the production name is the prose token's raw source,
		// angle brackets and all, so it can never collide with a real
		// rulename. Mirrors the TS `#PV #DEF` prod alternative.
		"@prod-name-prose": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["name"] = r.O[0].Src
			r.EnsureU()["nameTkn"] = r.O[0]
			r.EnsureU()["incremental"] = false
		}),
		"@prod-name-inc": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["name"] = tokString(r.O[0], r, ctx)
			r.EnsureU()["nameTkn"] = r.O[0]
			r.EnsureU()["incremental"] = true
		}),

		// --- alts ---
		"@alts-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = &[]abnfSequence{}
		}),
		"@alts-bc": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			if r.Child != nil && r.Child != tabnas.NoRule && r.Child.Node != nil {
				if seqPtr, ok := r.Child.Node.(*abnfSequence); ok {
					if listPtr, ok := r.Node.(*[]abnfSequence); ok {
						*listPtr = append(*listPtr, *seqPtr)
					}
				}
			}
		}),

		// --- seq ---
		"@seq-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = &abnfSequence{}
		}),

		// --- elem ---
		"@elem-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["min"] = 1
			r.EnsureU()["max"] = 1
		}),
		"@elem-rep-bounded": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["min"] = atoi(r.O[0].Src)
			r.EnsureU()["max"] = atoi(r.O[2].Src)
		}),
		"@elem-rep-atleast": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["min"] = atoi(r.O[0].Src)
			r.EnsureU()["max"] = maxInfinity
		}),
		"@elem-rep-atmost": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["min"] = 0
			r.EnsureU()["max"] = atoi(r.O[1].Src)
		}),
		"@elem-rep-star": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.EnsureU()["min"] = 0
			r.EnsureU()["max"] = maxInfinity
		}),
		"@elem-rep-exact": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			n := atoi(r.O[0].Src)
			r.EnsureU()["min"] = n
			r.EnsureU()["max"] = n
		}),
		"@elem-close": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			var item *abnfElement
			if r.Child != nil && r.Child != tabnas.NoRule {
				if it, ok := r.Child.Node.(*abnfElement); ok {
					item = it
				}
			}
			if item == nil {
				return
			}
			min := asInt(r.EnsureU()["min"])
			max := asInt(r.EnsureU()["max"])
			var wrapped *abnfElement
			switch {
			case min == 1 && max == 1:
				wrapped = item
			case min == 0 && max == maxInfinity:
				wrapped = &abnfElement{Kind: kindStar, Inner: item}
			case min == 1 && max == maxInfinity:
				wrapped = &abnfElement{Kind: kindPlus, Inner: item}
			case min == 0 && max == 1:
				wrapped = &abnfElement{Kind: kindOpt, Inner: item}
			default:
				wrapped = &abnfElement{Kind: kindRep, Min: min, Max: max, Inner: item}
			}
			// elem inherits seq's node (a *abnfSequence) by reference; append
			// through the pointer so seq sees the new element.
			if seqPtr, ok := r.Node.(*abnfSequence); ok {
				*seqPtr = append(*seqPtr, wrapped)
			}
		}),

		// --- atom ---
		"@atom-bo": tabnas.StateAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = nil
		}),
		"@atom-ss": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = &abnfElement{
				Kind: kindTerm, Literal: tokString(r.O[1], r, ctx),
				CaseSensitive: true, HasCaseSens: true,
				Sp: spanTo(r.O[0], r.O[1]),
			}
		}),
		"@atom-si": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = &abnfElement{
				Kind: kindTerm, Literal: tokString(r.O[1], r, ctx),
				Sp: spanTo(r.O[0], r.O[1]),
			}
		}),
		"@atom-st": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = &abnfElement{
				Kind: kindTerm, Literal: tokString(r.O[0], r, ctx),
				Sp: spanOf(r.O[0]),
			}
		}),
		"@atom-nv": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.Node = parseNumericValue(r.O[0].Src, r.O[0])
		}),
		"@atom-pv": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			// Strip the surrounding `<` and `>`; the resolveProseTerminals
			// pass decides what the prose means.
			src := r.O[0].Src
			r.Node = &abnfElement{
				Kind: kindProse, Text: src[1 : len(src)-1], Sp: spanOf(r.O[0]),
			}
		}),
		"@atom-tx": tabnas.AltAction(func(r *tabnas.Rule, ctx *tabnas.Context) {
			r.Node = &abnfElement{
				Kind: kindRef, Name: tokString(r.O[0], r, ctx), Sp: spanOf(r.O[0]),
			}
		}),
		"@atom-lp": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["groupKind"] = "group"
			r.EnsureU()["open"] = r.O[0]
		}),
		"@atom-ob": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			r.EnsureU()["groupKind"] = "opt"
			r.EnsureU()["open"] = r.O[0]
		}),
		"@atom-group-c": tabnas.AltCond(func(r *tabnas.Rule, _ *tabnas.Context) bool {
			return r.EnsureU()["groupKind"] == "group"
		}),
		"@atom-group-close": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			alts := childAlts(r)
			open, _ := r.EnsureU()["open"].(*tabnas.Token)
			r.Node = &abnfElement{
				Kind: kindGroup, Alts: alts, Sp: spanTo(open, closeTok(r)),
			}
		}),
		"@atom-opt-c": tabnas.AltCond(func(r *tabnas.Rule, _ *tabnas.Context) bool {
			return r.EnsureU()["groupKind"] == "opt"
		}),
		"@atom-opt-close": tabnas.AltAction(func(r *tabnas.Rule, _ *tabnas.Context) {
			alts := childAlts(r)
			open, _ := r.EnsureU()["open"].(*tabnas.Token)
			bracket := spanTo(open, closeTok(r))
			r.Node = &abnfElement{
				Kind:  kindOpt,
				Inner: &abnfElement{Kind: kindGroup, Alts: alts, Sp: bracket},
				Sp:    bracket,
			}
		}),
	}
}

// AbnfRules returns the rule map for the ABNF parser grammar — the
// grammar that parses ABNF source itself. It is the Go counterpart of
// the TS `abnfRules` export (ts/src/converter.ts). Where the TS table
// holds bo/bc/a closures inline, the Go table refers to actions by
// `@`-name; the closures live in the internal Ref map installed by the
// package's ABNF parser (see getAbnfParser). Each call builds a fresh
// map, so callers may modify the result freely.
func AbnfRules() map[string]*tabnas.GrammarRuleSpec {
	// Element-starter alternatives shared by seq.open / seq.close.
	seqElemAlts := func() []*tabnas.GrammarAltSpec {
		return []*tabnas.GrammarAltSpec{
			{S: "#TX #DEF", B: 2, G: "end"},
			{S: "#TX #DEFA", B: 2, G: "end"},
			// A rulename followed by a repetition count — `a 1*b`, `a 2b`,
			// `simple-key 1*( dot-sep simple-key )`.
			//
			// This alternative exists for its S: pattern, not its action: it
			// widens the tcol the lexer uses for the token AFTER a rulename.
			// The `#TX #DEF` / `#TX #DEFA` lookaheads above are the only other
			// two-token patterns starting with #TX, so without this one that
			// tcol is just {#DEF, #DEFA} — the #NUM match.token matcher is
			// never offered the position, and the digits fall through to the
			// engine's default matchers. `1*b` then arrives as #NR and fails
			// to parse at all, and `2b` arrives as a #TX bareword, silently
			// misparsing as a reference to a rule named `2b`. Mirrors the TS
			// seq rule in ts/src/converter.ts.
			{S: "#TX #NUM", B: 2, P: "elem"},
			// Same problem, same fix, for a rulename followed by another
			// atom — `#ATOM` covers every atom-starter. TS needs this to
			// reach its #NV / #SS / #SI / #PV matchers at all; Go gets #NV
			// and #PV for free via match.TokenEager, but the alternative is
			// kept so the two tables stay readable side by side.
			{S: "#TX #ATOM", B: 2, P: "elem"},
			// `<all> = …` starts the NEXT production, so the current
			// sequence ends here rather than taking the prose as another
			// element. TS has always carried this in seq.open; Go's shared
			// open/close list had it in neither, so `top = NR` followed by
			// `<all> = <remove>` failed to parse here while it compiled on
			// the TS side. Must precede the `{S: "#PV", P: "elem"}`
			// alternative below.
			{S: "#PV #DEF", B: 2, G: "end"},
			{S: "#ALT", B: 1, G: "end"},
			{S: "#ZZ", B: 1, G: "end"},
			{S: "#RP", B: 1, G: "end"},
			{S: "#CB", B: 1, G: "end"},
			{S: "#ST", B: 1, P: "elem"},
			{S: "#NV", B: 1, P: "elem"},
			{S: "#SS", B: 1, P: "elem"},
			{S: "#SI", B: 1, P: "elem"},
			{S: "#PV", B: 1, P: "elem"},
			{S: "#TX", B: 1, P: "elem"},
			{S: "#LP", B: 1, P: "elem"},
			{S: "#OB", B: 1, P: "elem"},
			{S: "#STAR", B: 1, P: "elem"},
			{S: "#NUM", B: 1, P: "elem"},
		}
	}

	rules := map[string]*tabnas.GrammarRuleSpec{
		// abnf: accumulate productions.
		"abnf": {
			Open: []*tabnas.GrammarAltSpec{
				{S: "#ZZ", G: "empty"},
				{P: "prod"},
			},
			Close: []*tabnas.GrammarAltSpec{
				{S: "#ZZ"},
			},
		},

		// prod: one production per invocation; tail-recurses.
		"prod": {
			Open: []*tabnas.GrammarAltSpec{
				{S: "#TX #DEF", A: "@prod-name", P: "alts"},
				// `<all> = <remove>` — the whole-grammar reset. Prose lexes as
				// its own token (#PV), so it cannot be confused with a rule
				// name or with the `*` repetition operator. The name keeps its
				// angle brackets, which no #TX rulename can produce, so it can
				// never collide with a production actually called `all`.
				{S: "#PV #DEF", A: "@prod-name-prose", P: "alts"},
				{S: "#TX #DEFA", A: "@prod-name-inc", P: "alts"},
			},
			Close: []*tabnas.GrammarAltSpec{
				{S: "#TX #DEF", B: 2, R: "prod"},
				{S: "#TX #DEFA", B: 2, R: "prod"},
				{S: "#PV #DEF", B: 2, R: "prod"},
				{B: 1},
			},
		},

		// alts: alternative sequences separated by `/`.
		"alts": {
			Open: []*tabnas.GrammarAltSpec{
				{P: "seq"},
			},
			Close: []*tabnas.GrammarAltSpec{
				{S: "#ALT", P: "seq"},
				{B: 1},
			},
		},

		// seq: a (possibly empty) sequence of elements.
		"seq": {
			Open:  append(seqElemAlts(), &tabnas.GrammarAltSpec{P: "elem"}),
			Close: append(seqElemAlts(), &tabnas.GrammarAltSpec{B: 1}),
		},

		// elem: optional ABNF repetition prefix + atom.
		"elem": {
			Open: []*tabnas.GrammarAltSpec{
				{S: "#NUM #STAR #NUM #ATOM", B: 1, A: "@elem-rep-bounded", P: "atom"},
				{S: "#NUM #STAR #ATOM", B: 1, A: "@elem-rep-atleast", P: "atom"},
				{S: "#STAR #NUM #ATOM", B: 1, A: "@elem-rep-atmost", P: "atom"},
				{S: "#STAR #ATOM", B: 1, A: "@elem-rep-star", P: "atom"},
				{S: "#NUM #ATOM", B: 1, A: "@elem-rep-exact", P: "atom"},
				{P: "atom"},
			},
			Close: []*tabnas.GrammarAltSpec{
				{A: "@elem-close"},
			},
		},

		// atom: ref, terminal, group, or optional.
		"atom": {
			Open: []*tabnas.GrammarAltSpec{
				{S: "#SS #ST", A: "@atom-ss"},
				{S: "#SI #ST", A: "@atom-si"},
				{S: "#ST", A: "@atom-st"},
				{S: "#NV", A: "@atom-nv"},
				{S: "#PV", A: "@atom-pv"},
				{S: "#TX", A: "@atom-tx"},
				{S: "#LP", A: "@atom-lp", P: "alts"},
				{S: "#OB", A: "@atom-ob", P: "alts"},
			},
			Close: []*tabnas.GrammarAltSpec{
				{S: "#RP", C: "@atom-group-c", A: "@atom-group-close"},
				{S: "#CB", C: "@atom-opt-c", A: "@atom-opt-close"},
				{S: "#TX", B: 1},
				{S: "#ST", B: 1},
				{S: "#NV", B: 1},
				{S: "#SS", B: 1},
				{S: "#SI", B: 1},
				{S: "#PV", B: 1},
				{S: "#NUM", B: 1},
				{S: "#STAR", B: 1},
				{S: "#LP", B: 1},
				{S: "#OB", B: 1},
				{S: "#RP", B: 1},
				{S: "#CB", B: 1},
				{S: "#ALT", B: 1},
				{S: "#DEF", B: 1},
				{S: "#ZZ", B: 1},
				{B: 1},
			},
		},
	}
	return rules
}

// abnfParserOptions returns the engine Options for the ABNF parser.
func abnfParserOptions() tabnas.Options {
	f := false
	del := func(s string) *string { return nil }
	_ = del
	def := "="
	defa := "=/"
	alt := "/"
	star := "*"
	lp := "("
	rp := ")"
	ob := "["
	cb := "]"
	// fixed.token: remap and clear JSON defaults. nil pointer deletes.
	fixedTok := map[string]*string{
		"#OS":   nil,
		"#CS":   nil,
		"#CL":   nil,
		"#CA":   nil,
		"#OB":   &ob,
		"#CB":   &cb,
		"#DEF":  &def,
		"#DEFA": &defa,
		"#ALT":  &alt,
		"#STAR": &star,
		"#LP":   &lp,
		"#RP":   &rp,
	}
	matchTok := map[string]*regexp.Regexp{
		"#NUM": regexp.MustCompile(`^[0-9]+`),
		"#NV":  regexp.MustCompile(`^%[xdbXDB][0-9a-fA-F]+(?:[-.][0-9a-fA-F]+)*`),
		// RFC 5234 prose-val: `<` free text `>`. The body is every printable
		// char except `>` itself (%x20-3D / %x3F-7E).
		"#PV": regexp.MustCompile(`^<[\x20-\x3D\x3F-\x7E]*>`),
	}
	// `#NV` (numeric value, `%xNN…`) must lex even at non-leading
	// lookahead slots — e.g. the atom after a repetition prefix
	// (`1*%x30-39`), where it sits at position 2 inside the `#ATOM` set.
	// The Go engine gates non-eager match tokens by alt position 0 only
	// (the TS engine uses a per-position tcol), so mark `#NV` eager so it
	// fires regardless of slot. It starts with `%`, so it never
	// over-matches other tokens.
	// `#PV` is eager for the same reason as `#NV`: it must lex at
	// non-leading lookahead slots too. It starts with `<`, which opens no
	// other token, so it never over-matches.
	matchEager := map[string]bool{"#NV": true, "#PV": true}

	return tabnas.Options{
		Rule:  &tabnas.RuleOptions{Start: "abnf"},
		Fixed: &tabnas.FixedOptions{Token: fixedTok},
		Match: &tabnas.MatchOptions{Token: matchTok, TokenEager: matchEager},
		// RFC 5234 rulename is `ALPHA *(ALPHA / DIGIT / "-")` — nothing is
		// reserved, so `true`, `false` and `null` are ordinary rule names.
		// JSON's grammar uses all three (`value = false / null / true /
		// object / array / number / string`), and with the engine's default
		// keyword-value lexing they arrived as #VL value tokens instead of
		// #TX barewords, so no ABNF rendering of JSON would compile. The
		// ABNF meta-grammar has no use for #VL at all — this switch only
		// affects the parser that reads ABNF source, not the grammars it
		// emits, where `VL` remains a built-in token name. Mirrors the TS
		// converter.
		Value: &tabnas.ValueOptions{Lex: &f},
		// RFC 5234 char-val has NO escape sequences at all:
		//   char-val = DQUOTE *(%x20-21 / %x23-7E) DQUOTE
		// A backslash is just %x5C, an ordinary member of that range, so
		// `"\"` is a one-character literal — and a common one, since every
		// RFC defining `quoted-pair` writes it that way (RFC 5322, RFC 3261,
		// RFC 8259, …). With the engine's default JSON-style escaping the
		// backslash swallowed the closing quote and the grammar died with
		// `unterminated_string`, while `"a\b"` silently became `a<BS>b`.
		//
		// The engine offers no "escaping off" switch both runtimes share
		// (TS takes escapeChar: null, Go falls back to `\` on an empty
		// string), so instead point the escape character at DEL (%x7F) —
		// outside char-val's %x20-21 / %x23-7E body, hence unreachable in
		// any legal ABNF literal. Mirrors ts/src/converter.ts.
		String: &tabnas.StringOptions{EscapeChar: "\x7F"},
		TokenSet: map[string][]string{
			"ATOM": {"#ST", "#NV", "#TX", "#LP", "#OB", "#SS", "#SI", "#PV"},
		},
		Comment: &tabnas.CommentOptions{
			Def: map[string]*tabnas.CommentDef{
				"hash":  {Line: true, Start: ";", Lex: &boolTrue, EatLine: &f},
				"slash": nil,
				"multi": nil,
			},
		},
	}
}

var boolTrue = true

// _abnfParser is the lazily-built ABNF parser instance.
var (
	abnfParserOnce sync.Once
	abnfParserInst *tabnas.Tabnas
	abnfParserErr  error
)

// getAbnfParser returns a tabnas instance that parses ABNF source into a
// production list. Built once, lazily.
func getAbnfParser() (*tabnas.Tabnas, error) {
	abnfParserOnce.Do(func() {
		j := tabnas.Make(abnfParserOptions())

		// `%s` / `%i` prefixes require a following `"` (RFC 5234 string
		// prefixes). RE2 has no lookahead, so use function-form matchers
		// that consume only the 2-char prefix, leaving the `"` for #ST.
		// Resolve the tins now (after Make) and capture them.
		ssTin := j.Token("#SS")
		siTin := j.Token("#SI")
		prefixMatcher := func(letters string, name string, tin tabnas.Tin) tabnas.LexMatcher {
			return func(l *tabnas.Lex, _ *tabnas.Rule) *tabnas.Token {
				fwd := l.Fwd(3)
				if len(fwd) < 3 || fwd[0] != '%' || fwd[2] != '"' {
					return nil
				}
				c := fwd[1]
				found := false
				for i := 0; i < len(letters); i++ {
					if letters[i] == c {
						found = true
						break
					}
				}
				if !found {
					return nil
				}
				src := fwd[:2]
				tkn := l.Token(name, tin, src, src)
				pnt := l.Cursor()
				pnt.SI += 2
				pnt.CI += 2
				return tkn
			}
		}
		j.SetOptions(tabnas.Options{Match: &tabnas.MatchOptions{
			TokenFn: map[string]tabnas.LexMatcher{
				"#SS": prefixMatcher("sS", "#SS", ssTin),
				"#SI": prefixMatcher("iI", "#SI", siTin),
			},
		}})

		// Register the ATOM token set. Make() does not apply
		// Options.TokenSet (only SetOptions does), and the set must be
		// applied after #NV / #SS / #SI tins exist so the names resolve.
		j.SetOptions(tabnas.Options{
			TokenSet: map[string][]string{
				"ATOM": {"#ST", "#NV", "#TX", "#LP", "#OB", "#SS", "#SI", "#PV"},
			},
		})

		// Drop the default JSON rules — they would compete with ours for
		// the starting token set.
		for name := range j.RSM() {
			j.Rule(name, nil)
		}

		ref := abnfParseRef()
		if err := j.Grammar(&tabnas.GrammarSpec{
			Ref:  ref,
			Rule: AbnfRules(),
		}); err != nil {
			abnfParserErr = err
			return
		}
		abnfParserInst = j
	})
	return abnfParserInst, abnfParserErr
}

// parseAbnfRaw runs the ABNF parser grammar over src and returns the raw
// production list.
func parseAbnfRaw(src string) ([]*abnfProduction, error) {
	j, err := getAbnfParser()
	if err != nil {
		return nil, err
	}
	out, perr := j.Parse(src)
	if perr != nil {
		return nil, perr
	}
	if out == nil {
		return nil, nil
	}
	if p, ok := out.(*productionList); ok {
		return *p, nil
	}
	if prods, ok := out.([]*abnfProduction); ok {
		return prods, nil
	}
	return nil, nil
}

// --- small helpers ---

func asStr(v any) string { s, _ := v.(string); return s }
func asInt(v any) int {
	switch n := v.(type) {
	case int:
		return n
	case float64:
		return int(n)
	}
	return 0
}
func atoi(s string) int {
	n, err := strconv.Atoi(s)
	if err != nil {
		return 0
	}
	return n
}

var _ = fmt.Sprintf

// closeTok is the first token matched in a rule's CLOSE phase — the `)`
// of a group, the `]` of a bracketed optional. Returns nil when the
// rule closed without matching one, so a span falls back to its opener.
func closeTok(r *tabnas.Rule) *tabnas.Token {
	if 0 < r.CN {
		return r.C[0]
	}
	return nil
}
