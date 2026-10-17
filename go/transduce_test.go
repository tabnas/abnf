// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnasabnf

import (
	"strings"
	"testing"
)

// transduceHosts is /etc/hosts, as aless's grammar library writes it.
const transduceHosts = "hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array\n" +
	"entry   = address names                      ; @object address names\n" +
	"address = word\n" +
	"names   = 1*word                             ; @array\n" +
	"word    = ( TX )\n"

// The transducer part of an annotated grammar is bnf's, of the converted
// spec, and the text the TypeScript front-end writes
// (ts/test/transduce.test.js); a grammar with no annotation on its start
// rule is read whole, and a source that does not convert is refused as
// Abnf refuses it.
func TestAbnfTransduceWritesTheGrammarsPart(t *testing.T) {
	part, err := AbnfTransduce(transduceHosts, "hosts", nil)
	if err != nil {
		t.Fatal(err)
	}
	want := strings.Join([]string{
		"; The transducer part of the grammar hosts: the JSON events of the value",
		"; its @object and @array annotations build, from its rule passes, as",
		"; tabnas-bnf wrote them from the value actions those annotations attach.",
		"def hosts-transduce [input]",
		"  transduce-tree",
		"    record",
		"      entry :_gen6_plus_word (record (entry :text true))",
		"      entry :_gen6_star_word$alt0 (record (entry :text true))",
		"      entry :entry (record (entry :container :object) (entry :member \"address\") (entry :text true))",
		"      entry :entry$step1 (record (entry :container :object) (entry :member \"names\"))",
		"      entry :hosts (record (entry :container :array))",
		"      entry :hosts$step1 (record (entry :container :array))",
		"      entry :names (record (entry :container :array))",
		"    input",
		"",
	}, "\n")
	if part.Entry != "hosts-transduce" || part.Source != want || part.Whole != "" {
		t.Errorf("got %+v", part)
	}

	opts := &AbnfConvertOptions{WordKeywords: true, Builtins: true}
	spec, err := Abnf(transduceHosts, opts)
	if err != nil {
		t.Fatal(err)
	}
	if got, err := AbnfTransduce(transduceHosts, "hosts", opts); err != nil || got != TransduceAlc(spec, "hosts") {
		t.Errorf("got %+v %v", got, err)
	}

	whole, err := AbnfTransduce("top = 1*DIGIT \".\" 1*DIGIT\n", "g", nil)
	if err != nil || whole.Source != "" || !strings.HasPrefix(whole.Whole, "the start rule top has no annotation, so the value is the {rule, src, kids} tree") {
		t.Errorf("got %+v %v", whole, err)
	}

	if _, err := AbnfTransduce("top = = x\n", "g", nil); err == nil {
		t.Error("a source that does not convert was given a part")
	}
}
