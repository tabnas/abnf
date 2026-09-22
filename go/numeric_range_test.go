// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// numeric_range_test.go — a reversed numeric range is REFUSED, never a
// panic (tabnas/abnf#72).
//
// `%x5A-41` names a range whose start is above its end. All three
// runtimes refuse it in the words of the regular expression engine they
// carry (DIVERGENCE.md entry 6): TypeScript throws V8's sentence, Rust
// returns the regex crate's. Go used to PANIC out of regexp.MustCompile
// in the shared compiler, which is the one outcome a compiler over
// untrusted input may not have. The boundary in bnf_alias.go turns that
// panic into an error return; this file pins the return and the
// message, and pins that the well-ordered range still compiles.

import (
	"fmt"
	"strings"
	"testing"

	bnf "github.com/tabnas/bnf/go"
)

// refuse converts src and reports how it was refused: by an error
// return, or by a panic that escaped the published API.
func refuse(t *testing.T, src string) (msg, how string) {
	t.Helper()
	defer func() {
		if r := recover(); nil != r {
			msg, how = strings.TrimSpace(strings.SplitN(fmt.Sprint(r), "\n", 2)[0]), "panic"
		}
	}()
	_, err := Abnf(src, nil)
	if nil == err {
		return "", "accepted"
	}
	return err.Error(), "error return"
}

func TestReversedNumericRangeIsAnErrorReturn(t *testing.T) {
	for _, src := range []string{
		"g = %x5A-41\n",
		"g = %d90-65\n",
		"g = %b1011010-1000001\n",
		// Inside a repetition and a group, so the guard is shown to sit
		// at the emit boundary rather than on one element shape.
		"g = 1*( \"a\" / %x5A-41 )\n",
	} {
		msg, how := refuse(t, src)
		if "error return" != how {
			t.Fatalf("%q was refused by %s, want an error return: %s", src, how, msg)
		}
		// The wording is Go's regexp package's, prefixed with the
		// notation tag like every other diagnostic here.
		if !strings.HasPrefix(msg, "abnf: invalid regular expression: ") ||
			!strings.Contains(msg, "invalid character class range") {
			t.Errorf("%q: got %q, want the regexp engine's class-range complaint under the abnf: prefix", src, msg)
		}
	}
}

func TestReversedNumericRangeIsAnEmitError(t *testing.T) {
	// The conformance child, and any caller sorting refusals from
	// crashes, classifies by type: this is a rejection of the grammar,
	// so it arrives as the shared compiler's own error type.
	_, err := Abnf("g = %x5A-41\n", nil)
	if _, ok := err.(*bnf.EmitError); !ok {
		t.Fatalf("got %T (%v), want *bnf.EmitError", err, err)
	}
}

func TestWellOrderedNumericRangeStillCompiles(t *testing.T) {
	for _, src := range []string{"g = %x41-5A\n", "g = %d65-90\n", "g = %x41-41\n"} {
		if msg, how := refuse(t, src); "accepted" != how {
			t.Errorf("%q was refused by %s: %s", src, how, msg)
		}
	}
	j := makeParser(t, "g = %x41-5A\n", nil)
	assertAccept(t, j, "Q")
	assertReject(t, j, "q")
}
