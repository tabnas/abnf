package tabnasabnf

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// Value annotations carried in ABNF comments. Mirrors
// ts/test/value-annotation.test.js.
//
//	ver = maj "." min "." pat    ; @object maj min pat
//
// RFC 5234 has nowhere else to put this. A comment is the only place in
// the notation that carries no meaning of its own, which is exactly why it
// can carry one here without changing what the grammar accepts — strip
// every annotation and the same language parses, just into a tree instead
// of a value. These tests assert that both halves hold.

const verSrc = "ver = maj \".\" min \".\" pat   ; @object maj min pat\n" +
	"maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n"

func annotBuild(t *testing.T, src, input, start string) any {
	t.Helper()
	spec, err := Abnf(src, &AbnfConvertOptions{Start: start})
	if err != nil {
		t.Fatalf("convert: %v", err)
	}
	j := tabnas.Make()
	if err := j.Grammar(spec); err != nil {
		t.Fatalf("install: %v", err)
	}
	out, err := j.Parse(input)
	if err != nil {
		t.Fatalf("parse %q: %v", input, err)
	}
	return tabnas.UnwrapUndefined(out)
}

func annotAccepts(src, input, start string) bool {
	spec, err := Abnf(src, &AbnfConvertOptions{Start: start})
	if err != nil {
		return false
	}
	j := tabnas.Make()
	if err := j.Grammar(spec); err != nil {
		return false
	}
	_, err = j.Parse(input)
	return err == nil
}

