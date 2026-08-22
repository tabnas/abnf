// Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License

// Package tabnasabnf is an ABNF -> tabnas GrammarSpec compiler for the
// tabnas parsing engine (github.com/tabnas/parser/go). It is a faithful
// Go port of the @tabnas/abnf TypeScript package.
//
// Given a small ABNF dialect it produces a function-free (when
// requested) GrammarSpec that, installed on a tabnas engine, parses
// inputs in that grammar and builds a {rule, src, kids} AST. It also
// emits "pure-data" jsonic (recognition / pure specs) and supports
// user actions.
//
// The package mirrors the TS sources:
//   - converter.ts -> converter.go (AST, parseAbnf, abnfRules, desugar,
//     core rules, left-recursion elimination, probe-dispatch rewriter,
//     FIRST sets, emitGrammarSpec, token allocation, Abnf entry) and
//     parser_abnf.go (the ABNF-for-ABNF parser grammar).
//   - compile.ts -> compile.go (AbnfCompile, ToRecognitionSpec,
//     ToPureSpec, ToJsonic, AttachActions, MarkListing).
//   - abnf.ts -> facade.go (Abnf, ParseAbnf, EmitGrammarSpec,
//     EliminateLeftRecursion, Install — the public facade).
package tabnasabnf

// VERSION is this module's version. It MUST equal ts/package.json
// "version": the release orchestrator rewrites both, and
// TestVersionMatchesPackageJSON fails the build if they drift.
const VERSION = "0.4.7"
