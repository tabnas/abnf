// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

// tree_test.go — the tree a host translates, ParseAbnfTree: an ABNF
// document read as the schema grammar-rules. test/spec/tree.tsv holds each
// grammar to its tree's JSON text, member order included, as the
// TypeScript and Rust runners (ts/test/tree.test.js, rs/tests/tree_test.rs)
// hold theirs; the order is part of the contract, since a host streams the
// members in that order and the part makes the same events from the rule
// passes of Make().
//
// Repetition is replacement (AGENTS.md): the rules of a grammar, the
// alternatives of a rule or a group and the elements of an alternative
// add no rule depth however many there are.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	bnf "github.com/tabnas/bnf/go"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
)

// treeJSON is a tree's JSON text, as the TypeScript and Rust runners
// write it: its members in order, and no character escaped that JSON
// does not need escaped.
func treeJSON(v any) (string, error) {
	var buf bytes.Buffer
	if err := writeTreeJSON(&buf, v); err != nil {
		return "", err
	}
	return buf.String(), nil
}

func writeTreeJSON(buf *bytes.Buffer, v any) error {
	switch x := v.(type) {
	case *tabnas.OrderedMap:
		buf.WriteByte('{')
		for i, k := range x.Keys {
			if i > 0 {
				buf.WriteByte(',')
			}
			if err := writeTreeJSON(buf, k); err != nil {
				return err
			}
			buf.WriteByte(':')
			if err := writeTreeJSON(buf, x.Vals[k]); err != nil {
				return err
			}
		}
		buf.WriteByte('}')
	case []any:
		buf.WriteByte('[')
		for i, item := range x {
			if i > 0 {
				buf.WriteByte(',')
			}
			if err := writeTreeJSON(buf, item); err != nil {
				return err
			}
		}
		buf.WriteByte(']')
	default:
		var one bytes.Buffer
		enc := json.NewEncoder(&one)
		enc.SetEscapeHTML(false)
		if err := enc.Encode(x); err != nil {
			return err
		}
		buf.Write(bytes.TrimSuffix(one.Bytes(), []byte("\n")))
	}
	return nil
}

// treeCode is the engine's code an error from ParseAbnfTree carries.
func treeCode(err error) string {
	var te *tabnas.TabnasError
	if errors.As(err, &te) {
		return te.Code
	}
	return ""
}

func TestSpecTree(t *testing.T) {
	specFile(t, "tree", support.Runner{
		Parse: func(grammar string) (any, error) {
			tree, err := ParseAbnfTree(grammar)
			if err != nil {
				return nil, err
			}
			if err := bnf.CheckRules(tree); err != nil {
				return nil, fmt.Errorf("not a grammar-rules tree: %w", err)
			}
			return treeJSON(tree)
		},
		// The cell is the tree's JSON text, compared as text.
		ParseExpected: func(expected string, _ *support.Row) (any, error) {
			return expected, nil
		},
		// An ERROR:<code> cell holds the code the parse refused with.
		MatchError: func(err error, want string, _ *support.Row) bool {
			return treeCode(err) == want
		},
	})
}

// Every grammar of the fixtures and of the TypeScript suite's grammars
// that the tree parser reads is a tree of the schema.
func TestTreesAreGrammarRules(t *testing.T) {
	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}
	var sources []string
	files, _ := filepath.Glob(filepath.Join(dir, "*.tsv"))
	for _, file := range files {
		raw, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		for _, line := range strings.Split(string(raw), "\n")[1:] {
			if "" == line || strings.HasPrefix(line, "#") {
				continue
			}
			sources = append(sources, support.Unescape(strings.SplitN(line, "\t", 2)[0]))
		}
	}
	grammars, _ := filepath.Glob(filepath.Join(dir, "..", "..", "ts", "test", "grammar", "*.abnf"))
	for _, file := range grammars {
		raw, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		sources = append(sources, string(raw))
	}
	read := 0
	for _, src := range sources {
		tree, err := ParseAbnfTree(src)
		if err != nil {
			continue
		}
		if err := bnf.CheckRules(tree); err != nil {
			t.Errorf("%q: %v", src, err)
		}
		read++
	}
	if read <= 100 {
		t.Fatalf("%d trees read", read)
	}
}

// The tree parser's actions are under the names the IR parser's take, so
// AbnfRules installs either, and the rule passes are the meta-grammar's.
func TestTreeActionsAreTheIRsNames(t *testing.T) {
	names := func(ref map[tabnas.FuncRef]any) []string {
		out := []string{}
		for name := range ref {
			out = append(out, string(name))
		}
		sort.Strings(out)
		return out
	}
	if got, want := strings.Join(names(treeRef()), " "), strings.Join(names(abnfParseRef()), " "); got != want {
		t.Fatalf("tree actions\n  %s\nIR actions\n  %s", got, want)
	}
}

func TestMakeIsFresh(t *testing.T) {
	a, b := Make(), Make()
	if a == b {
		t.Fatal("Make returned one parser twice")
	}
	got, err := a.Parse("x = \"y\"\n")
	if err != nil {
		t.Fatal(err)
	}
	want, _ := ParseAbnfTree("x = \"y\"\n")
	gotText, _ := treeJSON(got)
	wantText, _ := treeJSON(want)
	if gotText != wantText {
		t.Fatalf("Make's value %s, the tree %s", gotText, wantText)
	}
}

// treeDepth is the deepest rule a parse of src reaches.
func treeDepth(t *testing.T, src string) int {
	t.Helper()
	j := Make()
	deepest := 0
	j.SubRuleDone(func(rule *tabnas.Rule, _ *tabnas.Context, done tabnas.RuleDone) {
		if !done.Probe && rule.D > deepest {
			deepest = rule.D
		}
	})
	if _, err := j.Parse(src); err != nil {
		t.Fatal(err)
	}
	return deepest
}

func TestTreeRepetitionAddsNoDepth(t *testing.T) {
	const n = 10000
	join := func(n int, item func(i int) string, sep string) string {
		parts := make([]string, n)
		for i := range parts {
			parts[i] = item(i)
		}
		return strings.Join(parts, sep)
	}
	repeated := []struct {
		what string
		doc  func(n int) string
	}{
		{"rules of a grammar", func(n int) string {
			return join(n, func(i int) string { return fmt.Sprintf("r%d = \"x\"\n", i) }, "")
		}},
		{"alternatives of a rule", func(n int) string {
			return "a = " + join(n, func(i int) string { return fmt.Sprintf("r%d", i) }, " / ") + "\n"
		}},
		{"elements of an alternative", func(n int) string {
			return "a = " + join(n, func(i int) string { return fmt.Sprintf("*r%d", i) }, " ") + "\n"
		}},
		{"alternatives of a group", func(n int) string {
			return "a = ( " + join(n, func(i int) string { return fmt.Sprintf("%%x%x", i) }, " / ") + " )\n"
		}},
		{"elements of an option", func(n int) string {
			return "a = [ " + strings.Repeat("\"x\" ", n) + "]\n"
		}},
	}
	for _, c := range repeated {
		// Two items, since a list's first item can sit apart from its
		// repeated ones: the loop's own item is the second.
		if got, want := treeDepth(t, c.doc(n)), treeDepth(t, c.doc(2)); got != want {
			t.Errorf("the %s: %d reach depth %d, two reach %d", c.what, n, got, want)
		}
	}
}