func annotValueOf(t *testing.T, src, name string) *ValueAnnotation {
	t.Helper()
	g, err := ParseAbnf(src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	p := findProd(g, name)
	if p == nil {
		return nil
	}
	return p.Value
}

func jsonEq(got, want any) bool {
	norm := func(v any) any {
		b, err := json.Marshal(v)
		if err != nil {
			return nil
		}
		var out any
		_ = json.Unmarshal(b, &out)
		return out
	}
	return reflect.DeepEqual(norm(got), norm(want))
}

func TestAnnotationBuildsObject(t *testing.T) {
	got := annotBuild(t, verSrc, "1.2.30", "ver")
	want := map[string]any{"maj": "1", "min": "2", "pat": "30"}
	if !jsonEq(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}

func TestAnnotationReadsIntoTheIR(t *testing.T) {
	got := annotValueOf(t, verSrc, "ver")
	want := &ValueAnnotation{Kind: "object", Members: []string{"maj", "min", "pat"}}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
	if v := annotValueOf(t, verSrc, "maj"); v != nil {
		t.Errorf("only the annotated rule carries one, maj got %#v", v)
	}
}

// The point of putting this in a comment: the same source minus the
// annotation must still parse the same inputs — it just produces the tree
// it always did.
func TestAnnotationChangesWhatIsBuiltNotWhatIsAccepted(t *testing.T) {
	plain := "ver = maj \".\" min \".\" pat\n" +
		"maj = 1*DIGIT\nmin = 1*DIGIT\npat = 1*DIGIT\n"
	for _, input := range []string{"1.2.30", "11.22.33", "1.2", "x", ""} {
		if a, b := annotAccepts(verSrc, input, "ver"), annotAccepts(plain, input, "ver"); a != b {
			t.Errorf("the annotation changed whether %q parses: %v vs %v", input, a, b)
		}
	}
	tree, ok := annotBuild(t, plain, "1.2.30", "ver").(map[string]any)
	if !ok || tree["rule"] != "ver" || tree["src"] != "1.2.30" {
		t.Errorf("without the annotation the old tree should come back, got %#v", tree)
	}
}

func TestAnnotationNestsAnAnnotatedMember(t *testing.T) {
	src := "top = name \"=\" inner    ; @object name inner\n" +
		"name = 1*ALPHA\n" +
		"inner = maj \".\" min    ; @object maj min\n" +
		"maj = 1*DIGIT\nmin = 1*DIGIT\n"
	got := annotBuild(t, src, "ab=1.2", "top")
	want := map[string]any{
		"name":  "ab",
		"inner": map[string]any{"maj": "1", "min": "2"},
	}
	if !jsonEq(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}

func TestAnnotationBuildsArray(t *testing.T) {
	src := "top = a \",\" b   ; @array\na = 1*DIGIT\nb = 1*DIGIT\n"
	if got := annotBuild(t, src, "1,2", "top"); !jsonEq(got, []any{"1", "2"}) {
		t.Errorf("got %#v, want [1 2] as strings", got)
	}
}

// A rule written across several lines, with the annotation on the last of
// them, means the same thing.
func TestAnnotationFollowsTheRuleNotTheLine(t *testing.T) {
	src := "ver = maj \".\" min\n" +
		"                        ; @object maj min\n" +
		"maj = 1*DIGIT\nmin = 1*DIGIT\n"
	want := &ValueAnnotation{Kind: "object", Members: []string{"maj", "min"}}
	if got := annotValueOf(t, src, "ver"); !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}

// `";@object x"` is a LITERAL semicolon, not a comment — treating it as one
// would attach an annotation the author never wrote.
func TestAnnotationIgnoresASemicolonInsideAString(t *testing.T) {
	src := "top = sep 1*DIGIT\nsep = \";@object x\"\n"
	if v := annotValueOf(t, src, "top"); v != nil {
		t.Errorf("top got %#v, want none", v)
	}
	if v := annotValueOf(t, src, "sep"); v != nil {
		t.Errorf("sep got %#v, want none", v)
	}
	if !annotAccepts(src, ";@object x1", "top") {
		t.Error("and it should still parse")
	}
}

// The notation has no directive namespace, so this must not assume one: a
// reader's own `; @deprecated` has to keep meaning nothing.
func TestAnnotationLeavesOtherDirectivesAlone(t *testing.T) {
	src := "top = 1*DIGIT   ; @deprecated use ver instead\n"
	if v := annotValueOf(t, src, "top"); v != nil {
		t.Errorf("got %#v, want none", v)
	}
}

func TestAnnotationRefusals(t *testing.T) {
	cases := map[string]struct{ src, want string }{
		"before any rule": {
			"; @object a b\ntop = 1*DIGIT\n", "before any rule"},
		"two on one rule": {
			"top = a \".\" b   ; @object a b\n                ; @object a b\n" +
				"a = 1*DIGIT\nb = 1*DIGIT\n", "more than one value annotation"},
		"@array naming members": {
			"top = a \",\" b   ; @array a b\na = 1*DIGIT\nb = 1*DIGIT\n",
			"'@array' names no members"},
		"a member that is not a rule name": {
			"top = a \".\" b   ; @object a 9nope\na = 1*DIGIT\nb = 1*DIGIT\n",
			"is not a rule name"},
	}
	for label, c := range cases {
		_, err := ParseAbnf(c.src)
		if err == nil {
			t.Errorf("%s: expected a refusal, got none", label)
			continue
		}
		if !strings.Contains(err.Error(), c.want) {
			t.Errorf("%s: got %q, want it to mention %q", label, err.Error(), c.want)
		}
	}
}

// The refusals above are the front-end's own — they are about the
// COMMENT, and ParseAbnf raises them. These come from the compiler
// underneath, and are here because they are what an ABNF author actually
// hits: the comment is well-formed, the grammar is not. They must reach
// the author in ABNF's own words, never as "bnf:". Mirrors
// ts/test/value-annotation.test.js.
func TestAnnotationCompilerRefusals(t *testing.T) {
	cases := map[string]struct{ src, want string }{
		// A rule's first reference is folded into it, which erases that
		// rule's builders — the member would hold an internal node.
		"a leading member that builds a value": {
			"top = inner \",\" x   ; @object inner x\n" +
				"inner = a \".\" b     ; @object a b\n" +
				"a = 1*DIGIT\nb = 1*DIGIT\nx = 1*DIGIT\n",
			"erases the value 'inner' is annotated to build"},
		// The erasure needs a LEADING reference, not an annotated caller.
		// `top` names nothing, so nothing looked at it, and the grammar
		// compiled to an ordinary AST with `leaf`'s value nowhere in it.
		"an unannotated rule that inlines an annotated one": {
			"top = leaf \",\"\nleaf = d   ; @object d\nd = 1*DIGIT\n",
			"erases the value 'leaf' is annotated to build"},
		// A group produces a value, so it is a member and must be named —
		// but a member name has to be a rule name, and a group has none.
		// Naming only `c` used to key the GROUP as `c` and then overwrite
		// it, which is why refusing is the point.
		"a group that cannot be named": {
			"top = ( a / b ) c   ; @object c\n" +
				"a = 1*DIGIT\nb = 1*ALPHA\nc = 1*DIGIT\n",
			"names 1 member but has 2 parts that produce a value"},
	}
	for label, c := range cases {
		_, err := Abnf(c.src, nil)
		if err == nil {
			t.Errorf("%s: expected a refusal, got none", label)
			continue
		}
		if !strings.HasPrefix(err.Error(), "abnf: ") {
			t.Errorf("%s: diagnostic must name ABNF, got %q", label, err.Error())
		}
		if !strings.Contains(err.Error(), c.want) {
			t.Errorf("%s: got %q, want it to mention %q", label, err.Error(), c.want)
		}
	}
}

// A repetition is ONE part, so its whole run is one element. This is not
// the behaviour anyone wants from `; @array` on the ABNF list idiom, and
// ts/doc/guide.md says so — but it is the behaviour, and pinning it means
// a change to it has to be deliberate rather than a surprise.
func TestAnnotationTakesARepetitionAsOneElement(t *testing.T) {
	src := "list = item *( \",\" item )   ; @array\nitem = 1*DIGIT\n"
	if got := annotBuild(t, src, "1,2,3", "list"); !jsonEq(got, []any{"1", ",2,3"}) {
		t.Errorf("got %#v, want [1 ,2,3]", got)
	}
	// An empty run is an empty-string element, not an absent one.
	if got := annotBuild(t, src, "1", "list"); !jsonEq(got, []any{"1", ""}) {
		t.Errorf("got %#v, want [1 \"\"]", got)
	}
}

// Not a refusal — the opposite. A pure alias is the one caller
// left-recursion elimination does not substitute into, so an annotated
// alias of an annotated rule works and must not be caught by the
// leading-reference rule above.
func TestAnnotationNestsThroughAPureAlias(t *testing.T) {
	src := "top = child   ; @object child\nchild = d   ; @object d\nd = 1*DIGIT\n"
	got := annotBuild(t, src, "7", "top")
	want := map[string]any{"child": map[string]any{"d": "7"}}
	if !jsonEq(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}
