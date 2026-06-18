package abnf

import (
	"regexp"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

// ---- probe-dispatch: synthetic [X D] Y pattern ---------------------

const probeGrammar = `
top = [ X "@" ] Y
X   = *( ALPHA )
Y   = *( ALPHA )
`

func TestProbeYOnly(t *testing.T) {
	j := makeParser(t, probeGrammar, nil)
	assertAccept(t, j, "abc")
}

func TestProbeXPresent(t *testing.T) {
	j := makeParser(t, probeGrammar, nil)
	assertAccept(t, j, "ab@cd")
}

func TestProbeEmptyXPresent(t *testing.T) {
	j := makeParser(t, probeGrammar, nil)
	assertAccept(t, j, "@cd")
}

func TestProbeEmptyInput(t *testing.T) {
	j := makeParser(t, probeGrammar, nil)
	assertAccept(t, j, " ")
}

// ---- emitter shape -------------------------------------------------

func TestProbeEmitterShape(t *testing.T) {
	spec, err := Bnf(probeGrammar, nil)
	if err != nil {
		t.Fatal(err)
	}
	names := ruleNames(spec)
	checks := []struct {
		re   string
		desc string
	}{
		{`^top\$pd\d+$`, "dispatcher rule top$pdN"},
		{`^top\$pd\d+\$probe$`, "probe helper rule"},
		{`^top\$pd\d+\$with$`, "with-branch rule"},
		{`^top\$pd\d+\$no$`, "no-branch rule"},
	}
	for _, c := range checks {
		re := regexp.MustCompile(c.re)
		if !anyMatch(names, re) {
			t.Errorf("expected %s; rule names: %v", c.desc, names)
		}
	}
}

func TestProbeNoAmbiguityLeftAlone(t *testing.T) {
	// The optional's body ends with a terminal "!" that can't be in the
	// tail's (digit) vocabulary, so FIRST-set dispatch suffices.
	spec, err := Bnf("top = [ X \"!\" ] Y\nX   = *( ALPHA )\nY   = *DIGIT", nil)
	if err != nil {
		t.Fatal(err)
	}
	pd := regexp.MustCompile(`\$pd\d+`)
	for _, n := range ruleNames(spec) {
		if pd.MatchString(n) {
			t.Errorf("expected no probe helpers; found %q", n)
		}
	}
}

func TestProbeVocabExcludesDisambiguator(t *testing.T) {
	spec, err := Bnf("top = [ X \"@\" ] Y\nX   = *( ALPHA / \"@\" )\nY   = *( ALPHA )", nil)
	if err != nil {
		t.Fatal(err)
	}
	var probeRule string
	for _, n := range ruleNames(spec) {
		if strings.HasSuffix(n, "$probe") {
			probeRule = n
			break
		}
	}
	if probeRule == "" {
		t.Fatal("expected a probe helper rule")
	}
	// Resolve the probe's leading token names back to their fixed source
	// chars; assert "@" is absent.
	tokSrc := map[string]string{}
	if spec.Options != nil && spec.Options.Fixed != nil {
		for name, src := range spec.Options.Fixed.Token {
			if src != nil {
				tokSrc[name] = *src
			}
		}
	}
	for _, a := range altSpecs(spec.Rule[probeRule].Open) {
		s, _ := a.S.(string)
		if s == "" {
			continue
		}
		first := strings.Fields(s)[0]
		if tokSrc[first] == "@" {
			t.Errorf("probe vocab must not contain disambiguator '@'")
		}
	}
}

// ---- RFC 3986 authority-style ambiguity ----------------------------

const authorityGrammar = `
authority  = [ userinfo "@" ] host [ ":" port ]
userinfo   = *( unreserved / ":" )
host       = reg-name
port       = *DIGIT
reg-name   = *( unreserved )
unreserved = ALPHA / "-" / "."
`

func TestProbeAuthority(t *testing.T) {
	j := makeParser(t, authorityGrammar, nil)
	for _, in := range []string{
		"example.com",
		"example.com:8080",
		"user@example.com",
		"user:pass@example.com",
		"user:pass@example.com:8080",
		"@example.com",
	} {
		assertAccept(t, j, in)
	}
}

// ---- helpers -------------------------------------------------------

func ruleNames(spec *tabnas.GrammarSpec) []string {
	out := make([]string, 0, len(spec.Rule))
	for n := range spec.Rule {
		out = append(out, n)
	}
	return out
}

func anyMatch(names []string, re *regexp.Regexp) bool {
	for _, n := range names {
		if re.MatchString(n) {
			return true
		}
	}
	return false
}

func altSpecs(field any) []*tabnas.GrammarAltSpec {
	if gas, ok := field.([]*tabnas.GrammarAltSpec); ok {
		return gas
	}
	return nil
}
