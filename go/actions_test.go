package abnf

import (
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// nodeMap returns the rule's AST node as a map (the tree builtins build
// map[string]any nodes).
func nodeMap(r *tabnas.Rule) map[string]any {
	m, _ := r.Node.(map[string]any)
	return m
}

func TestActionsBindByMark(t *testing.T) {
	var log []string
	rh := 4096
	j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
	_, err := Install(j, `op = "inc" / "dec"`, nil, ActionsMap{
		"@op:o:INC": {func(r *tabnas.Rule, _ *tabnas.Context) {
			log = append(log, "inc")
			if n := nodeMap(r); n != nil {
				n["delta"] = 1
			}
		}},
		"@op:o:DEC": {func(r *tabnas.Rule, _ *tabnas.Context) {
			log = append(log, "dec")
			if n := nodeMap(r); n != nil {
				n["delta"] = -1
			}
		}},
	})
	if err != nil {
		t.Fatal(err)
	}
	inc, err := j.Parse("inc")
	if err != nil {
		t.Fatal(err)
	}
	dec, err := j.Parse("dec")
	if err != nil {
		t.Fatal(err)
	}
	im := inc.(map[string]any)
	dm := dec.(map[string]any)
	if im["delta"] != 1 {
		t.Errorf("inc delta = %v, want 1", im["delta"])
	}
	if dm["delta"] != -1 {
		t.Errorf("dec delta = %v, want -1", dm["delta"])
	}
	if im["rule"] != "op" {
		t.Errorf("inc rule = %v, want op (compiler tree action ran first)", im["rule"])
	}
	if strings.Join(log, ",") != "inc,dec" {
		t.Errorf("log = %v, want [inc dec]", log)
	}
}

func TestActionsMultipleRunInOrder(t *testing.T) {
	var log []string
	j := tabnas.Make()
	_, err := Install(j, `op = "inc"`, nil, ActionsMap{
		"@op:o:INC": {
			func(r *tabnas.Rule, _ *tabnas.Context) { log = append(log, "a") },
			func(r *tabnas.Rule, _ *tabnas.Context) { log = append(log, "b") },
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := j.Parse("inc"); err != nil {
		t.Fatal(err)
	}
	if strings.Join(log, ",") != "a,b" {
		t.Errorf("log = %v, want [a b]", log)
	}
}

func TestActionsRulePhaseHook(t *testing.T) {
	var log []string
	j := tabnas.Make()
	_, err := Install(j, `g = "x"`, nil, ActionsMap{
		"@g:bo": {func(r *tabnas.Rule, _ *tabnas.Context) { log = append(log, "enter") }},
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := j.Parse("x"); err != nil {
		t.Fatal(err)
	}
	if strings.Join(log, ",") != "enter" {
		t.Errorf("log = %v, want [enter]", log)
	}
}

func TestActionsRejectBadRefs(t *testing.T) {
	for _, bad := range []string{"@op:o:NOPE", "@nope:o:INC", "@op:zz"} {
		spec, err := Bnf(`op = "inc" / "dec"`, &BnfConvertOptions{Marks: true})
		if err != nil {
			t.Fatal(err)
		}
		err = AttachActions(spec, ActionsMap{bad: {func(r *tabnas.Rule, _ *tabnas.Context) {}}})
		if err == nil {
			t.Errorf("should reject %q", bad)
		} else if _, ok := err.(*BnfActionError); !ok {
			t.Errorf("%q: expected *BnfActionError, got %T", bad, err)
		}
	}
}

func TestMarkListing(t *testing.T) {
	spec, err := Bnf(`op = "inc" / "dec"`, &BnfConvertOptions{Marks: true})
	if err != nil {
		t.Fatal(err)
	}
	listing := MarkListing(spec)
	if !strings.Contains(listing, "op") || !strings.Contains(listing, "o:INC") {
		t.Errorf("listing missing op o:INC:\n%s", listing)
	}
	if !strings.Contains(listing, "o:DEC") {
		t.Errorf("listing missing o:DEC:\n%s", listing)
	}
}

func TestMarksOptIn(t *testing.T) {
	// Default conversion carries no marks (m$ slot).
	spec, err := Bnf(`op = "inc" / "dec"`, nil)
	if err != nil {
		t.Fatal(err)
	}
	for name, rs := range spec.Rule {
		for _, a := range append(altSpecs(rs.Open), altSpecs(rs.Close)...) {
			if a != nil && a.U != nil {
				if _, ok := a.U["m$"]; ok {
					t.Errorf("rule %q carries a mark without marks:true", name)
				}
			}
		}
	}
}

func TestActionsBuiltinsModeComposition(t *testing.T) {
	// builtins-mode spec: user action runs after the @node$ tree builtin.
	spec, err := Bnf(`op = "inc" / "dec"`, &BnfConvertOptions{Builtins: true, Marks: true})
	if err != nil {
		t.Fatal(err)
	}
	if err := AttachActions(spec, ActionsMap{
		"@op:o:INC": {func(r *tabnas.Rule, _ *tabnas.Context) {
			if n := nodeMap(r); n != nil {
				n["delta"] = 1
			}
		}},
	}); err != nil {
		t.Fatal(err)
	}
	j := tabnas.Make()
	if err := j.Grammar(spec); err != nil {
		t.Fatal(err)
	}
	out, err := j.Parse("inc")
	if err != nil {
		t.Fatal(err)
	}
	m := out.(map[string]any)
	if m["rule"] != "op" {
		t.Errorf("tree not built by @node$: rule=%v", m["rule"])
	}
	if m["delta"] != 1 {
		t.Errorf("user action did not run: delta=%v", m["delta"])
	}
}
