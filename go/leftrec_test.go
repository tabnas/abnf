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
// alternative cannot be eliminated and is rejected. The TS form throws;
// the Go form panics with the same message. Mirrors the TS
// "rejects purely left-recursive productions (no seed)".
func TestRejectsPurelyLeftRecursive(t *testing.T) {
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected a panic for a purely left-recursive rule, got none")
		}
		if msg := fmt.Sprint(r); !strings.Contains(msg, "purely left-recursive") {
			t.Errorf("panic = %q, want it to mention 'purely left-recursive'", msg)
		}
	}()
	_, _ = Abnf("a = a \"x\"", nil)
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
