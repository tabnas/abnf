// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// conformance_test.go — third-party ABNF conformance, Go half.
//
// The mirror of ts/test/conformance.test.js. Both halves read the SAME corpus
// (test/abnf-corpus/, fetched by test/fetch-abnf-corpus.sh at pinned commit
// SHAs, never committed), the SAME third-party classification manifest
// (test/corpus/manifest.tsv), the SAME mutation table
// (test/corpus/mutations.tsv) and the SAME pinned residual gaps
// (test/corpus/known-gaps.tsv, `go` rows). Neither runtime can report a
// different conformance from the other without one of them going red.
//
// See the TS file for the full rationale. Four things bear repeating:
//
//   - IT CANNOT SKIP. Missing corpus => t.Fatalf with the fetch command, and
//     TestMain fetches it first anyway. A conformance test that quietly does
//     not run is worse than no test, because the green tick is a lie.
//
//   - VALID GRAMMARS GET A VALUE ASSERTION, not merely "it did not error":
//     every rulename the source declares (RFC 5234 s4, `rule = rulename
//     defined-as elements c-nl`) must be reachable in the compiled
//     GrammarSpec as a rule, a fixed token or a match token.
//
//   - EVERY COMPILE IS BUDGETED, in its own process. Two real published
//     grammars in the corpus (RFC 5322 email, Dhall) do not terminate in this
//     compiler, in either runtime. Exceeding the budget is recorded as a
//     failure to accept — never as a pass, never as a skip.
//
//   - THE RESIDUAL GAPS ARE AN EXACT SET, not a ratchet. Fixing one fails the
//     suite as loudly as regressing one; the fix is to delete its row from
//     known-gaps.tsv. Never edit a row to silence a failure you did not fix.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"runtime/debug"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	tabnas "github.com/tabnas/parser/go"
)

const (
	// The budget one corpus compile gets, matching the TS half.
	conformanceBudgetBytes = 256 << 20
	conformanceBudget      = 60 * time.Second

	// Set in the subprocess by compileBudgeted; consumed by TestMain.
	envConformanceFile   = "ABNF_CONFORMANCE_FILE"
	envConformanceAppend = "ABNF_CONFORMANCE_APPEND"

	// Print the measured `go` rows of known-gaps.tsv instead of asserting.
	// For the maintainer who has just changed the compiler; it weakens
	// nothing, it only reports.
	envConformanceRecord = "ABNF_CONFORMANCE_RECORD"

	conformanceFetchHint = "\n  Fetch it with:  sh test/fetch-abnf-corpus.sh   (or: make abnf-corpus)" +
		"\n  `make test-go` does this for you." +
		"\n  This test MUST NOT be skipped — a conformance run that silently does" +
		"\n  not happen is the exact defect this suite exists to prevent."
)

func repoPath(parts ...string) string {
	return filepath.Join(append([]string{".."}, parts...)...)
}

func corpusDir() string { return repoPath("test", "abnf-corpus") }

// TestMain has two jobs.
//
// One: when envConformanceFile is set this process IS the budgeted child —
// compile that one grammar, print the result as JSON, exit. No test runs.
//
// Two: otherwise, make sure the corpus is on disk before anything else. `go
// test` has no `pretest` hook (the TS side gets one from package.json) and
// the shared CI workflow that runs `go test ./...` lives in tabnas/.github,
// which this repo cannot edit — so the fetch happens here. The script is
// idempotent, so it is a no-op once the corpus is at its pinned SHAs. If the
// fetch cannot run, the conformance tests below still FAIL LOUDLY.
func TestMain(m *testing.M) {
	if file := os.Getenv(envConformanceFile); file != "" {
		conformanceChild(file, os.Getenv(envConformanceAppend))
		return
	}
	if _, err := os.Stat(corpusDir()); err != nil {
		script := repoPath("test", "fetch-abnf-corpus.sh")
		if _, err := os.Stat(script); err == nil {
			cmd := exec.Command("sh", script)
			cmd.Stdout, cmd.Stderr = os.Stderr, os.Stderr
			if err := cmd.Run(); err != nil {
				fmt.Fprintf(os.Stderr,
					"WARNING: %s failed (%v); the conformance tests will FAIL, not skip.\n",
					script, err)
			}
		}
	}
	os.Exit(m.Run())
}

