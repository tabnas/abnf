// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// conformance_test.go — third-party ABNF conformance, Go half.
//
// The mirror of ts/test/conformance.test.js. Both read the SAME corpus
// (test/abnf-corpus/, fetched by scripts/fetch-abnf-corpus.sh at pinned
// commit SHAs, never committed), the SAME classification manifest
// (test/corpus/manifest.tsv) and the SAME mutation table
// (test/corpus/mutations.tsv), so the two runtimes cannot report different
// conformance without one of them going red.
//
// See the TS file for the full rationale. Two things bear repeating:
//
//   - THIS SUITE CANNOT SKIP. Missing corpus => t.Fatalf with the fetch
//     command. A conformance test that quietly does not run is worse than
//     no test, because the green tick is a lie.
//
//   - VALID GRAMMARS GET A VALUE ASSERTION, not merely "it didn't error":
//     every rulename the source declares (RFC 5234 s4, `rule = rulename
//     defined-as elements c-nl`) must be reachable in the compiled
//     GrammarSpec as a rule, a fixed token or a match token.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

const conformanceFetchHint = "\n  Fetch it with:  ./scripts/fetch-abnf-corpus.sh" +
	"\n  This test MUST NOT be skipped — a conformance run that silently does" +
	"\n  not happen is the exact defect this suite exists to prevent."

const (
	// Go and TS DIVERGE here: TS is 39/620. See the Phase-1 report.

	// TRUE Phase-1 baseline, observed by running this suite. Not a target.
	conformanceValidBaseline   = 42
	conformanceInvalidBaseline = 557
)

func corpusDir() string { return filepath.Join("..", "test", "abnf-corpus") }

// TestMain fetches the corpus before any test runs.
//
// `go test` has no `pretest` hook (the TS side gets one via package.json), and
// the shared CI workflow that runs `go test ./...` lives in the tabnas/.github
// repo, which this repo cannot edit. So the fetch happens here instead: the
// script is idempotent, so this is a no-op once the corpus is at its pinned
// SHA. If the fetch cannot run (no network, no git) the conformance tests
// still FAIL LOUDLY rather than skipping — a conformance run that silently
// does not happen is worse than no test, because the green tick is a lie.
func TestMain(m *testing.M) {
	if _, err := os.Stat(corpusDir()); err != nil {
		script := filepath.Join("..", "scripts", "fetch-abnf-corpus.sh")
		if _, err := os.Stat(script); err == nil {
			cmd := exec.Command("bash", script)
			cmd.Stdout, cmd.Stderr = os.Stderr, os.Stderr
			if err := cmd.Run(); err != nil {
				fmt.Fprintf(os.Stderr,
					"WARNING: %s failed (%v); conformance tests will FAIL, not skip.\n",
					script, err)
			}
		}
	}
	os.Exit(m.Run())
}

func loadCorpusTSV(t *testing.T, name string) [][]string {
	t.Helper()
	path := filepath.Join("..", "test", "corpus", name)
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("missing %s: %v", path, err)
	}
	var rows [][]string
	for i, line := range strings.Split(string(raw), "\n") {
		line = strings.TrimRight(line, "\r")
		if i == 0 || strings.TrimSpace(line) == "" {
			continue
		}
		rows = append(rows, strings.Split(line, "\t"))
	}
	return rows
}

// RFC 5234 s4:  rule = rulename defined-as elements c-nl
// A rulename starts in column 1; "=" or "=/" follows.
var ruleDeclRe = regexp.MustCompile(`(?m)^([A-Za-z][A-Za-z0-9-]*)[ \t]*=/?`)

func declaredRules(src string) []string {
	seen := map[string]bool{}
	var out []string
	for _, m := range ruleDeclRe.FindAllStringSubmatch(src, -1) {
		// Exclude the `= /...` shape the TS regex excludes: a body that
		// begins with "/" is not legal ABNF anyway, but keep the two
		// extractors byte-identical in behaviour.
		n := strings.ToLower(m[1])
		if !seen[n] {
			seen[n] = true
			out = append(out, n)
		}
	}
	sort.Strings(out)
	return out
}

// Every name the compiled spec can reach: rules, fixed tokens, match tokens.
func specNames(spec *tabnas.GrammarSpec) map[string]bool {
	out := map[string]bool{}
	add := func(k string) { out[strings.ToLower(strings.TrimPrefix(k, "#"))] = true }
	if spec == nil {
		return out
	}
	for k := range spec.Rule {
		add(k)
	}
	if spec.Options != nil {
		if spec.Options.Fixed != nil {
			for k := range spec.Options.Fixed.Token {
				add(k)
			}
		}
		if spec.Options.Match != nil {
			for k := range spec.Options.Match.Token {
				add(k)
			}
		}
	}
	return out
}

func readCorpusFile(t *testing.T, rel string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(corpusDir(), rel))
	if err != nil {
		t.Fatalf("corpus file missing: %s: %v%s", rel, err, conformanceFetchHint)
	}
	return string(raw)
}

// RFC 5234 lines are CRLF-terminated.
var trailingNLRe = regexp.MustCompile(`\r?\n*$`)

func applyMutation(base, appendLine string) string {
	return trailingNLRe.ReplaceAllString(base, "") + "\r\n" + appendLine + "\r\n"
}

