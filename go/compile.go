// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package abnf

// compile.go — compilation mode: turn ABNF source into a *pure-data*
// tabnas grammar and serialise it as jsonic text. The Go port of
// ts/src/compile.ts.
//
// A GrammarSpec emitted with builtins:true carries only string refs
// (`@…$` builtins) — no closures. toRecognitionSpec drops the
// tree-building builtins (recognition is structural); toPureSpec keeps
// them (still pure data). Both refuse a spec that still needs control
// closures.

import (
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"

	tabnas "github.com/tabnas/parser/go"
)

// BnfCompileError is raised when a grammar can't be compiled to a
// pure-data spec.
type BnfCompileError struct {
	Message string
	Rules   []string
}

func (e *BnfCompileError) Error() string { return e.Message }

// Ref fields whose string value is an AST-building action dropped by
// recognition mode.
var refFields = map[string]bool{"a": true, "bo": true, "bc": true}

// Tree-building builtins dropped by recognition mode.
var treeBuiltins = map[string]bool{"@node$": true, "@capture$": true, "@bubble$": true}
var treeConfigKeys = []string{"node$", "capture$"}

// BnfCompileOptions controls compilation. Mirrors BnfCompileOptions.
type BnfCompileOptions struct {
	Start       string
	Tag         string
	Strict      bool
	Indent      int
	Recognition *bool // default true
}

// specToData converts a typed *GrammarSpec (built with builtins:true,
// so all action/cond fields are `@…$` strings) into a generic nested
// data tree: map[string]any / []any / scalars / *regexp.Regexp. This is
// the canonical form the clone/serialise functions operate on.
//
// Returns the offending rule names if any control closures remain (i.e.
// a probe dispatcher compiled without builtins) — those have non-string
// A/C fields.
func specToData(spec *tabnas.GrammarSpec) (map[string]any, []string, error) {
	offenders := map[string]bool{}

	// options block: only fixed.token, match.token, rule.start are
	// emitted by the converter.
	options := map[string]any{}
	if spec.Options != nil {
		opt := spec.Options
		if opt.Fixed != nil && opt.Fixed.Token != nil {
			ft := map[string]any{}
			for name, srcPtr := range opt.Fixed.Token {
				if srcPtr != nil {
					ft[name] = *srcPtr
				}
			}
			options["fixed"] = map[string]any{"token": ft}
		}
		if opt.Match != nil && opt.Match.Token != nil {
			mt := map[string]any{}
			for name, re := range opt.Match.Token {
				eager := opt.Match.TokenEager != nil && opt.Match.TokenEager[name]
				mt[name] = regexHolder{re: re, eager: eager}
			}
			options["match"] = map[string]any{"token": mt}
		}
		if opt.Rule != nil && opt.Rule.Start != "" {
			options["rule"] = map[string]any{"start": opt.Rule.Start}
		}
	}

	rules := map[string]any{}
	for name, rspec := range spec.Rule {
		rm := map[string]any{}
		if rspec.Open != nil {
			rm["open"] = altsToData(rspec.Open, name, offenders)
		}
		if rspec.Close != nil {
			rm["close"] = altsToData(rspec.Close, name, offenders)
		}
		rules[name] = rm
	}

	data := map[string]any{"options": options, "rule": rules}
	if len(offenders) > 0 {
		off := make([]string, 0, len(offenders))
		for r := range offenders {
			off = append(off, r)
		}
		sort.Strings(off)
		return data, off, nil
	}
	return data, nil, nil
}

// regexHolder wraps a regexp with its eager flag for serialisation.
type regexHolder struct {
	re    *regexp.Regexp
	eager bool
}

// SpecToData converts a *GrammarSpec into a plain data tree
// (map/slice/scalar/regexHolder). Action/condition refs are emitted as
// their `@`-name strings. Closures in spec.Ref are dropped — like the TS
// CLI's JSON output, which serialises actions as FuncRef strings. Used by
// the CLI's default (spec-dump) mode.
func SpecToData(spec *tabnas.GrammarSpec) map[string]any {
	data, _, _ := specToData(spec)
	if spec.Ref != nil && len(spec.Ref) > 0 {
		// List the ref names (closures can't serialise) for parity with
		// the TS shape where `ref` maps names to functions.
		refs := map[string]any{}
		for name := range spec.Ref {
			refs[name] = "@fn"
		}
		data["ref"] = refs
	}
	return data
}