type conformanceResult struct {
	OK     bool     `json:"ok"`
	Names  []string `json:"names,omitempty"`
	Error  string   `json:"error,omitempty"`
	Budget bool     `json:"-"`
}

// conformanceChild is the budgeted child: compile one grammar under a hard
// heap and wall-clock cap, print JSON, exit. Exceeding either cap exits
// non-zero with no JSON, which the parent reads as `budget`.
func conformanceChild(file, appendLine string) {
	debug.SetMemoryLimit(conformanceBudgetBytes)
	go func() {
		deadline := time.Now().Add(conformanceBudget)
		for {
			time.Sleep(50 * time.Millisecond)
			var ms runtime.MemStats
			runtime.ReadMemStats(&ms)
			if ms.HeapAlloc > conformanceBudgetBytes || time.Now().After(deadline) {
				os.Exit(3)
			}
		}
	}()

	raw, err := os.ReadFile(file)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	src := string(raw)
	if appendLine != "" {
		src = applyMutation(src, appendLine)
	}

	out := conformanceResult{}
	spec, cerr := Abnf(src, nil)
	if cerr != nil {
		out.Error = strings.SplitN(cerr.Error(), "\n", 2)[0]
	} else {
		out.OK = true
		out.Names = specNames(spec)
	}
	enc, _ := json.Marshal(out)
	os.Stdout.Write(enc)
	os.Exit(0)
}

// compileBudgeted runs one corpus compile in its own process.
func compileBudgeted(t *testing.T, rel, appendLine string) conformanceResult {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), conformanceBudget+10*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, os.Args[0])
	cmd.Env = append(os.Environ(),
		envConformanceFile+"="+filepath.Join(corpusDir(), rel),
		envConformanceAppend+"="+appendLine)
	out, err := cmd.Output()
	var res conformanceResult
	if err != nil || json.Unmarshal(out, &res) != nil {
		return conformanceResult{Budget: true}
	}
	return res
}

