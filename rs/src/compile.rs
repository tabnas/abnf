// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

//! Compilation mode: convert ABNF source, then serialize the resulting
//! spec as pure-data tabnas grammar text.
//!
//! The spec transforms themselves (recognition and pure lowering, jsonic
//! serialization, user action attachment) are notation-neutral and live
//! in `tabnas-bnf`. What stays here is [`abnf_compile`], the only piece
//! that has to know how to read ABNF.

use tabnas_bnf::{compile_spec, ConvertOptions};

use crate::AbnfError;
use crate::{abnf_convert, AbnfCompileOptions};

/// Compile ABNF source into a pure-data tabnas grammar, as jsonic text.
///
/// Always converts with `builtins: true`, so probe dispatch and tree
/// building serialize as `@…$` builtin refs and the result carries no
/// closures at all.
pub fn abnf_compile(src: &str, opts: &AbnfCompileOptions) -> Result<String, AbnfError> {
    let spec = abnf_convert(
        src,
        Some(&ConvertOptions {
            start: opts.start.clone(),
            tag: opts.tag.clone(),
            builtins: true,
            marks: true,
            ..ConvertOptions::default()
        }),
    )?;
    compile_spec(
        &spec,
        tabnas_bnf::CompileOptions {
            recognition: opts.recognition,
            strict: opts.strict,
            indent: opts.indent,
        },
    )
    .map_err(AbnfError::Compile)
}
