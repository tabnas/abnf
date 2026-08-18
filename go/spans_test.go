package tabnasabnf

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	bnf "github.com/tabnas/bnf/go"
)

// Source spans (plan C5). The front-end records where each element and
// production came from, so a compile failure carries a range and a tool
// can underline the offending text.
//
// Every assertion slices the ORIGINAL SOURCE with the span and compares
// the text. That is the only check worth making: an offset pair that is
// self-consistent but points at the wrong characters would satisfy any
// assertion about the numbers themselves.
//
// Mirrors ts/test/abnf.test.js `describe('source spans')`.

const spanSrc = "doc  = item\n" +
	"item = \"hi\" / %s\"Yo\" / ref / (alt / two) / [opt] / %x41-5A\n" +
	"ref  = ALPHA\n" +
	"alt  = \"a\"\n" +
	"two  = \"b\"\n" +
	"opt  = \"c\""

func spanGrammar(t *testing.T) *abnfGrammar {
	t.Helper()
	g, err := ParseAbnf(spanSrc)
	if err != nil {
		t.Fatalf("ParseAbnf: %v", err)
	}
	return g
}

func spanText(t *testing.T, sp *bnf.SrcSpan) string {
	t.Helper()
	if sp == nil {
		t.Fatal("expected a span, got none")
	}
	return spanSrc[sp.S:sp.E]
}

func spanProd(t *testing.T, name string) *abnfProduction {
	t.Helper()
	for _, p := range spanGrammar(t).Productions {
		if p.Name == name {
			return p
		}
	}
	t.Fatalf("no production %q", name)
	return nil
}

func TestProductionSpansItsName(t *testing.T) {
	p := spanProd(t, "item")
	if got := spanText(t, p.Sp); got != "item" {
		t.Errorf("production span = %q, want %q", got, "item")
	}
	if p.Sp.R != 2 || p.Sp.C != 1 {
		t.Errorf("row/col = %d:%d, want 2:1 (1-based, as the engine reports)",
			p.Sp.R, p.Sp.C)
	}
}

func TestStringTerminalSpansIncludeAnyPrefix(t *testing.T) {
	alts := spanProd(t, "item").Alts
	if got := spanText(t, alts[0][0].Sp); got != `"hi"` {
		t.Errorf("bare string span = %q, want %q", got, `"hi"`)
	}
	if got := spanText(t, alts[1][0].Sp); got != `%s"Yo"` {
		t.Errorf("%%s string span = %q, want %q", got, `%s"Yo"`)
	}
}

func TestRefNumericAndProseSpans(t *testing.T) {
	alts := spanProd(t, "item").Alts
	if got := spanText(t, alts[2][0].Sp); got != "ref" {
		t.Errorf("ref span = %q, want %q", got, "ref")
	}
	if got := spanText(t, alts[5][0].Sp); got != "%x41-5A" {
		t.Errorf("numeric span = %q, want %q", got, "%x41-5A")
	}
}

func TestGroupAndOptionalSpanTheirDelimiters(t *testing.T) {
	alts := spanProd(t, "item").Alts

	group := alts[3][0]
	if group.Kind != kindGroup {
		t.Fatalf("expected a group, got %v", group.Kind)
	}
	if got := spanText(t, group.Sp); got != "(alt / two)" {
		t.Errorf("group span = %q, want %q", got, "(alt / two)")
	}
	if got := spanText(t, group.Alts[0][0].Sp); got != "alt" {
		t.Errorf("inner span = %q, want %q", got, "alt")
	}
	if got := spanText(t, group.Alts[1][0].Sp); got != "two" {
		t.Errorf("inner span = %q, want %q", got, "two")
	}

	optional := alts[4][0]
	if optional.Kind != kindOpt {
		t.Fatalf("expected an opt, got %v", optional.Kind)
	}
	if got := spanText(t, optional.Sp); got != "[opt]" {
		t.Errorf("optional span = %q, want %q", got, "[opt]")
	}
	if got := spanText(t, optional.Inner.Sp); got != "[opt]" {
		t.Errorf("optional inner span = %q, want %q", got, "[opt]")
	}
}

// The RFC 5234 core rules are parsed from a string inside the converter,
// not from the user's grammar, so any offset would index a document the
// user never wrote. A missing span means "nowhere to point", which is
// right; a wrong one is worse than none.
func TestCoreRulesCarryNoSpan(t *testing.T) {
	var alpha *abnfProduction
	for _, p := range spanGrammar(t).Productions {
		if p.Name == "ALPHA" {
			alpha = p
		}
	}
	if alpha == nil {
		t.Fatal("ALPHA should have been pulled in")
	}
	if alpha.NodeKind != "core" {
		t.Errorf("NodeKind = %q, want core", alpha.NodeKind)
	}
	if alpha.Sp != nil {
		t.Errorf("a core rule must carry no span, got %+v", alpha.Sp)
	}
	for _, alt := range alpha.Alts {
		for _, el := range alt {
			if el.Sp != nil {
				t.Errorf("core rule element must carry no span, got %+v", el.Sp)
			}
		}
	}
	// ...but the user's REFERENCE to it does: that reference is in their
	// source, and it is what a diagnostic points at.
	if got := spanText(t, spanProd(t, "ref").Alts[0][0].Sp); got != "ALPHA" {
		t.Errorf("reference span = %q, want %q", got, "ALPHA")
	}
}

// A span whose offset and row/column disagree is worse than no span: a
// consumer picking either one gets a different answer.
func TestSpanRowAndColumnAgreeWithTheOffset(t *testing.T) {
	for _, p := range spanGrammar(t).Productions {
		if p.Sp == nil {
			continue // core rules, deliberately
		}
		before := spanSrc[:p.Sp.S]
		row := strings.Count(before, "\n") + 1
		col := p.Sp.S - (strings.LastIndex(before, "\n") + 1) + 1
		if p.Sp.R != row {
			t.Errorf("%s: row = %d, want %d (from the offset)", p.Name, p.Sp.R, row)
		}
		if p.Sp.C != col {
			t.Errorf("%s: col = %d, want %d (from the offset)", p.Name, p.Sp.C, col)
		}
	}
}

func TestCompileErrorCarriesARange(t *testing.T) {
	src := "doc = item\nitem = missing"
	_, err := Abnf(src, nil)
	if err == nil {
		t.Fatal("expected an unknown-rule failure")
	}
	var emit *bnf.EmitError
	if !errors.As(err, &emit) {
		t.Fatalf("expected a *bnf.EmitError, got %T: %v", err, err)
	}
	sp := emit.Sp
	if sp == nil {
		t.Fatalf("compile error carried no range: %v", err)
	}
	if got := src[sp.S:sp.E]; got != "missing" {
		t.Errorf("range covers %q, want %q", got, "missing")
	}
	if sp.R != 2 {
		t.Errorf("row = %d, want 2", sp.R)
	}
}

func TestSpansDoNotReachTheEmittedGrammar(t *testing.T) {
	spec, err := Abnf("doc = item\nitem = \"hi\" / (a / b)\na = \"x\"\nb = ALPHA", nil)
	if err != nil {
		t.Fatalf("AbnfConvert: %v", err)
	}
	raw, err := json.Marshal(spec.Rule)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if strings.Contains(string(raw), "\"Sp\"") ||
		strings.Contains(string(raw), "\"sp\"") {
		t.Errorf("a span reached the emitted grammar:\n%s", raw)
	}
}
