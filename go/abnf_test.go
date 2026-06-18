package abnf

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// fixturesDir is the shared .bnf/.abnf fixture directory used by both
// the TS and Go test suites, keeping them in lockstep.
func fixturesDir() string { return filepath.Join("..", "ts", "test", "grammar") }

func loadFixture(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(fixturesDir(), name))
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}
	return string(b)
}

// makeParser converts src and installs it on a fresh engine, returning
// the engine. A generous rewind history covers probe dispatch.
func makeParser(t *testing.T, src string, opts *BnfConvertOptions) *tabnas.Tabnas {
	t.Helper()
	spec, err := Bnf(src, opts)
	if err != nil {
		t.Fatalf("Bnf(%q): %v", src, err)
	}
	rh := 4096
	j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
	if err := j.Grammar(spec); err != nil {
		t.Fatalf("Grammar: %v", err)
	}
	return j
}

// node builds the AST map shape the tree builtins produce.
func node(rule, src string, kids ...any) map[string]any {
	k := []any{}
	k = append(k, kids...)
	return map[string]any{"rule": rule, "src": src, "kids": k}
}

func assertParse(t *testing.T, j *tabnas.Tabnas, input string, want any) {
	t.Helper()
	got, err := j.Parse(input)
	if err != nil {
		t.Fatalf("parse %q: %v", input, err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("parse %q:\n  got  %#v\n  want %#v", input, got, want)
	}
}

func assertAccept(t *testing.T, j *tabnas.Tabnas, input string) {
	t.Helper()
	if _, err := j.Parse(input); err != nil {
		t.Errorf("expected accept %q, got error: %v", input, firstLineOf(err))
	}
}

func assertReject(t *testing.T, j *tabnas.Tabnas, input string) {
	t.Helper()
	if _, err := j.Parse(input); err == nil {
		t.Errorf("expected reject %q, but it parsed", input)
	}
}

func firstLineOf(e error) string {
	s := e.Error()
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			return s[:i]
		}
	}
	return s
}

// ---- AST output contract (mirrors bnf.test.js) ---------------------

func TestAstAlternationOfTerminals(t *testing.T) {
	j := makeParser(t, `g = "hi" / "hello"`, nil)
	assertParse(t, j, "hi", node("g", "hi"))
	assertParse(t, j, "hello", node("g", "hello"))
}

func TestAstSingleTerminal(t *testing.T) {
	j := makeParser(t, `g = "x"`, nil)
	assertParse(t, j, "x", node("g", "x"))
}

func TestAstRefAsChildNode(t *testing.T) {
	// `p = "a" q` keeps q as a kid (q is not at the leading position).
	j := makeParser(t, "p = \"a\" q\nq = \"b\"", nil)
	assertParse(t, j, "a b", node("p", "ab", node("q", "b")))
}

func TestAstCompositeRule(t *testing.T) {
	j := makeParser(t, "x = \"x\" name \"=\" value\nname = 1*ALPHA\nvalue = 1*DIGIT", nil)
	assertParse(t, j, "xfoo=42", node("x", "xfoo=42",
		node("name", "foo"), node("value", "42")))
}

func TestAstStarWithLeadingTerminal(t *testing.T) {
	j := makeParser(t, "list = \"[\" item *(\",\" item) \"]\"\nitem = 1*ALPHA", nil)
	assertParse(t, j, "[a,b,c]", node("list", "[a,b,c]",
		node("item", "a"), node("item", "b"), node("item", "c")))
}

// ---- EBNF desugaring (accept/reject) -------------------------------

func TestEbnfOptional(t *testing.T) {
	j := makeParser(t, "g = \"hi\" [ \"there\" ]", nil)
	assertAccept(t, j, "hi")
	assertAccept(t, j, "hi there")
	assertReject(t, j, "hi nope")
}

func TestEbnfStar(t *testing.T) {
	j := makeParser(t, "g = *\"x\" \"end\"", nil)
	assertAccept(t, j, "end")
	assertAccept(t, j, "x end")
	assertAccept(t, j, "x x x end")
	assertReject(t, j, "y end")
}

func TestEbnfPlus(t *testing.T) {
	j := makeParser(t, "g = 1*\"x\" \"end\"", nil)
	assertAccept(t, j, "x end")
	assertAccept(t, j, "x x x end")
	assertReject(t, j, "end")
}

func TestEbnfBoundedRep(t *testing.T) {
	j := makeParser(t, "g = 2*4\"x\" \"end\"", nil)
	assertReject(t, j, "end")
	assertReject(t, j, "x end")
	assertAccept(t, j, "x x end")
	assertAccept(t, j, "x x x end")
	assertAccept(t, j, "x x x x end")
	assertReject(t, j, "x x x x x end")
}

func TestEbnfExactRep(t *testing.T) {
	j := makeParser(t, "g = 3\"x\" \"end\"", nil)
	assertReject(t, j, "x x end")
	assertAccept(t, j, "x x x end")
	assertReject(t, j, "x x x x end")
}

