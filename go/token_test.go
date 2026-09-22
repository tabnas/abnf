package tabnasabnf

import (
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// The built-in token terminals. The Go twin of ts/test/token.test.js,
// which had none until tabnas/abnf#73: `wordKeywords` and `#TX` appeared
// nowhere in this package's tests, so either port could have drifted on
// them with all three suites green.
//
// What a shared fixture reaches is pinned there instead, and is NOT
// repeated here: test/spec/alignment-abnf-ast.tsv carries every parse
// this file used to assert (a bareword through TX, a number through NR, a
// quoted string through ST, two adjacent token terminals, a repetition
// over one, the nullable-optional dispatch and a user rule of the same
// name), and test/spec/alignment-abnf-rules.tsv carries the rule sets.
// What is left is the emitted terminal itself, which no fixture column
// reads, and `wordKeywords`, which is an option no fixture passes.

// openTokens is the `s` of each opening alternate of one rule.
func openTokens(t *testing.T, spec *tabnas.GrammarSpec, rule string) []string {
	t.Helper()
	r, ok := spec.Rule[rule]
	if !ok || nil == r {
		t.Fatalf("no rule %q in the emitted spec", rule)
	}
	alts, ok := r.Open.([]*tabnas.GrammarAltSpec)
	if !ok {
		t.Fatalf("rule %q: open is %T, want []*tabnas.GrammarAltSpec", rule, r.Open)
	}
	out := make([]string, 0, len(alts))
	for _, alt := range alts {
		s, _ := alt.S.(string)
		out = append(out, s)
	}
	return out
}

func TestBareTokenRefCompilesToATerminal(t *testing.T) {
	spec, err := Abnf("ident = TX", &AbnfConvertOptions{Tag: "tk"})
	if nil != err {
		t.Fatalf("convert: %v", err)
	}
	got := openTokens(t, spec, "ident")
	if 1 != len(got) || "#TX" != got[0] {
		t.Errorf("rule ident opens on %v, want [#TX]", got)
	}
}

func TestEachBuiltinTokenTerminalMapsToItsLexerToken(t *testing.T) {
	spec, err := Abnf("w = TX\nn = NR\ns = ST\nv = VL", &AbnfConvertOptions{Tag: "tk"})
	if nil != err {
		t.Fatalf("convert: %v", err)
	}
	for rule, want := range map[string]string{
		"w": "#TX", "n": "#NR", "s": "#ST", "v": "#VL",
	} {
		got := openTokens(t, spec, rule)
		if 1 != len(got) || want != got[0] {
			t.Errorf("rule %s opens on %v, want [%s]", rule, got, want)
		}
	}
}

// A user rule of the same name wins over the built-in, so the reference
// stays a rule push rather than becoming a token terminal. The parse is
// pinned by the shared AST fixture; what is asserted here is that the
// emitted spec carries the rule at all.
func TestAUserRuleOfTheSameNameWinsOverTheBuiltin(t *testing.T) {
	spec, err := Abnf("top = TX\nTX = \"literal\"", &AbnfConvertOptions{Tag: "tk"})
	if nil != err {
		t.Fatalf("convert: %v", err)
	}
	if r, ok := spec.Rule["TX"]; !ok || nil == r {
		t.Fatalf("the user's TX rule is not in the emitted spec")
	}
	for _, s := range openTokens(t, spec, "top") {
		if "#TX" == s {
			t.Errorf("rule top opens on the built-in #TX terminal, "+
				"but the grammar defines TX itself: %v", openTokens(t, spec, "top"))
		}
	}
}

// wordKeywords: a keyword does not grab the prefix of an identifier.
//
// `map` is a prefix of the identifier `mapping`. Off, the literal matches
// that prefix and `name` takes the rest; on, the literal only matches a
// whole word, so the grammar refuses `mapping ;` and still accepts a real
// `map foo ;`.
func TestWordKeywordsDoesNotGrabAnIdentifierPrefix(t *testing.T) {
	const src = "decl = \"map\" name \";\"\nname = TX"

	engine := func(wordKeywords bool) *tabnas.Tabnas {
		t.Helper()
		spec, err := Abnf(src, &AbnfConvertOptions{
			Tag: "tk", WordKeywords: wordKeywords})
		if nil != err {
			t.Fatalf("convert (wordKeywords=%v): %v", wordKeywords, err)
		}
		rh := 4096
		j := tabnas.Make(tabnas.Options{
			Rewind: &tabnas.RewindOptions{History: &rh}})
		if err := j.Grammar(spec); nil != err {
			t.Fatalf("install (wordKeywords=%v): %v", wordKeywords, err)
		}
		return j
	}

	if _, err := engine(false).Parse("mapping ;"); nil != err {
		t.Errorf("off: `mapping ;` should parse, the literal taking the "+
			"prefix and name taking `ping`, got %v", err)
	}
	if _, err := engine(true).Parse("mapping ;"); nil == err {
		t.Errorf("on: `mapping ;` should be refused, `map` is not a whole word there")
	}
	out, err := engine(true).Parse("map foo ;")
	if nil != err {
		t.Fatalf("on: `map foo ;` should parse, got %v", err)
	}
	if got := srcField(out); "mapfoo;" != got {
		t.Errorf("on: src is %q, want %q", got, "mapfoo;")
	}
}
