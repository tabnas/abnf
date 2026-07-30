// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// parity_test.go — cross-runtime conformance, driven by the shared
// `test/spec/*.tsv` fixtures at the repo root (see ../test/AGENTS.md), the
// same convention @tabnas/parser uses.
//
// ts/test/parity.test.js runs the SAME files, so the two implementations
// cannot drift without one of them going red. That is the check the repo
// previously lacked: leftrec_test.go and rfc3986_test.go mirror the TS suite
// by hand, which catches nothing when only one side changes.

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
)

type specRow struct {
	cols   []string
	lineNo int
}

// specUnescape mirrors the loader in @tabnas/parser's ts/test/utility.js.
// ABNF grammars are multi-line, so the `grammar` column relies on `\n`.
func specUnescape(s string) string {
	s = strings.ReplaceAll(s, `\r\n`, "\r\n")
	s = strings.ReplaceAll(s, `\n`, "\n")
	s = strings.ReplaceAll(s, `\r`, "\r")
	return s
}

func loadSpecTSV(t *testing.T, name string) []specRow {
	t.Helper()
	path := filepath.Join("..", "test", "spec", name+".tsv")
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("spec file not found: %s: %v", path, err)
	}
	defer f.Close()

	var rows []specRow
	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	lineNo := 0
	for scanner.Scan() {
		lineNo++
		if lineNo == 1 {
			continue // header
		}
		line := scanner.Text()
		if line == "" {
			continue
		}
		cols := strings.Split(line, "\t")
		for i := range cols {
			cols[i] = specUnescape(cols[i])
		}
		rows = append(rows, specRow{cols: cols, lineNo: lineNo})
	}
	if err := scanner.Err(); err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	if len(rows) == 0 {
		t.Fatalf("%s has no cases", path)
	}
	return rows
}

// specLabel is the grammar, truncated, so a failure names its case readably.
func specLabel(g string) string {
	one := strings.ReplaceAll(g, "\n", " ; ")
	if len(one) > 60 {
		return one[:57] + "..."
	}
	return one
}

// jsonRound normalises through JSON so Go's map[string]any and the fixture's
// decoded shape compare structurally.
func jsonRound(t *testing.T, v any) any {
	t.Helper()
	raw, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var out any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return out
}

func TestSpecAbnfAST(t *testing.T) {
	for _, row := range loadSpecTSV(t, "alignment-abnf-ast") {
		grammar, input, expected := row.cols[0], row.cols[1], row.cols[2]
		t.Run(specLabel(grammar), func(t *testing.T) {
			spec, err := Abnf(grammar, nil)
			if err != nil {
				t.Fatalf("Abnf: %v", err)
			}
			rh := 4096
			j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
			if err := j.Grammar(spec); err != nil {
				t.Fatalf("Grammar: %v", err)
			}
			got, err := j.Parse(input)
			if err != nil {
				t.Fatalf("Parse(%q): %v", input, err)
			}
			var want any
			if err := json.Unmarshal([]byte(expected), &want); err != nil {
				t.Fatalf("bad expected JSON on line %d: %v", row.lineNo, err)
			}
			if !reflect.DeepEqual(jsonRound(t, got), want) {
				t.Errorf("parse %q:\n  got  %v\n  want %v", input, jsonRound(t, got), want)
			}
		})
	}
}

func TestSpecAbnfTokens(t *testing.T) {
	for _, row := range loadSpecTSV(t, "alignment-abnf-tokens") {
		grammar, expected := row.cols[0], row.cols[1]
		t.Run(specLabel(grammar), func(t *testing.T) {
			spec, err := Abnf(grammar, nil)
			if err != nil {
				t.Fatalf("Abnf: %v", err)
			}
			fixed := map[string]string{}
			if spec.Options != nil && spec.Options.Fixed != nil {
				for k, v := range spec.Options.Fixed.Token {
					if v != nil {
						fixed[k] = *v
					}
				}
			}
			want := map[string]string{}
			if err := json.Unmarshal([]byte(expected), &want); err != nil {
				t.Fatalf("bad expected JSON on line %d: %v", row.lineNo, err)
			}
			if !reflect.DeepEqual(fixed, want) {
				t.Errorf("fixed tokens:\n  got  %v\n  want %v", fixed, want)
			}
		})
	}
}

func TestSpecAbnfRules(t *testing.T) {
	for _, row := range loadSpecTSV(t, "alignment-abnf-rules") {
		grammar, expected := row.cols[0], row.cols[1]
		t.Run(specLabel(grammar), func(t *testing.T) {
			spec, err := Abnf(grammar, nil)
			if err != nil {
				t.Fatalf("Abnf: %v", err)
			}
			rules := []string{}
			for name := range spec.Rule {
				rules = append(rules, name)
			}
			sort.Strings(rules)
			var want []string
			if err := json.Unmarshal([]byte(expected), &want); err != nil {
				t.Fatalf("bad expected JSON on line %d: %v", row.lineNo, err)
			}
			if !reflect.DeepEqual(rules, want) {
				t.Errorf("rule names:\n  got  %v\n  want %v", rules, want)
			}
		})
	}
}

func TestSpecAbnfErrors(t *testing.T) {
	for _, row := range loadSpecTSV(t, "alignment-abnf-errors") {
		grammar, expected := row.cols[0], row.cols[1]
		t.Run(specLabel(grammar), func(t *testing.T) {
			if !strings.HasPrefix(expected, "ERROR:") {
				t.Fatalf("expected column must be ERROR:… (line %d)", row.lineNo)
			}
			want := strings.TrimPrefix(expected, "ERROR:")
			_, err := Abnf(grammar, nil)
			if err == nil {
				t.Fatalf("expected error %q, got none", want)
			}
			if err.Error() != want {
				t.Errorf("message:\n  got  %q\n  want %q", err.Error(), want)
			}
		})
	}
}