// Every name the compiled spec can reach: rules, fixed tokens, match tokens.
func specNames(spec *tabnas.GrammarSpec) []string {
	seen := map[string]bool{}
	add := func(k string) { seen[strings.ToLower(strings.TrimPrefix(k, "#"))] = true }
	if spec != nil {
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
	}
	out := make([]string, 0, len(seen))
	for k := range seen {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// RFC 5234 s4:  rule = rulename defined-as elements c-nl
// A rulename starts in column 1; "=" or "=/" follows.
var ruleDeclRe = regexp.MustCompile(`(?m)^([A-Za-z][A-Za-z0-9-]*)[ \t]*=/?`)

func declaredRules(src string) []string {
	seen := map[string]bool{}
	var out []string
	for _, m := range ruleDeclRe.FindAllStringSubmatch(src, -1) {
		n := strings.ToLower(m[1])
		if !seen[n] {
			seen[n] = true
			out = append(out, n)
		}
	}
	sort.Strings(out)
	return out
}

// RFC 5234 lines are CRLF-terminated.
var trailingNLRe = regexp.MustCompile(`\r?\n*$`)

func applyMutation(base, appendLine string) string {
	return trailingNLRe.ReplaceAllString(base, "") + "\r\n" + appendLine + "\r\n"
}

func loadCorpusTSV(t *testing.T, name string) [][]string {
	t.Helper()
	path := repoPath("test", "corpus", name)
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("missing %s: %v%s", path, err, conformanceFetchHint)
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

func readCorpusFile(t *testing.T, rel string) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(corpusDir(), rel))
	if err != nil {
		t.Fatalf("corpus file missing: %s: %v%s", rel, err, conformanceFetchHint)
	}
	return string(raw)
}

type conformanceCorpus struct {
	valid, invalid, fragment []string
	mutations                [][]string
	validGaps                []string
	invalidGaps              []string
	overBudget               []string
	mutationLeaks            map[string]int
	pinnedValid              []string
	pinnedInvalid            []string
	pinnedBudget             []string
	pinnedLeaks              map[string]int
	record                   bool
	recorded                 []string
}

func loadConformance(t *testing.T) *conformanceCorpus {
	t.Helper()
	c := &conformanceCorpus{
		mutationLeaks: map[string]int{},
		pinnedLeaks:   map[string]int{},
		record:        os.Getenv(envConformanceRecord) == "1",
	}
	for _, r := range loadCorpusTSV(t, "manifest.tsv") {
		switch r[1] {
		case "valid":
			c.valid = append(c.valid, r[0])
		case "invalid":
			c.invalid = append(c.invalid, r[0])
		case "fragment":
			c.fragment = append(c.fragment, r[0])
		default:
			t.Fatalf("manifest row %q has unknown class %q", r[0], r[1])
		}
	}
	c.mutations = loadCorpusTSV(t, "mutations.tsv")
	if len(c.valid) < 50 || len(c.invalid) < 10 || len(c.mutations) < 13 {
		t.Fatalf("degenerate corpus: %d valid / %d invalid / %d mutation classes%s",
			len(c.valid), len(c.invalid), len(c.mutations), conformanceFetchHint)
	}
	for _, r := range loadCorpusTSV(t, "known-gaps.tsv") {
		if r[0] != "go" {
			continue
		}
		switch r[1] {
		case "valid-not-accepted":
			c.pinnedValid = append(c.pinnedValid, r[2])
		case "invalid-accepted":
			c.pinnedInvalid = append(c.pinnedInvalid, r[2])
		case "budget-exceeded":
			c.pinnedBudget = append(c.pinnedBudget, r[2])
		case "mutation-leak":
			n, err := strconv.Atoi(r[3])
			if err != nil {
				t.Fatalf("known-gaps.tsv: mutation-leak %q has a non-numeric count %q", r[2], r[3])
			}
			c.pinnedLeaks[r[2]] = n
		default:
			t.Fatalf("known-gaps.tsv row %q has unknown kind %q", r[2], r[1])
		}
	}
	sort.Strings(c.pinnedValid)
	sort.Strings(c.pinnedInvalid)
	sort.Strings(c.pinnedBudget)
	return c
}

func (c *conformanceCorpus) note(kind, key string, count int, note string) {
	c.recorded = append(c.recorded,
		strings.Join([]string{"go", kind, key, strconv.Itoa(count), note}, "\t"))
}

// TestConformance is one test, not many, because the three halves share one
// expensive sweep and the mutation half needs to know which bases the
// compiler could not finish at all.
func TestConformance(t *testing.T) {
	c := loadConformance(t)

	if _, err := os.Stat(corpusDir()); err != nil {
		t.Fatalf("ABNF conformance corpus missing at %s%s", corpusDir(), conformanceFetchHint)
	}
	abnfFiles := 0
	_ = filepath.Walk(corpusDir(), func(p string, info os.FileInfo, err error) error {
		if err == nil && info != nil && !info.IsDir() && strings.HasSuffix(p, ".abnf") {
			abnfFiles++
		}
		return nil
	})
	if abnfFiles < 60 {
		t.Fatalf("expected >=60 .abnf corpus files, found %d%s", abnfFiles, conformanceFetchHint)
	}
	for _, r := range loadCorpusTSV(t, "manifest.tsv") {
		if _, err := os.Stat(filepath.Join(corpusDir(), r[0])); err != nil {
			t.Fatalf("manifest names a corpus file that is not there: %s%s",
				r[0], conformanceFetchHint)
		}
	}

	// --- half 1: valid grammars compile AND yield every declared rule ---
	for _, rel := range c.valid {
		res := compileBudgeted(t, rel, "")
		if res.Budget {
			c.overBudget = append(c.overBudget, rel)
			c.validGaps = append(c.validGaps, rel)
			if c.record {
				c.note("budget-exceeded", rel, 1, "compiler does not terminate within 256MB / 60s")
				c.note("valid-not-accepted", rel, 1, "budget exceeded")
			}
			continue
		}
		if !res.OK {
			c.validGaps = append(c.validGaps, rel)
			if c.record {
				c.note("valid-not-accepted", rel, 1, "rejected: "+res.Error)
			}
			continue
		}
		have := map[string]bool{}
		for _, n := range res.Names {
			have[n] = true
		}
		var missing []string
		for _, n := range declaredRules(readCorpusFile(t, rel)) {
			if !have[n] {
				missing = append(missing, n)
			}
		}
		if len(missing) > 0 {
			c.validGaps = append(c.validGaps, rel)
			if c.record {
				c.note("valid-not-accepted", rel, 1,
					"compiled, but declared rules vanished: "+strings.Join(missing, ","))
			}
		}
	}

	// --- half 2a: corpus grammars the oracle rejects must be rejected ---
	for _, rel := range c.invalid {
		if res := compileBudgeted(t, rel, ""); res.OK {
			c.invalidGaps = append(c.invalidGaps, rel)
			if c.record {
				c.note("invalid-accepted", rel, 1, "accepted; the oracle rejects it")
			}
		}
	}

	// --- half 2b: mutants violating a named RFC 5234 production ---------
	//
	// Bases the compiler cannot finish at all are excluded here and ONLY
	// here: a mutant of a base that never compiles measures nothing about the
	// mutation. That exclusion is pinned by name, so it cannot quietly grow.
	var bases []string
	for _, rel := range c.valid {
		if !slicesContains(c.overBudget, rel) {
			bases = append(bases, rel)
		}
	}
	for _, m := range c.mutations {
		name, appendLine := m[0], m[1]
		n := 0
		for _, rel := range bases {
			if _, err := Abnf(applyMutation(readCorpusFile(t, rel), appendLine), nil); err == nil {
				n++
			}
		}
		if n > 0 {
			c.mutationLeaks[name] = n
			if c.record {
				c.note("mutation-leak", name, n,
					fmt.Sprintf("%d/%d bases accepted `%s`", n, len(bases), appendLine))
			}
		}
	}

	// --- the dial: what was actually measured ---------------------------
	mutantTotal := len(bases) * len(c.mutations)
	mutantLeaks := 0
	for _, n := range c.mutationLeaks {
		mutantLeaks += n
	}
	t.Logf("\n  ABNF conformance dial (Go), as measured by this run:"+
		"\n    valid   accepted + value-correct : %d/%d"+
		"\n    invalid rejected                 : %d/%d"+
		"\n    excluded fragments               : %d"+
		"\n    over budget (counted as failures): %d",
		len(c.valid)-len(c.validGaps), len(c.valid),
		len(c.invalid)-len(c.invalidGaps)+mutantTotal-mutantLeaks,
		len(c.invalid)+mutantTotal,
		len(c.fragment), len(c.overBudget))

	if c.record {
		t.Logf("\n# paste the `go` rows of test/corpus/known-gaps.tsv:\n%s",
			strings.Join(c.recorded, "\n"))
		return
	}

	sort.Strings(c.validGaps)
	sort.Strings(c.invalidGaps)
	sort.Strings(c.overBudget)

	assertSetEqual(t, "valid RFC 5234 grammars this compiler does not fully accept",
		c.validGaps, c.pinnedValid)
	assertSetEqual(t, "grammars the compiler cannot finish within 256MB / 60s",
		c.overBudget, c.pinnedBudget)
	assertSetEqual(t, "non-RFC-5234 corpus grammars this compiler accepts",
		c.invalidGaps, c.pinnedInvalid)

	for name, got := range c.mutationLeaks {
		if want, ok := c.pinnedLeaks[name]; !ok || want != got {
			t.Errorf("mutation class %q now leaks %d bases, known-gaps.tsv pins %d (present=%v). "+
				"Lower is better; update test/corpus/known-gaps.tsv when you improve one.",
				name, got, want, ok)
		}
	}
	for name, want := range c.pinnedLeaks {
		if _, ok := c.mutationLeaks[name]; !ok {
			t.Errorf("mutation class %q no longer leaks (known-gaps.tsv pins %d). "+
				"If you fixed it, delete its row.", name, want)
		}
	}
}

func assertSetEqual(t *testing.T, what string, got, want []string) {
	t.Helper()
	if strings.Join(got, "\n") == strings.Join(want, "\n") {
		return
	}
	t.Errorf("the set of %s has changed.\n  measured now: %v\n  known-gaps.tsv: %v\n"+
		"  If you FIXED one, delete its row from test/corpus/known-gaps.tsv. "+
		"If you BROKE one, that is a regression. Never edit a row to silence a "+
		"failure you did not fix.", what, got, want)
}

func slicesContains(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}
