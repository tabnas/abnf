module github.com/tabnas/abnf/go

go 1.24.7

require github.com/tabnas/parser/go v0.0.0

// This package is a BNF/ABNF -> tabnas GrammarSpec compiler for the
// tabnas engine. Until tabnas/parser publishes a tagged Go module,
// depend on a sibling checkout — the same development model the
// TypeScript package uses for `tabnas` (file:../../parser/ts). Clone
// https://github.com/tabnas/parser as a sibling of this repo.
replace github.com/tabnas/parser/go => ../../parser/go
