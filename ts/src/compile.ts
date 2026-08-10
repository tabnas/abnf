/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

/*  compile.ts
 *  Compilation mode for ABNF: convert the source, then serialise the
 *  resulting spec as pure-data tabnas grammar text.
 *
 *  The spec transforms themselves (recognition/pure lowering, jsonic
 *  serialisation, user action attachment) are notation-neutral and live
 *  in `@tabnas/bnf`. This file keeps the ABNF-named public API those
 *  transforms used to be exported under, plus `abnfCompile`, which is
 *  the only piece that needs to know how to parse ABNF.
 */

import type { GrammarSpec } from '@tabnas/parser'

import {
  compileSpec,
  toRecognitionSpec,
  toPureSpec,
  toJsonic,
  attachActions,
  attachActionSlots,
  markListing,
  CompileError,
  ActionError,
} from '@tabnas/bnf'

import type {
  CompileOptions,
  JsonicOptions,
  ActionsMap,
} from '@tabnas/bnf'

import { abnf as abnfConvert } from './converter'

// Historical names for the shared errors and options. These are
// re-exports, not `const` aliases: a `const` would carry only the value
// side, so `let e: AbnfCompileError` would stop compiling (TS2749).
export { CompileError as AbnfCompileError, ActionError as AbnfActionError }
type AbnfCompileOptions = CompileOptions


// Compile ABNF source into a pure-data tabnas grammar as jsonic text.
// Always converts with `builtins: true` so probe dispatch and tree
// building serialise as `@…$` builtin refs (no closures).
export function abnfCompile(src: string, opts: AbnfCompileOptions = {}): string {
  const spec: GrammarSpec = abnfConvert(src,
    { start: opts.start, tag: opts.tag, builtins: true, marks: true })
  return compileSpec(spec, opts)
}


export {
  toRecognitionSpec,
  toPureSpec,
  toJsonic,
  attachActions,
  attachActionSlots,
  markListing,
}

export type { AbnfCompileOptions, JsonicOptions, ActionsMap }
