// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// parity_test.go — cross-runtime conformance, driven by the shared
// `test/spec/*.tsv` fixtures at the repo root (see ../test/AGENTS.md).
//
// The fixture loader, the escape codec, the ERROR: contract and the row
// loop all come from github.com/tabnas/support/go, whose TypeScript half
// ts/test/parity.test.js uses to run the SAME files — so the two
// implementations cannot drift without one of them going red, and neither
// can the two loaders. That is the check the repo previously lacked:
// leftrec_test.go and rfc3986_test.go mirror the TS suite by hand, which
// catches nothing when only one side changes.
//
// What is left here is only what is specific to abnf: four fixtures, each
// asserting a different thing about the same `grammar` column.

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
)

// specFile runs one fixture. Every fixture's first column is an ABNF
// grammar, which is multi-line, so it always needs escape-decoding; where
// the runner's own input column is something else, the grammar is read
// explicitly with UnescNamed.
func specFile(t *testing.T, name string, r support.Runner) {
	t.Helper()

	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}

	if "" == r.InputName {
		r.InputName = "grammar"
	}
	r.ExpectedName = "expected"
	r.CaseName = func(row *support.Row, _ string) string {
		return fmt.Sprintf("row %d: %s", row.Line, specLabel(row.UnescNamed("grammar")))
	}

	r.File(t, filepath.Join(dir, name+".tsv"))
}

// specLabel is the grammar, truncated, so a failure names its case readably.
func specLabel(g string) string {
	one := strings.ReplaceAll(g, "\n", " ; ")
	if len(one) > 60 {
		return one[:57] + "..."
	}
	return one
}

// TestSpecAbnfAST: the grammar parses the input into the expected AST.
func TestSpecAbnfAST(t *testing.T) {
	specFile(t, "alignment-abnf-ast", support.Runner{
		InputName: "input",
		ParseRow: func(input string, row *support.Row) (any, error) {
			spec, err := Abnf(row.UnescNamed("grammar"), nil)
			if err != nil {
				return nil, err
			}

			rh := 4096
			j := tabnas.Make(tabnas.Options{
				Rewind: &tabnas.RewindOptions{History: &rh},
			})
			if err := j.Grammar(spec); err != nil {
				return nil, err
			}
			return j.Parse(input)
		},
		Normalize: jsonFlatten,
	})
}

// TestSpecAbnfTokens: the grammar declares the expected fixed tokens.
func TestSpecAbnfTokens(t *testing.T) {
	specFile(t, "alignment-abnf-tokens", support.Runner{
		Parse: func(grammar string) (any, error) {
			spec, err := Abnf(grammar, nil)
			if err != nil {
				return nil, err
			}

			fixed := map[string]any{}
			if nil != spec.Options && nil != spec.Options.Fixed {
				for k, v := range spec.Options.Fixed.Token {
					if nil != v {
						fixed[k] = *v
					}
				}
			}
			return fixed, nil
		},
	})
}

// TestSpecAbnfRules: the grammar declares the expected rules, by name.
func TestSpecAbnfRules(t *testing.T) {
	specFile(t, "alignment-abnf-rules", support.Runner{
		Parse: func(grammar string) (any, error) {
			spec, err := Abnf(grammar, nil)
			if err != nil {
				return nil, err
			}

			rules := []any{}
			for name := range spec.Rule {
				rules = append(rules, name)
			}
			sort.Slice(rules, func(i, j int) bool {
				return rules[i].(string) < rules[j].(string)
			})
			return rules, nil
		},
	})
}

// TestSpecAbnfErrors: the grammar is rejected, with exactly this message.
func TestSpecAbnfErrors(t *testing.T) {
	specFile(t, "alignment-abnf-errors", support.Runner{
		Parse: func(grammar string) (any, error) {
			return Abnf(grammar, nil)
		},

		// abnf's ERROR: cells hold the whole MESSAGE, compared EXACTLY —
		// not a code, and not a substring. These rejections are the
		// converter's own diagnostics, several of them paragraphs that name
		// the offending rule and say what to write instead, and the wording
		// is the thing under test: a diagnostic that stops explaining
		// itself is the regression worth catching.
		MatchError: func(err error, want string, _ *support.Row) bool {
			return err.Error() == want
		},
	})
}

// jsonFlatten renders a value as JSON and reads it back as plain
// map/slice/float64/string/bool/nil. A value that will not marshal is
// returned as it is: the comparison then fails and prints it, which says
// more than a panic here would.
func jsonFlatten(v any) any {
	raw, err := json.Marshal(v)
	if err != nil {
		return v
	}
	var out any
	if err := json.Unmarshal(raw, &out); err != nil {
		return v
	}
	return out
}
