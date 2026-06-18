package tabnasabnf

import (
	"encoding/json"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// jsonTextParser registers encoding/json as the engine's text parser so
// strict-mode jsonic output (which is valid JSON) round-trips through
// GrammarText. Registered once via TestMain. Mirrors the TS tests'
// JSON.parse + resolveFuncRefs round-trip.
func init() {
	tabnas.RegisterTextParser(func(src string) (any, error) {
		var v any
		if err := json.Unmarshal([]byte(src), &v); err != nil {
			return nil, err
		}
		return v, nil
	})
}

// loadJsonicSpec installs strict-jsonic grammar text on a fresh engine.
func loadJsonicSpec(t *testing.T, strictText string) *tabnas.Tabnas {
	t.Helper()
	rh := 4096
	j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
	if err := j.GrammarText(strictText); err != nil {
		t.Fatalf("GrammarText: %v\n---\n%s", err, strictText)
	}
	return j
}

var recognitionCases = []struct {
	name   string
	src    string
	accept []string
	reject []string
}{
	{"greet", `greet = "hi" / "hello"`, []string{"hi", "hello"}, []string{"nope", "h"}},
	{"pair", `pair = "a" "b"`, []string{"ab"}, []string{"a", "ba"}},
	{"arith",
		"expr = term *(\"+\" term)\nterm = \"(\" expr \")\" / number\nnumber = 1*DIGIT",
		[]string{"1", "1+2", "(1+2)+3"}, []string{"1+", "(1"}},
}

// recognisesJsonic installs the (strict-JSON) recognition text and checks
// parse acceptance.
func recognisesJsonic(t *testing.T, jsonicText, input string) bool {
	t.Helper()
	j := loadJsonicSpec(t, jsonicText)
	_, err := j.Parse(input)
	return err == nil
}

func TestCompileRecognitionStrict(t *testing.T) {
	for _, tc := range recognitionCases {
		t.Run(tc.name, func(t *testing.T) {
			text, err := AbnfCompile(tc.src, &AbnfCompileOptions{Strict: true})
			if err != nil {
				t.Fatalf("AbnfCompile: %v", err)
			}
			// No function references / live functions in the text.
			if strings.Contains(text, "@abnf_a") {
				t.Errorf("recognition spec leaked a closure ref:\n%s", text)
			}
			if strings.Contains(text, "@node$") || strings.Contains(text, "@capture$") ||
				strings.Contains(text, "@bubble$") {
				t.Errorf("recognition spec retained a tree builtin:\n%s", text)
			}
			for _, ok := range tc.accept {
				if !recognisesJsonic(t, text, ok) {
					t.Errorf("%s: should accept %q", tc.name, ok)
				}
			}
			for _, bad := range tc.reject {
				if recognisesJsonic(t, text, bad) {
					t.Errorf("%s: should reject %q", tc.name, bad)
				}
			}
		})
	}
}

func TestCompileRelaxedFormat(t *testing.T) {
	text, err := AbnfCompile(recognitionCases[0].src, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(text, "open:") {
		t.Errorf("expected bare identifier key 'open:'\n%s", text)
	}
	if !strings.Contains(text, "'#HI'") && !strings.Contains(text, "'#HELLO'") {
		t.Errorf("expected single-quoted token strings\n%s", text)
	}
	if strings.Contains(text, `"open"`) {
		t.Errorf("keys should not be double-quoted in relaxed mode\n%s", text)
	}
}

func TestCompileEagerRegexSerialisation(t *testing.T) {
	// "hi" is case-insensitive -> an eager regex match token, emitted as
	// @~/^hi/i.
	text, err := AbnfCompile(`greet = "hi"`, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(text, `'@~/^hi/i'`) {
		t.Errorf("expected eager regex '@~/^hi/i' in:\n%s", text)
	}
}

func TestCompileFullKeepsTreeBuiltins(t *testing.T) {
	rec := false
	full, err := AbnfCompile(`pair = "a" "b"`, &AbnfCompileOptions{Recognition: &rec})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(full, "@node$") {
		t.Errorf("full mode should retain @node$:\n%s", full)
	}
	recT := true
	recog, err := AbnfCompile(`pair = "a" "b"`, &AbnfCompileOptions{Recognition: &recT})
	if err != nil {
		t.Fatal(err)
	}
	for _, b := range []string{"@node$", "@capture$", "@bubble$", "node$", "capture$"} {
		if strings.Contains(recog, b) {
			t.Errorf("recognition mode should drop %q:\n%s", b, recog)
		}
	}
}

func TestCompileFullModeParity(t *testing.T) {
	// The full pure-data AST grammar (round-tripped through jsonic text)
	// builds the SAME {rule,src,kids} tree as the live closure grammar.
	cases := []struct{ name, src, input string }{
		{"greet", `greet = "hi" / "hello"`, "hello"},
		{"pair", `pair = "a" "b"`, "ab"},
		{"arith",
			"expr = term *(\"+\" term)\nterm = \"(\" expr \")\" / number\nnumber = 1*DIGIT",
			"(1+2)+3"},
		{"probe", "R = [ A \"@\" ] A\nA = 1*ALPHA", "a@b"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			// Live tree (closure mode).
			live := makeParser(t, tc.src, nil)
			liveTree, err := live.Parse(tc.input)
			if err != nil {
				t.Fatalf("live parse: %v", err)
			}
			// Pure-data tree from the compiled full grammar (strict JSON
			// so it round-trips via encoding/json).
			rec := false
			text, err := AbnfCompile(tc.src, &AbnfCompileOptions{Recognition: &rec, Strict: true})
			if err != nil {
				t.Fatalf("AbnfCompile: %v", err)
			}
			j := loadJsonicSpec(t, text)
			pureTree, err := j.Parse(tc.input)
			if err != nil {
				t.Fatalf("pure parse: %v", err)
			}
			if !treeEqual(liveTree, pureTree) {
				t.Errorf("%s: pure-data tree != live tree\n  live: %#v\n  pure: %#v",
					tc.name, liveTree, pureTree)
			}
		})
	}
}

func TestCompileBuiltinsNoClosures(t *testing.T) {
	for _, tc := range []string{
		`greet = "hi" / "hello"`,
		`pair = "a" "b"`,
		"R = [ A \"@\" ] A\nA = 1*ALPHA",
	} {
		spec, err := Abnf(tc, &AbnfConvertOptions{Builtins: true})
		if err != nil {
			t.Fatal(err)
		}
		if len(spec.Ref) != 0 {
			keys := make([]string, 0, len(spec.Ref))
			for k := range spec.Ref {
				keys = append(keys, k)
			}
			t.Errorf("builtins conversion left closures: %v", keys)
		}
	}
}

func TestToPureRejectsClosureSpec(t *testing.T) {
	spec, err := Abnf(`greet = "hi"`, nil) // closure mode (no builtins)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := toPureData(spec); err == nil {
		t.Errorf("toPureData should reject a closure spec")
	} else if _, ok := err.(*AbnfCompileError); !ok {
		t.Errorf("expected *AbnfCompileError, got %T", err)
	}
}

// treeEqual compares two {rule,src,kids} trees structurally, ignoring
// map key ordering. Both come from the engine, so they're map[string]any.
func treeEqual(a, b any) bool {
	am, aok := a.(map[string]any)
	bm, bok := b.(map[string]any)
	if aok != bok {
		return false
	}
	if !aok {
		return a == b
	}
	if am["rule"] != bm["rule"] || am["src"] != bm["src"] {
		return false
	}
	ak := asKids(am["kids"])
	bk := asKids(bm["kids"])
	if len(ak) != len(bk) {
		return false
	}
	for i := range ak {
		if !treeEqual(ak[i], bk[i]) {
			return false
		}
	}
	return true
}

func asKids(v any) []any {
	if s, ok := v.([]any); ok {
		return s
	}
	return nil
}
