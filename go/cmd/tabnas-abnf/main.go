// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

// Command tabnas-abnf converts an ABNF grammar into a tabnas grammar
// spec (JSON), a pure-data recognition/AST grammar (jsonic text), lists
// the per-alt action marks, or parses sample inputs against the grammar.
//
// It is the Go port of ts/src/bin/tabnas-abnf-cli.ts.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"

	abnf "github.com/tabnas/abnf/go"
	tabnas "github.com/tabnas/parser/go"
)

type cliArgs struct {
	help       bool
	stdin      bool
	files      []string
	inline     []string
	start      string
	tag        string
	space      int
	parse      []string
	parseFiles []string
	compile    bool
	full       bool
	marks      bool
}

func main() {
	os.Exit(run(os.Args, os.Stdin, os.Stdout, os.Stderr))
}

func run(argv []string, stdin io.Reader, stdout, stderr io.Writer) int {
	args := cliArgs{space: 2}

	for i := 1; i < len(argv); i++ {
		arg := argv[i]
		switch {
		case arg == "-":
			args.stdin = true
		case arg == "--help" || arg == "-h":
			args.help = true
		case arg == "--file" || arg == "-f":
			i++
			if i < len(argv) {
				args.files = append(args.files, argv[i])
			}
		case arg == "--start" || arg == "-s":
			i++
			if i < len(argv) {
				args.start = argv[i]
			}
		case arg == "--tag" || arg == "-t":
			i++
			if i < len(argv) {
				args.tag = argv[i]
			}
		case arg == "--compact" || arg == "-c":
			args.space = 0
		case arg == "--compile" || arg == "-C":
			args.compile = true
		case arg == "--full" || arg == "-F":
			args.compile = true
			args.full = true
		case arg == "--marks" || arg == "-m":
			args.marks = true
		case arg == "--parse" || arg == "-P":
			i++
			if i < len(argv) {
				args.parse = append(args.parse, argv[i])
			}
		case arg == "--parse-file":
			i++
			if i < len(argv) {
				args.parseFiles = append(args.parseFiles, argv[i])
			}
		case arg != "" && arg[0] != '-':
			args.inline = append(args.inline, arg)
		}
	}

	if args.help {
		printHelp(stdout)
		return 0
	}

	var src string
	for _, fp := range args.files {
		if fp != "" {
			b, err := os.ReadFile(fp)
			if err != nil {
				fmt.Fprintln(stderr, err)
				return 1
			}
			src += string(b) + "\n"
		}
	}
	for _, inline := range args.inline {
		src += inline + "\n"
	}
	if trim(src) == "" || args.stdin {
		b, _ := io.ReadAll(stdin)
		src += string(b)
	}

	convOpts := &abnf.AbnfConvertOptions{Start: args.start, Tag: args.tag}

	// Marks mode.
	if args.marks {
		spec, err := abnf.Abnf(src, &abnf.AbnfConvertOptions{
			Start: args.start, Tag: args.tag, Marks: true,
		})
		if err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		fmt.Fprintln(stdout, abnf.MarkListing(spec))
		return 0
	}

	// Compilation mode.
	if args.compile {
		indent := args.space
		if indent == 0 {
			indent = 2
		}
		out, err := abnf.AbnfCompile(src, &abnf.AbnfCompileOptions{
			Start: args.start, Tag: args.tag, Indent: indent, Recognition: boolp(!args.full),
		})
		if err != nil {
			if _, ok := err.(*abnf.AbnfCompileError); ok {
				fmt.Fprintln(stderr, err.Error())
				return 1
			}
			fmt.Fprintln(stderr, err)
			return 1
		}
		fmt.Fprintln(stdout, out)
		return 0
	}

	spec, err := abnf.Abnf(src, convOpts)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}

	// Parse mode.
	if len(args.parse) > 0 || len(args.parseFiles) > 0 {
		type sample struct{ label, input string }
		samples := []sample{}
		for _, fp := range args.parseFiles {
			b, e := os.ReadFile(fp)
			if e != nil {
				fmt.Fprintln(stderr, e)
				return 1
			}
			samples = append(samples, sample{fp, string(b)})
		}
		for _, inp := range args.parse {
			samples = append(samples, sample{inp, inp})
		}

		rh := 4096
		j := tabnas.Make(tabnas.Options{Rewind: &tabnas.RewindOptions{History: &rh}})
		if err := j.Grammar(spec); err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		failed := 0
		for _, s := range samples {
			tree, perr := j.Parse(s.input)
			if perr != nil {
				failed++
				fmt.Fprintf(stderr, "fail: %s: %s\n", jsonStr(s.label), firstLine(perr.Error()))
				continue
			}
			fmt.Fprintf(stdout, "ok: %s -> %s\n", jsonStr(s.label), marshal(tree, args.space))
		}
		if failed > 0 {
			return 1
		}
		return 0
	}

	indent := args.space
	if indent == 0 {
		indent = 2
	}
	fmt.Fprintln(stdout, abnf.SpecToJSON(spec, indent))
	return 0
}

