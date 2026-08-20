// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

// leftrec_test.go — unit tests for the left-recursion elimination pass,
// mirroring the "left-recursion elimination" suite in
// ../ts/test/abnf.test.js so the two implementations stay in lockstep.
// (The fixture-driven positive/equivalence tests live in abnf_test.go;
// these pin the rewrite structure and the purely-left-recursive rejection.)

package tabnasabnf

import (
	"fmt"
	"strings"
	"testing"
)

// TestEliminateLeftRecursionRewrite: P = P alpha / beta becomes
// P = beta (alpha)* — one alt whose first element is the seed and whose
// second is a star of the recursive tail. Mirrors the TS
// "rewrites P -> P alpha | beta into P -> beta (alpha)*".
func TestEliminateLeftRecursionRewrite(t *testing.T) {
	g, err := parseAbnf("e = e \"+\" t / t\nt = \"1\"")
	if err != nil {
		t.Fatalf("parseAbnf: %v", err)
	}
	r := eliminateLeftRecursion(g)
	e := findProd(r, "e")
	if e == nil {
		t.Fatal("production 'e' missing after elimination")
	}
	if len(e.Alts) != 1 {
		t.Fatalf("e.Alts = %d, want 1", len(e.Alts))
	}
	alt := e.Alts[0]
	if len(alt) != 2 {
		t.Fatalf("e.Alts[0] len = %d, want 2", len(alt))
	}
	// Seed is t's body ("1") inlined (Paull's topo-orders t before e).
	if alt[0].Kind != kindTerm {
		t.Errorf("seed kind = %q, want %q", alt[0].Kind, kindTerm)
	}
	// Recursive tail wrapped in a star.
	if alt[1].Kind != kindStar {
		t.Errorf("tail kind = %q, want %q", alt[1].Kind, kindStar)
	}
}

// TestEliminateLeftRecursionMultipleAlts: several recursive and several
// seed alternatives group correctly — seed becomes a group of the
// non-recursive alts, the star's inner a group of the recursive tails.
// Mirrors the TS "handles multiple recursive and seed alternatives".
func TestEliminateLeftRecursionMultipleAlts(t *testing.T) {
	g, err := parseAbnf("e = e \"+\" t / e \"-\" t / t / \"(\" e \")\"\nt = \"1\"")
	if err != nil {
		t.Fatalf("parseAbnf: %v", err)
	}
	r := eliminateLeftRecursion(g)
	e := findProd(r, "e")
	if e == nil {
		t.Fatal("production 'e' missing after elimination")
	}
	if len(e.Alts) != 1 {
		t.Fatalf("e.Alts = %d, want 1", len(e.Alts))
	}
	seed, star := e.Alts[0][0], e.Alts[0][1]
	if seed.Kind != kindGroup || len(seed.Alts) != 2 {
		t.Errorf("seed = {kind:%q, alts:%d}, want {group, 2}", seed.Kind, len(seed.Alts))
	}
	if star.Kind != kindStar {
		t.Fatalf("tail kind = %q, want %q", star.Kind, kindStar)
	}
	if star.Inner == nil || star.Inner.Kind != kindGroup || len(star.Inner.Alts) != 2 {
		t.Errorf("star.Inner = %+v, want a group of 2 alts", star.Inner)
	}
}

// TestRejectsPurelyLeftRecursive: a rule with no seed (non-recursive)
// alternative cannot be eliminated and is rejected. Mirrors the TS
// "rejects purely left-recursive productions (no seed)".
//
// EACH PORT SIGNALS FAILURE ITS OWN WAY, and that is not a divergence: TS
// throws, Go rejects, and the message is the same in both.
//
// HOW Go rejects depends on which bnf is linked, so this asserts the
// invariant rather than the mechanism:
//
//	bnf <= v0.1.8 (what go.mod pins)   panics with an *EmitError
//	bnf main (what CI links)           returns it, via the recover that
//	                                   tabnas/bnf#28 added at emit.go's
//	                                   boundary — "invalid user input is
//	                                   an error return"
//
// Asserting either one alone fails in the other build. A panic escaping a
// published API is the defect bnf#28 removes; a test that DEMANDS the
// panic pins the defect, and one that demands the error cannot run against
// any released bnf, because #28 is not published yet. So this accepts
// either and says which it saw. When the release wave bumps the pin past
// #28, the panic branch stops being reached and can go.
//
// The message is asserted either way, because a rule with no seed and a
// rule that merely fails to compile are different things and only one of
// them is this test's subject.
func TestRejectsPurelyLeftRecursive(t *testing.T) {
	rejected, how := rejectPurelyLeftRecursive(t)
	if !strings.Contains(rejected, "purely left-recursive") {
		t.Errorf("%s = %q, want it to mention 'purely left-recursive'",
			how, rejected)
	}
	t.Logf("linked bnf rejects by %s", how)
}

// rejectPurelyLeftRecursive returns the rejection message and how it
// arrived, failing the test if the input is not rejected at all.
func rejectPurelyLeftRecursive(t *testing.T) (msg, how string) {
	t.Helper()
	defer func() {
		if r := recover(); nil != r {
			msg, how = fmt.Sprint(r), "panic"
		}
	}()
	_, err := Abnf("a = a \"x\"", nil)
	if nil == err {
		t.Fatal("a purely left-recursive rule was not rejected: " +
			"no error returned and no panic raised")
	}
	return err.Error(), "error return"
}

// TestDropsTrivialSelfRef: a trivial `P = P` alternative adds nothing and
// is dropped, leaving the rule's real language intact. Mirrors the TS
// "silently drops trivial P = P alternatives".
func TestDropsTrivialSelfRef(t *testing.T) {
	j := makeParser(t, "a = a / \"x\"", nil)
	if _, err := j.Parse("x"); err != nil {
		t.Errorf("expected accept %q, got error: %v", "x", firstLineOf(err))
	}
	if _, err := j.Parse("y"); err == nil {
		t.Errorf("expected reject %q, but it parsed", "y")
	}
}