// SpecToJSON renders a spec as JSON text (the CLI default output).
func SpecToJSON(spec *tabnas.GrammarSpec, indent int) string {
	return ToJsonic(SpecToData(spec), true, indent)
}

func altsToData(alts any, rule string, offenders map[string]bool) []any {
	gas, ok := alts.([]*tabnas.GrammarAltSpec)
	if !ok {
		return []any{}
	}
	out := make([]any, 0, len(gas))
	for _, ga := range gas {
		out = append(out, altToData(ga, rule, offenders))
	}
	return out
}

func altToData(ga *tabnas.GrammarAltSpec, rule string, offenders map[string]bool) map[string]any {
	m := map[string]any{}
	if ga.S != nil {
		m["s"] = ga.S
	}
	if ga.B != nil {
		switch n := ga.B.(type) {
		case int:
			m["b"] = n
		case float64:
			m["b"] = int(n)
		}
	}
	if ga.P != "" {
		m["p"] = ga.P
	}
	if ga.R != "" {
		m["r"] = ga.R
	}
	if ga.A != nil {
		// In builtins mode A is a string ref (or []string). Non-string =
		// a closure offender.
		switch a := ga.A.(type) {
		case string:
			m["a"] = a
		case []any:
			m["a"] = a
		case []string:
			arr := make([]any, len(a))
			for i, s := range a {
				arr[i] = s
			}
			m["a"] = arr
		default:
			offenders[rule] = true
		}
	}
	if ga.C != nil {
		switch c := ga.C.(type) {
		case string:
			// A non-ref-field string ref signals a control function.
			m["c"] = c
		case map[string]any:
			m["c"] = c
		default:
			offenders[rule] = true
		}
	}
	if ga.K != nil {
		m["k"] = copyAnyMap(ga.K)
	}
	if ga.N != nil {
		nm := map[string]any{}
		for k, v := range ga.N {
			nm[k] = v
		}
		m["n"] = nm
	}
	if ga.U != nil {
		// Recover the mark stashed under "m$" as a top-level "m".
		um := copyAnyMap(ga.U)
		if mv, ok := um["m$"]; ok {
			m["m"] = mv
			delete(um, "m$")
		}
		if len(um) > 0 {
			m["u"] = um
		}
	}
	if ga.G != "" {
		m["g"] = ga.G
	}
	return m
}