func boolp(b bool) *bool { return &b }

func trim(s string) string {
	start, end := 0, len(s)
	for start < end && isSpace(s[start]) {
		start++
	}
	for end > start && isSpace(s[end-1]) {
		end--
	}
	return s[start:end]
}

func isSpace(b byte) bool { return b == ' ' || b == '\t' || b == '\n' || b == '\r' }

func jsonStr(s string) string {
	b, _ := json.Marshal(s)
	return string(b)
}

func marshal(v any, space int) string {
	var b []byte
	if space > 0 {
		b, _ = json.MarshalIndent(v, "", spaces(space))
	} else {
		b, _ = json.Marshal(v)
	}
	return string(b)
}

func spaces(n int) string {
	out := make([]byte, n)
	for i := range out {
		out[i] = ' '
	}
	return string(out)
}

func firstLine(s string) string {
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			return s[:i]
		}
	}
	return s
}

func printHelp(w io.Writer) {
	fmt.Fprint(w, `
tabnas-abnf: convert an ABNF grammar into a tabnas grammar spec.

Usage: tabnas-abnf <args> [<abnf-source>]*

Arguments:
  -                      Read ABNF source from stdin.
  --file <path>          Read ABNF source from <path> (repeatable).
  -f <path>

  --start <name>         Set the start rule (defaults to the first
  -s <name>                production).

  --tag <name>           Group tag applied to every emitted alt.
  -t <name>                Defaults to `+"`abnf`"+`.

  --compact              Emit single-line JSON (default indent is 2).
  -c

  --compile              Compilation mode: emit a pure-data *recognition*
  -C                       grammar as jsonic text (no closures; control
                           and tree building reference engine
                           `+"`$`"+`-builtins).

  --full                 With compilation mode, emit the full AST
  -F                       grammar (tree `+"`$`"+`-builtins retained) instead
                           of recognition-only. Still pure data.

  --marks                List the per-alt marks the compiler assigns,
  -m                       usable as `+"`@<rule>:o|c:<mark>`"+` action refs.

  --parse <input>        Parse <input> against the generated grammar
  -P <input>               and print its parse tree. Repeatable.
                           Exits non-zero if any sample fails.

  --parse-file <path>    Parse the contents of <path> against the
                           generated grammar (repeatable).

  --help                 Print this help message.
  -h

Grammar dialect:
  Rules are ABNF-style: `+"`name = element ...`"+`, with `+"`/`"+` for choice and
  double-quoted literals. For example: `+"`greet = \"hi\" / \"hello\"`"+`.

Examples:
  > tabnas-abnf 'greet = "hi" / "hello"'
  > tabnas-abnf -f grammar.abnf
  > echo 'g = "a"' | tabnas-abnf -
  > tabnas-abnf -f grammar.abnf --parse 'hi'
`)
}