func TestConformanceCorpusPresent(t *testing.T) {
	dir := corpusDir()
	if _, err := os.Stat(dir); err != nil {
		t.Fatalf("ABNF conformance corpus missing at %s%s", dir, conformanceFetchHint)
	}
	count := 0
	_ = filepath.Walk(dir, func(p string, info os.FileInfo, err error) error {
		if err == nil && info != nil && !info.IsDir() && strings.HasSuffix(p, ".abnf") {
			count++
		}
		return nil
	})
	if count < 60 {
		t.Fatalf("expected >=60 .abnf corpus files, found %d%s", count, conformanceFetchHint)
	}
}

func conformanceSets(t *testing.T) (valid, invalid, fragment []string, muts [][]string) {
	t.Helper()
	for _, r := range loadCorpusTSV(t, "manifest.tsv") {
		switch r[1] {
		case "valid":
			valid = append(valid, r[0])
		case "invalid":
			invalid = append(invalid, r[0])
		case "fragment":
			fragment = append(fragment, r[0])
		default:
			t.Fatalf("manifest row %q has unknown class %q", r[0], r[1])
		}
	}
	muts = loadCorpusTSV(t, "mutations.tsv")
	if len(valid) < 50 || len(invalid) < 10 || len(muts) < 13 {
		t.Fatalf("degenerate corpus: %d valid / %d invalid / %d mutations",
			len(valid), len(invalid), len(muts))
	}
	return
}

// Half 1: valid grammars must compile AND yield every declared rule.
func TestConformanceValid(t *testing.T) {
	valid, _, _, _ := conformanceSets(t)
	for _, rel := range valid {
		rel := rel
		t.Run(rel, func(t *testing.T) {
			src := readCorpusFile(t, rel)
			spec, err := Abnf(src, nil)
			if err != nil {
				t.Fatalf("valid RFC 5234 grammar rejected: %v", err)
			}
			names := specNames(spec)
			var missing []string
			for _, n := range declaredRules(src) {
				if !names[n] {
					missing = append(missing, n)
				}
			}
			if len(missing) > 0 {
				t.Fatalf("compiled, but %d declared rule(s) vanished from the "+
					"GrammarSpec: %s", len(missing), strings.Join(missing, ", "))
			}
		})
	}
}

// Half 2a: corpus grammars the third-party oracle rejects must be rejected.
func TestConformanceInvalidCorpus(t *testing.T) {
	_, invalid, _, _ := conformanceSets(t)
	for _, rel := range invalid {
		rel := rel
		t.Run(rel, func(t *testing.T) {
			if _, err := Abnf(readCorpusFile(t, rel), nil); err == nil {
				t.Fatalf("accepted a grammar the third-party oracle rejects as non-RFC-5234")
			}
		})
	}
}

// Half 2b: mutants violating a named RFC 5234 production must be rejected.
func TestConformanceInvalidMutants(t *testing.T) {
	valid, _, _, muts := conformanceSets(t)
	for _, m := range muts {
		name, appendLine, violates := m[0], m[1], m[2]
		t.Run(name, func(t *testing.T) {
			var leaked []string
			for _, rel := range valid {
				src := applyMutation(readCorpusFile(t, rel), appendLine)
				if _, err := Abnf(src, nil); err == nil {
					leaked = append(leaked, rel)
				}
			}
			if len(leaked) > 0 {
				show := leaked
				if len(show) > 3 {
					show = show[:3]
				}
				t.Fatalf("%d/%d bases accepted the appended line %q, which RFC 5234 "+
					"cannot derive (%s). e.g. %s",
					len(leaked), len(valid), appendLine, violates, strings.Join(show, ", "))
			}
		})
	}
}

// The dial. Prints the true numbers and ratchets them, so the figure cannot
// silently regress. These are NOT a pass mark: the suites above are already
// red, deliberately — this is Phase 1, an instrument, not a fix.
func TestConformanceDial(t *testing.T) {
	valid, invalid, fragment, muts := conformanceSets(t)

	validOK := 0
	for _, rel := range valid {
		src := readCorpusFile(t, rel)
		spec, err := Abnf(src, nil)
		if err != nil {
			continue
		}
		names := specNames(spec)
		all := true
		for _, n := range declaredRules(src) {
			if !names[n] {
				all = false
				break
			}
		}
		if all {
			validOK++
		}
	}

	invalidOK, invalidTotal := 0, 0
	for _, rel := range invalid {
		invalidTotal++
		if _, err := Abnf(readCorpusFile(t, rel), nil); err != nil {
			invalidOK++
		}
	}
	for _, rel := range valid {
		base := readCorpusFile(t, rel)
		for _, m := range muts {
			invalidTotal++
			if _, err := Abnf(applyMutation(base, m[1]), nil); err != nil {
				invalidOK++
			}
		}
	}

	t.Logf("\n  ABNF conformance dial (Go):"+
		"\n    valid   accepted+value-correct : %d/%d"+
		"\n    invalid rejected               : %d/%d"+
		"\n    excluded fragments             : %d",
		validOK, len(valid), invalidOK, invalidTotal, len(fragment))

	// Ratchet: must never go DOWN. Raising them is a real fix.
	if validOK < conformanceValidBaseline {
		t.Fatalf("valid-accepted regressed: %d < %d", validOK, conformanceValidBaseline)
	}
	if invalidOK < conformanceInvalidBaseline {
		t.Fatalf("invalid-rejected regressed: %d < %d", invalidOK, conformanceInvalidBaseline)
	}
}
