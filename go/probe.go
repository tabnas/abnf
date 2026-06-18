// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

package abnf

// probe.go — the probe-dispatch analyser + rewriter, the Go port of the
// `[X D] Y` ambiguity handling in converter.ts. For an optional group
// whose body ends with a terminal D and is followed by a tail Y whose
// leading vocabulary overlaps X's, we rewrite the rule to a
// probe + phase-retry dispatcher (function-free in builtins mode).

import "strconv"

// isProbeableOpt: element is `[ X D ]` where X is one or more elements
// and D is a terminal literal or a regex terminal. Returns (xSeq, D).
func isProbeableOpt(el *bnfElement) (bnfSequence, *bnfElement, bool) {
	if el.Kind != kindOpt {
		return nil, nil, false
	}
	inner := el.Inner
	if inner.Kind != kindGroup || len(inner.Alts) != 1 {
		return nil, nil, false
	}
	seq := inner.Alts[0]
	if len(seq) < 2 {
		return nil, nil, false
	}
	last := seq[len(seq)-1]
	if last.Kind != kindTerm && last.Kind != kindRegex {
		return nil, nil, false
	}
	xSeq := append(bnfSequence{}, seq[:len(seq)-1]...)
	return xSeq, last, true
}

func collectTerminalVocabElements(el *bnfElement, grammar *bnfGrammar,
	out map[string]*bnfElement, visited map[string]bool) {
	switch el.Kind {
	case kindTerm:
		k := termKey(el)
		if _, ok := out[k]; !ok {
			out[k] = el
		}
	case kindRegex:
		k := regexKey(el)
		if _, ok := out[k]; !ok {
			out[k] = el
		}
	case kindRef:
		if visited[el.Name] {
			return
		}
		visited[el.Name] = true
		prod := findProd(grammar, el.Name)
		if prod == nil {
			return
		}
		for _, alt := range prod.Alts {
			for _, sub := range alt {
				collectTerminalVocabElements(sub, grammar, out, visited)
			}
		}
	case kindOpt, kindStar, kindPlus, kindRep:
		collectTerminalVocabElements(el.Inner, grammar, out, visited)
	case kindGroup:
		for _, alt := range el.Alts {
			for _, sub := range alt {
				collectTerminalVocabElements(sub, grammar, out, visited)
			}
		}
	}
}

func collectSeqVocabElements(seq bnfSequence, grammar *bnfGrammar) map[string]*bnfElement {
	out := map[string]*bnfElement{}
	visited := map[string]bool{}
	for _, el := range seq {
		collectTerminalVocabElements(el, grammar, out, visited)
	}
	return out
}

func mapsOverlap(a, b map[string]*bnfElement) bool {
	for k := range a {
		if _, ok := b[k]; ok {
			return true
		}
	}
	return false
}

// rewriteProbeDispatches rewrites every ambiguous `[X D] Y` subsequence
// into a probe-dispatch pattern. Runs before token allocation.
func rewriteProbeDispatches(grammar *bnfGrammar) *bnfGrammar {
	reports := grammar.Ambiguities
	if reports == nil {
		reports = []ambiguityReport{}
	}
	extra := []*bnfProduction{}
	used := map[string]bool{}
	for _, p := range grammar.Productions {
		used[p.Name] = true
	}
	freshName := func(hint string) string {
		name := hint
		i := 1
		for used[name] {
			name = hint + strconv.Itoa(i)
			i++
		}
		used[name] = true
		return name
	}

	rewritten := []*bnfProduction{}

	for _, prod := range grammar.Productions {
		newAlts := []bnfSequence{}
		touched := false
		for altIdx, alt := range prod.Alts {
			resultAlt := bnfSequence{}
			for i := 0; i < len(alt); i++ {
				el := alt[i]
				xSeq, disamb, ok := isProbeableOpt(el)
				if !ok {
					resultAlt = append(resultAlt, el)
					continue
				}
				ySeq := append(bnfSequence{}, alt[i+1:]...)
				if len(ySeq) == 0 {
					resultAlt = append(resultAlt, el)
					continue
				}
				xVocab := collectSeqVocabElements(xSeq, grammar)
				yVocab := collectSeqVocabElements(ySeq, grammar)
				if !mapsOverlap(xVocab, yVocab) {
					resultAlt = append(resultAlt, el)
					continue
				}

				// Joint vocab minus the disambiguator.
				vocab := map[string]*bnfElement{}
				vocabOrder := []string{}
				addVocab := func(m map[string]*bnfElement, order *[]string) {
					for _, k := range orderedVocabKeys(m) {
						if _, exists := vocab[k]; !exists {
							vocab[k] = m[k]
							*order = append(*order, k)
						}
					}
				}
				addVocab(xVocab, &vocabOrder)
				addVocab(yVocab, &vocabOrder)
				var dKey string
				if disamb.Kind == kindTerm {
					dKey = termKey(disamb)
				} else if disamb.Kind == kindRegex {
					dKey = regexKey(disamb)
				}
				if dKey != "" {
					delete(vocab, dKey)
				}

				dispatchName := freshName(prod.Name + "$pd" + strconv.Itoa(i))
				probeName := freshName(dispatchName + "$probe")
				withName := freshName(dispatchName + "$with")
				noName := freshName(dispatchName + "$no")

				// Vocab elements in insertion order, minus disambiguator.
				vocabElems := []*bnfElement{}
				for _, k := range vocabOrder {
					if v, ok := vocab[k]; ok {
						vocabElems = append(vocabElems, v)
					}
				}

				extra = append(extra, &bnfProduction{
					Name:        probeName,
					Alts:        []bnfSequence{},
					ProbeHelper: &probeHelperSpec{VocabElements: vocabElems},
					NodeKind:    "helper",
				})
				withAlt := append(append(bnfSequence{}, xSeq...), disamb)
				withAlt = append(withAlt, ySeq...)
				extra = append(extra, &bnfProduction{
					Name: withName, Alts: []bnfSequence{withAlt}, NodeKind: "helper"})
				extra = append(extra, &bnfProduction{
					Name: noName, Alts: []bnfSequence{ySeq}, NodeKind: "helper"})
				extra = append(extra, &bnfProduction{
					Name: dispatchName,
					Alts: []bnfSequence{
						{{Kind: kindRef, Name: withName}},
						{{Kind: kindRef, Name: noName}},
					},
					ProbeDisp: &probeDispatchSpec{
						ProbeRule:     probeName,
						Disambiguator: disamb,
						WithBranch:    withName,
						NoBranch:      noName,
					},
					NodeKind: "helper",
				})

				reports = append(reports, ambiguityReport{
					Rule: prod.Name, AltIdx: altIdx, OptIdx: i,
					Reason: "optional prefix shares vocabulary with tail", Resolved: true,
				})

				resultAlt = append(resultAlt, &bnfElement{Kind: kindRef, Name: dispatchName})
				i = len(alt)
				touched = true
			}
			newAlts = append(newAlts, resultAlt)
		}
		if touched {
			rewritten = append(rewritten, &bnfProduction{
				Name: prod.Name, Alts: newAlts, NodeKind: prod.NodeKind})
		} else {
			rewritten = append(rewritten, prod)
		}
	}

	return &bnfGrammar{
		Productions: append(rewritten, extra...),
		Ambiguities: reports,
	}
}

// orderedVocabKeys returns the keys of a vocab map. To mirror the TS
// Map insertion order (which derives from grammar walk order) we sort
// for determinism; vocab membership, not order, drives correctness.
func orderedVocabKeys(m map[string]*bnfElement) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sortStrings(keys)
	return keys
}
