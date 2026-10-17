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
  transduceAlc,
  CompileError,
  ActionError,
} from '@tabnas/bnf'

import type {
  CompileOptions,
  JsonicOptions,
  ActionsMap,
  TransducePart,
} from '@tabnas/bnf'

import { abnf as abnfConvert } from './converter'
import type { AbnfConvertOptions } from './converter'

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


// The transducer part of an ABNF grammar, `<name>-transduce`: the alchemy
// program that streams the JSON events of the value its `; @object` and
// `; @array` annotations build, from its rule passes, or why its documents
// are read whole (`transduceAlc` in @tabnas/bnf says when). Converts as
// `abnfConvert` does, with `opts`.
export function abnfTransduce(
  src: string,
  name: string,
  opts?: AbnfConvertOptions,
): TransducePart {
  return transduceAlc(abnfConvert(src, opts), name)
}


export {
  toRecognitionSpec,
  toPureSpec,
  toJsonic,
  attachActions,
  attachActionSlots,
  markListing,
  transduceAlc,
}

export type { AbnfCompileOptions, JsonicOptions, ActionsMap, TransducePart }