func TestEbnfUpperBoundedRep(t *testing.T) {
	j := makeParser(t, "g = *2\"x\" \"end\"", nil)
	assertAccept(t, j, "end")
	assertAccept(t, j, "x end")
	assertAccept(t, j, "x x end")
	assertReject(t, j, "x x x end")
}

// ---- ABNF numeric values -------------------------------------------

func TestNumericHexSingleChar(t *testing.T) {
	j := makeParser(t, `g = %x61`, nil) // 'a'
	assertAccept(t, j, "a")
	assertReject(t, j, "b")
}

func TestNumericConcatenated(t *testing.T) {
	j := makeParser(t, `g = %x66.6f.6f`, nil) // "foo"
	assertAccept(t, j, "foo")
	assertReject(t, j, "bar")
}

func TestNumericRange(t *testing.T) {
	j := makeParser(t, `g = %x30-39`, nil) // [0-9]
	assertAccept(t, j, "0")
	assertAccept(t, j, "5")
	assertAccept(t, j, "9")
	assertReject(t, j, "a")
}

func TestNumericRangeWithRepetition(t *testing.T) {
	j := makeParser(t, `g = 1*%x30-39`, nil)
	assertAccept(t, j, "1")
	assertAccept(t, j, "12345")
	assertReject(t, j, "abc")
}

// ---- case sensitivity ----------------------------------------------

func TestCaseInsensitiveDefault(t *testing.T) {
	j := makeParser(t, `g = "GET"`, nil)
	assertAccept(t, j, "GET")
	assertAccept(t, j, "get")
}

func TestCaseSensitiveExplicit(t *testing.T) {
	j := makeParser(t, `g = %s"GET"`, nil)
	assertAccept(t, j, "GET")
	assertReject(t, j, "get")
}

// ---- start rule + multi-production ---------------------------------

func TestStartOverride(t *testing.T) {
	j := makeParser(t, "a = \"x\"\nb = \"y\"", &BnfConvertOptions{Start: "b"})
	assertParse(t, j, "y", node("b", "y"))
}

// ---- error cases ---------------------------------------------------

func TestRejectUnknownRule(t *testing.T) {
	_, err := Bnf(`g = missing`, nil)
	if err == nil {
		t.Fatalf("expected error for unknown rule reference")
	}
}

func TestRejectNoProductions(t *testing.T) {
	_, err := Bnf("; just a comment\n", nil)
	if err == nil {
		t.Fatalf("expected error for no productions")
	}
	if _, ok := err.(*BnfParseError); !ok {
		t.Errorf("expected *BnfParseError, got %T", err)
	}
}

// ---- fixtures: parse representative inputs --------------------------

func TestFixtureGreet(t *testing.T) {
	j := makeParser(t, loadFixture(t, "greet.bnf"), nil)
	assertParse(t, j, "hi", node("greet", "hi"))
	assertParse(t, j, "hello", node("greet", "hello"))
}

func TestFixturePair(t *testing.T) {
	j := makeParser(t, loadFixture(t, "pair.bnf"), nil)
	assertParse(t, j, "ab", node("pair", "ab"))
}

func TestFixtureArith(t *testing.T) {
	j := makeParser(t, loadFixture(t, "arith.bnf"), nil)
	for _, in := range []string{"1", "1+2", "1+2*3", "(1+2)*3", "1+2-3"} {
		assertAccept(t, j, in)
	}
	assertReject(t, j, "1+")
	assertReject(t, j, "(1")
}

func TestFixtureArithLeftrec(t *testing.T) {
	j := makeParser(t, loadFixture(t, "arith-leftrec.bnf"), nil)
	for _, in := range []string{"1", "1+2", "1+2*3", "(1+2)*3", "1+2-3"} {
		assertAccept(t, j, in)
	}
	assertReject(t, j, "1+")
}

func TestFixtureArithLeftrecEquivalence(t *testing.T) {
	// The leftrec form recognises the same language as the stratified
	// form: src text is identical for accepted inputs.
	a := makeParser(t, loadFixture(t, "arith.bnf"), nil)
	b := makeParser(t, loadFixture(t, "arith-leftrec.bnf"), nil)
	for _, in := range []string{"1", "1+2", "1+2*3", "(1+2)*3"} {
		ra, ea := a.Parse(in)
		rb, eb := b.Parse(in)
		if ea != nil || eb != nil {
			t.Errorf("parse %q: arith err=%v leftrec err=%v", in, ea, eb)
			continue
		}
		if srcField(ra) != in || srcField(rb) != in {
			t.Errorf("parse %q: arith src=%q leftrec src=%q", in, srcField(ra), srcField(rb))
		}
	}
}

func TestFixtureJsonSubset(t *testing.T) {
	j := makeParser(t, loadFixture(t, "json-subset.bnf"), nil)
	assertParse(t, j, "1", node("value", "1"))
	assertParse(t, j, "a", node("value", "a"))
	for _, in := range []string{"{a:1}", "[1,2,3]", "{a:{b:2}}", "[a,b]"} {
		assertAccept(t, j, in)
	}
}

func srcField(v any) string {
	if m, ok := v.(map[string]any); ok {
		if s, ok := m["src"].(string); ok {
			return s
		}
	}
	return ""
}
