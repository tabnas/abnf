package tabnasabnf

import (
	"regexp"
	"testing"
)

// End-to-end test for RFC 3986 Appendix A — the collected ABNF grammar
// for URI. Mirrors ts/test/rfc3986.test.js. Exercises hyphenated rule
// names, =/, /, [ ], ( ), prefix repetition, %x numeric values,
// case-insensitive literals, transitive core-rule inclusion, ; comments,
// and the probe + phase-retry dispatcher for the authority ambiguity.

func rfcGrammar(t *testing.T) string { return loadFixture(t, "rfc3986-uri.abnf") }

func TestRfcCompiles(t *testing.T) {
	if _, err := Abnf(rfcGrammar(t), &AbnfConvertOptions{Start: "URI"}); err != nil {
		t.Fatalf("compile RFC 3986: %v", err)
	}
}

func TestRfcEveryProductionSurvives(t *testing.T) {
	spec, err := Abnf(rfcGrammar(t), &AbnfConvertOptions{Start: "URI"})
	if err != nil {
		t.Fatal(err)
	}
	expected := []string{
		"URI", "hier-part", "scheme", "authority", "userinfo",
		"host", "port", "IP-literal", "IPvFuture", "IPv6address",
		"h16", "ls32", "IPv4address", "dec-octet", "reg-name",
		"path-abempty", "path-absolute", "path-rootless",
		"path-empty", "segment", "segment-nz", "pchar", "query",
		"fragment", "pct-encoded", "unreserved", "sub-delims",
		"ALPHA", "DIGIT", "HEXDIG",
	}
	have := map[string]bool{}
	for _, n := range ruleNames(spec) {
		have[n] = true
	}
	for _, n := range expected {
		if !have[n] {
			t.Errorf("missing rule %q in emitted spec", n)
		}
	}
}

func TestRfcAuthorityAmbiguityRewritten(t *testing.T) {
	spec, err := Abnf(rfcGrammar(t), &AbnfConvertOptions{Start: "URI"})
	if err != nil {
		t.Fatal(err)
	}
	names := ruleNames(spec)
	for _, pat := range []string{
		`^authority\$pd\d+\$probe$`,
		`^authority\$pd\d+\$with$`,
		`^authority\$pd\d+\$no$`,
	} {
		if !anyMatch(names, regexp.MustCompile(pat)) {
			t.Errorf("expected a rule matching %q (authority probe rewrite)", pat)
		}
	}
}

func TestRfcURIAcceptance(t *testing.T) {
	j := makeParser(t, rfcGrammar(t), &AbnfConvertOptions{Start: "URI"})
	accept := []string{
		"urn:isbn:0451450523",
		"mailto:alice@example.com",
		"tag:yaml.org,2002:int",
		"http://[::1]/",
		"http://example.com",
		"http://example.com:8080",
		"ftp://user@host",
		"http://user@example.com:8080",
		"http://user:pass@example.com:8080/some/path",
		"https://www.example.org/path/to/resource?name=value&other=thing#section",
	}
	for _, u := range accept {
		assertAccept(t, j, u)
	}
	for _, u := range []string{"not a uri", ":foo"} {
		assertReject(t, j, u)
	}
}