func copyAnyMap(m map[string]any) map[string]any {
	out := make(map[string]any, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

// controlRefRules finds rules whose alts carry a control ref (a `c:`
// string ref that is NOT a tree/AST builtin) — those can't be expressed
// purely structurally without the probe builtins. In builtins mode the
// probe phase guards are `@probePhase0$` etc., which ARE valid pure-data
// control refs (engine builtins) and so are NOT offenders. Closure-mode
// probe specs have non-string C fields, caught as offenders in altToData.
//
// The TS controlRefRules flags refs found in non-REF_FIELDS positions
// that point into spec.ref (closures). Since our pure-data spec has an
// empty ref map, only closure-mode specs (with typed func fields) offend,
// and those are already collected during specToData.

// ToRecognitionSpec strips a converted spec to a function-free
// recognition grammar (drops AST building). Returns the serialisable
// data tree. Errors if control closures remain.
func toRecognitionData(spec *tabnas.GrammarSpec) (map[string]any, error) {
	if len(spec.Ref) > 0 {
		// Closures present -> not pure data. Identify offenders below.
	}
	data, offenders, err := specToData(spec)
	if err != nil {
		return nil, err
	}
	if len(offenders) > 0 {
		return nil, &BnfCompileError{
			Message: "bnf: grammar needs control functions (probe / unbounded " +
				"lookahead) and cannot be emitted as a pure recognition grammar; " +
				"recompile with `builtins: true`. Offending rule(s): " +
				strings.Join(offenders, ", "),
			Rules: offenders,
		}
	}
	out := cloneRecognition(data)
	out["v"] = tabnas.BUILTIN_SCHEMA_VERSION
	return out, nil
}

// toPureData keeps the AST-building builtins. Requires builtins-mode
// conversion (no closures).
func toPureData(spec *tabnas.GrammarSpec) (map[string]any, error) {
	if len(spec.Ref) > 0 {
		keys := make([]string, 0, len(spec.Ref))
		for k := range spec.Ref {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		if len(keys) > 3 {
			keys = keys[:3]
		}
		return nil, &BnfCompileError{
			Message: "bnf: spec still contains closures; convert with `builtins: true` " +
				"for pure-data output. Stray ref(s): " + strings.Join(keys, ", "),
		}
	}
	data, offenders, err := specToData(spec)
	if err != nil {
		return nil, err
	}
	if len(offenders) > 0 {
		return nil, &BnfCompileError{
			Message: "bnf: spec still contains closures; convert with `builtins: true` " +
				"for pure-data output.",
			Rules: offenders,
		}
	}
	out, _ := cloneData(data).(map[string]any)
	if out == nil {
		out = map[string]any{}
	}
	out["v"] = tabnas.BUILTIN_SCHEMA_VERSION
	return out, nil
}

// cloneData deep-copies, preserving regexHolder and dropping nothing
// (the data tree already has no functions).
func cloneData(v any) any {
	switch x := v.(type) {
	case regexHolder:
		return x
	case map[string]any:
		o := map[string]any{}
		for k, val := range x {
			o[k] = cloneData(val)
		}
		return mapOrSelf(o)
	case []any:
		out := make([]any, len(x))
		for i, e := range x {
			out[i] = cloneData(e)
		}
		return out
	default:
		return v
	}
}

func mapOrSelf(m map[string]any) map[string]any { return m }

// cloneRecognition drops AST-building hooks: a/bo/bc fields pointing at
// a tree builtin, and the now-orphaned k.node$/k.capture$ config.
func cloneRecognition(v any) map[string]any {
	res := cloneRecognitionVal(v)
	if m, ok := res.(map[string]any); ok {
		return m
	}
	return map[string]any{}
}

func cloneRecognitionVal(v any) any {
	switch x := v.(type) {
	case regexHolder:
		return x
	case []any:
		out := make([]any, len(x))
		for i, e := range x {
			out[i] = cloneRecognitionVal(e)
		}
		return out
	case map[string]any:
		o := map[string]any{}
		for k, val := range x {
			if refFields[k] {
				if s, ok := val.(string); ok && isDroppedRecognition(s) {
					continue
				}
			}
			if k == "k" {
				if km, ok := val.(map[string]any); ok {
					kc := cloneRecognitionVal(km).(map[string]any)
					for _, tk := range treeConfigKeys {
						delete(kc, tk)
					}
					if len(kc) == 0 {
						continue
					}
					o[k] = kc
					continue
				}
			}
			o[k] = cloneRecognitionVal(val)
		}
		return o
	default:
		return v
	}
}

func isDroppedRecognition(s string) bool {
	return treeBuiltins[s]
}

// ---- jsonic serialisation ------------------------------------------

var identRe = regexp.MustCompile(`^[A-Za-z_$][A-Za-z0-9_$]*$`)

// ToJsonic serialises a (function-free) data value as jsonic text.
func ToJsonic(value any, strict bool, indent int) string {
	if indent == 0 {
		indent = 2
	}
	sep := "\n"
	if strict {
		sep = ",\n"
	}
	pad := func(n int) string { return strings.Repeat(" ", indent*n) }

	quote := func(s, ch string) string {
		r := strings.ReplaceAll(s, "\\", "\\\\")
		r = strings.ReplaceAll(r, ch, "\\"+ch)
		r = strings.ReplaceAll(r, "\n", "\\n")
		return ch + r + ch
	}
	dq := func(s string) string { return quote(s, `"`) }
	str := func(s string) string {
		if strict {
			return dq(s)
		}
		return quote(s, "'")
	}
	key := func(k string) string {
		if !strict && identRe.MatchString(k) {
			return k
		}
		return dq(k)
	}

	var ser func(v any, depth int) string
	ser = func(v any, depth int) string {
		if v == nil {
			return "null"
		}
		switch x := v.(type) {
		case regexHolder:
			sentinel := "@/"
			if x.eager {
				sentinel = "@~/"
			}
			return str(sentinel + jsRegexSource(x.re) + "/" + jsRegexFlags(x.re))
		case bool:
			if x {
				return "true"
			}
			return "false"
		case int:
			return fmt.Sprintf("%d", x)
		case float64:
			return strconv.FormatFloat(x, 'g', -1, 64)
		case string:
			return str(x)
		case []any:
			if len(x) == 0 {
				return "[]"
			}
			items := make([]string, len(x))
			for i, e := range x {
				items[i] = pad(depth+1) + ser(e, depth+1)
			}
			return "[\n" + strings.Join(items, sep) + "\n" + pad(depth) + "]"
		case [][]int:
			// S field can arrive typed; convert to a string list form.
			return ser(tinMatrixToAny(x), depth)
		case map[string]any:
			keys := sortedMapKeys(x)
			if len(keys) == 0 {
				return "{}"
			}
			items := make([]string, len(keys))
			for i, k := range keys {
				items[i] = pad(depth+1) + key(k) + ": " + ser(x[k], depth+1)
			}
			return "{\n" + strings.Join(items, sep) + "\n" + pad(depth) + "}"
		}
		return "null"
	}

	return ser(value, 0)
}

func tinMatrixToAny(_ [][]int) any { return nil }

// sortedMapKeys returns map keys in a stable order. The converter emits
// alts as maps; jsonic key order only affects formatting, not meaning.
// Use the canonical field order where recognised, else alpha.
var fieldOrder = map[string]int{
	"options": 0, "rule": 1, "v": 2,
	"fixed": 0, "match": 1, "token": 0, "start": 0,
	"open": 0, "close": 1,
	"s": 0, "b": 1, "c": 2, "p": 3, "r": 4, "a": 5, "k": 6, "n": 7, "u": 8, "m": 9, "g": 10,
}

func sortedMapKeys(m map[string]any) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		oi, iok := fieldOrder[keys[i]]
		oj, jok := fieldOrder[keys[j]]
		if iok && jok {
			if oi != oj {
				return oi < oj
			}
			return keys[i] < keys[j]
		}
		if iok != jok {
			return iok
		}
		return keys[i] < keys[j]
	})
	return keys
}

