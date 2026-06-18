package main

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func runCLI(t *testing.T, argv []string, stdin string) (out, errOut string, code int) {
	t.Helper()
	var so, se bytes.Buffer
	code = run(append([]string{"tabnas-bnf"}, argv...), strings.NewReader(stdin), &so, &se)
	return so.String(), se.String(), code
}

func TestCLIInlineSpec(t *testing.T) {
	out, _, code := runCLI(t, []string{`g = "x"`}, "")
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	var spec map[string]any
	if err := json.Unmarshal([]byte(out), &spec); err != nil {
		t.Fatalf("output not JSON: %v\n%s", err, out)
	}
	rules := spec["rule"].(map[string]any)
	start := rules["__start__"].(map[string]any)
	open := start["open"].([]any)
	first := open[0].(map[string]any)
	if first["p"] != "g" {
		t.Errorf("__start__ should push 'g', got %v", first["p"])
	}
}

func TestCLIStart(t *testing.T) {
	out, _, code := runCLI(t, []string{"--start", "b", `a = "x"`, `b = "y"`}, "")
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	var spec map[string]any
	json.Unmarshal([]byte(out), &spec)
	open := spec["rule"].(map[string]any)["__start__"].(map[string]any)["open"].([]any)
	if open[0].(map[string]any)["p"] != "b" {
		t.Errorf("expected start to push 'b'")
	}
}

func TestCLIStdin(t *testing.T) {
	out, _, code := runCLI(t, []string{"-"}, `g = "a"`)
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(out, `"__start__"`) {
		t.Errorf("expected spec output, got:\n%s", out)
	}
}

func TestCLIParse(t *testing.T) {
	out, _, code := runCLI(t, []string{`greet = "hi" / "hello"`, "--parse", "hi"}, "")
	if code != 0 {
		t.Fatalf("exit %d, stdout=%s", code, out)
	}
	if !strings.Contains(out, "ok:") || !strings.Contains(out, `"greet"`) {
		t.Errorf("expected ok parse line, got:\n%s", out)
	}
}

func TestCLIParseFail(t *testing.T) {
	_, errOut, code := runCLI(t, []string{`greet = "hi"`, "--parse", "nope"}, "")
	if code == 0 {
		t.Errorf("expected non-zero exit for failed parse")
	}
	if !strings.Contains(errOut, "fail:") {
		t.Errorf("expected fail line in stderr, got:\n%s", errOut)
	}
}

func TestCLICompile(t *testing.T) {
	out, _, code := runCLI(t, []string{"-C", `greet = "hi"`}, "")
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(out, "open:") {
		t.Errorf("expected relaxed jsonic 'open:' key:\n%s", out)
	}
	if !strings.Contains(out, `'@~/^hi/i'`) {
		t.Errorf("expected eager regex token:\n%s", out)
	}
}

func TestCLIMarks(t *testing.T) {
	out, _, code := runCLI(t, []string{"-m", `op = "inc" / "dec"`}, "")
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(out, "o:INC") || !strings.Contains(out, "o:DEC") {
		t.Errorf("expected mark listing, got:\n%s", out)
	}
}

func TestCLIHelp(t *testing.T) {
	out, _, code := runCLI(t, []string{"-h"}, "")
	if code != 0 {
		t.Fatalf("exit %d", code)
	}
	if !strings.Contains(out, "tabnas-bnf") || !strings.Contains(out, "--compile") {
		t.Errorf("expected help text, got:\n%s", out)
	}
}
