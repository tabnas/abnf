/* Copyright (c) 2025-2026 Richard Rodger and other contributors, MIT License */

/*  abnf.ts
 *  ABNF plugin — adds `tn.abnf(src)` (install) and `tn.abnf.toSpec(src)`
 *  (build without installing) to an Tabnas instance.
 *
 *  The conversion logic itself lives in ./converter.ts; this file
 *  exposes it both as a Plugin (for `tn.use(abnf)`) and as bare
 *  exports (for code that wants to convert without an instance).
 */

import type {
  Tabnas,
  GrammarSpec,
  Plugin,
} from '@tabnas/parser'

import {
  abnf as abnfConvert,
  parseAbnf,
  emitGrammarSpec,
  eliminateLeftRecursion,
  abnfRules,
  AbnfParseError,
  AbnfConvertOptions,
} from './converter'

import {
  abnfCompile,
  toRecognitionSpec,
  toPureSpec,
  toJsonic,
  attachActions,
  attachActionSlots,
  markListing,
  AbnfCompileError,
  AbnfActionError,
  AbnfCompileOptions,
  ActionsMap,
} from './compile'


// Plugin entry point. Decorates the instance with a callable `abnf`
// member that converts and installs a grammar, plus `abnf.toSpec` for
// callers that just want the spec.
type AbnfPluginOptions = AbnfConvertOptions & { actions?: ActionsMap }

const abnf: Plugin = function abnf(tn: Tabnas, _options?: any): void {
  const fn = ((src: string, opts?: AbnfPluginOptions): GrammarSpec => {
    // User actions wrap the compiler's closures, so convert in closure
    // mode (never builtins) and request marks when actions are supplied.
    const spec = abnfConvert(src,
      opts?.actions ? { ...opts, builtins: false, marks: true } : opts)
    if (opts?.actions) attachActions(spec, opts.actions)
    tn.grammar(spec)
    return spec
  }) as ((src: string, opts?: AbnfPluginOptions) => GrammarSpec) & {
    toSpec: (src: string, opts?: AbnfConvertOptions) => GrammarSpec
  }
  fn.toSpec = (src: string, opts?: AbnfConvertOptions): GrammarSpec =>
    abnfConvert(src, opts)
  tn.abnf = fn
}


export {
  abnf,
  abnfConvert,
  parseAbnf,
  emitGrammarSpec,
  eliminateLeftRecursion,
  abnfRules,
  AbnfParseError,
  abnfCompile,
  toRecognitionSpec,
  toPureSpec,
  toJsonic,
  attachActions,
  attachActionSlots,
  markListing,
  AbnfCompileError,
  AbnfActionError,
}

export type { AbnfConvertOptions, AbnfCompileOptions, ActionsMap }
