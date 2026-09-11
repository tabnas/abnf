package tabnasabnf

import (
	"sort"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// Overlapping character classes. Mirrors ts/test/class-overlap.test.js.
//
// The lexer produces ONE token per position and picks it by running the
// matchers the rule expects in allocation order, first match wins. So
// when two class tokens both cover a character, whichever was allocated
// first always won, and every alternative keyed on the other one was
// unreachable. Which alternative died depended only on the order the
// classes happened to be allocated in — which depends on the order the
// productions are visited, so the SAME language written two ways gave
// two different parsers.
//
// The compiler now lays overlapping classes over a shared partition of
// disjoint atoms and expresses each class as a token set over them, so
// there is nothing left for allocation order to decide.

const digitFirst = "top = c\nc = DIGIT / %x31-39 DIGIT\n"
const rangeFirst = "top = c\nc = %x31-39 DIGIT / DIGIT\n"

func classAccepts(t *testing.T, grammar, input string) bool {
	t.Helper()
	spec, err := Abnf(grammar, nil)
	if err != nil {
		return false
	}
	rh := 4096
	j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
	if err := j.Grammar(spec); err != nil {
		return false
	}
	_, err = j.Parse(input)
	return err == nil
}

func TestClassOverlapBothWidths(t *testing.T) {
	for label, grammar := range map[string]string{
		"DIGIT first":   digitFirst,
		"%x31-39 first": rangeFirst,
	} {
		if !classAccepts(t, grammar, "8") {
			t.Errorf("%s: rejected one digit", label)
		}
		if !classAccepts(t, grammar, "18") {
			t.Errorf("%s: rejected two digits", label)
		}
		if classAccepts(t, grammar, "08") {
			t.Errorf("%s: accepted 08, but the two-digit alt needs 1-9", label)
		}
		if classAccepts(t, grammar, "x") {
			t.Errorf("%s: accepted a non-digit", label)
		}
	}
}

func TestClassOverlapOrderIndependent(t *testing.T) {
	for _, input := range []string{"0", "5", "9", "10", "42", "99", "08", "x", ""} {
		a := classAccepts(t, digitFirst, input)
		b := classAccepts(t, rangeFirst, input)
		if a != b {
			t.Errorf("alternative order changed the verdict for %q: %v vs %v",
				input, a, b)
		}
	}
}

// Verbatim from RFC 3986 Appendix A. Standing alone — outside the
// `"." dec-octet` context that gave its alternatives distinguishing
// two-token prefixes — every multi-digit octet used to be rejected.
func TestClassOverlapDecOctet(t *testing.T) {
	const g = "top = dec-octet\n" +
		"dec-octet = DIGIT\n" +
		"          / %x31-39 DIGIT\n" +
		`          / "1" 2DIGIT` + "\n" +
		`          / "2" %x30-34 DIGIT` + "\n" +
		`          / "25" %x30-35` + "\n"
	for _, v := range []string{"0", "9", "10", "42", "99"} {
		if !classAccepts(t, g, v) {
			t.Errorf("dec-octet should accept %q", v)
		}
	}
	for _, v := range []string{"a", "1a"} {
		if classAccepts(t, g, v) {
			t.Errorf("dec-octet should reject %q", v)
		}
	}
}

// %x30-39 and %x31-39 overlap, so the atoms are [0-0] and [1-9]. Only
// %x30-39 spans more than one of them and so becomes a set; %x31-39 IS
// an atom and points straight at that token, and ALPHA overlaps nothing
// and keeps the tokens it has always had.
func TestClassOverlapEmitsSets(t *testing.T) {
	spec, err := Abnf("top = c\nc = DIGIT / %x31-39 DIGIT / ALPHA\n", nil)
	if err != nil {
		t.Fatal(err)
	}
	if spec.Options == nil || spec.Options.TokenSet == nil {
		t.Fatal("no token sets emitted")
	}
	var names []string
	for n := range spec.Options.TokenSet {
		names = append(names, n)
		// Keyed WITHOUT the leading `#`: that is the only form both
		// engines resolve (this one trims it outright, TS falls back to
		// the stripped name).
		if n[0] == '#' {
			t.Errorf("set key %q must not carry a '#'", n)
		}
	}
	sort.Strings(names)
	if len(names) != 1 {
		t.Fatalf("expected one set (a class spanning several atoms), got %v", names)
	}
}

// `#HELLO` and `#HI` share an `h` but are distinct TOKENS, so the
// dispatch was never in doubt and needs no lookahead help. Asking the
// character question here doubled this rule's alternates.
func TestClassOverlapLeavesDistinctHeadsAlone(t *testing.T) {
	spec, err := Abnf("greeting = \"hello\" name / \"hi\" name\nname = TX\n", nil)
	if err != nil {
		t.Fatal(err)
	}
	rs := spec.Rule["greeting"]
	if rs == nil {
		t.Fatal("no greeting rule")
	}
	// GrammarRuleSpec.Open is `any` — the emitter always fills it with a
	// plain alt slice, so assert that rather than reaching for a length
	// the type does not promise.
	open, ok := rs.Open.([]*tabnas.GrammarAltSpec)
	if !ok {
		t.Fatalf("greeting open is %T, want []*tabnas.GrammarAltSpec", rs.Open)
	}
	if len(open) != 2 {
		t.Errorf("greeting has %d open alts, want 2", len(open))
	}
}