// jsRegexSource returns a JS-flavoured source for the Go regex (strips
// the leading (?i) flag group and anchor as needed). The converter's
// match-token regexes are `(?i)^…` or `^[\x{..}-\x{..}]`.
func jsRegexSource(re *regexp.Regexp) string {
	s := re.String()
	s = strings.TrimPrefix(s, "(?i)")
	// Keep the anchor; TS emitted `^` + pattern too.
	// Translate Go \x{HHHH} back to JS \uHHHH where 4 hex digits.
	s = goHexToJsUnicode(s)
	return s
}

func jsRegexFlags(re *regexp.Regexp) string {
	if strings.HasPrefix(re.String(), "(?i)") {
		return "i"
	}
	return ""
}

var goHexRe = regexp.MustCompile(`\\x\{([0-9a-fA-F]+)\}`)

func goHexToJsUnicode(s string) string {
	return goHexRe.ReplaceAllStringFunc(s, func(m string) string {
		sub := goHexRe.FindStringSubmatch(m)
		h := sub[1]
		for len(h) < 4 {
			h = "0" + h
		}
		return "\\u" + h
	})
}

// BnfCompile compiles ABNF source into pure-data jsonic text.
func BnfCompile(src string, opts *BnfCompileOptions) (string, error) {
	if opts == nil {
		opts = &BnfCompileOptions{}
	}
	spec, err := Bnf(src, &BnfConvertOptions{
		Start: opts.Start, Tag: opts.Tag, Builtins: true, Marks: true,
	})
	if err != nil {
		return "", err
	}
	recognition := true
	if opts.Recognition != nil {
		recognition = *opts.Recognition
	}
	var data map[string]any
	if !recognition {
		data, err = toPureData(spec)
	} else {
		data, err = toRecognitionData(spec)
	}
	if err != nil {
		return "", err
	}
	return ToJsonic(data, opts.Strict, opts.Indent), nil
}
