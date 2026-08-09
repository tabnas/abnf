#!/bin/sh
# Regenerate test/corpus/manifest.tsv from the fetched reference corpus.
#
# The manifest says, for every .abnf file in test/abnf-corpus/, whether it is
# valid RFC 5234 ABNF, invalid, or a fragment. Those labels MUST NOT come from
# this implementation — that is marking your own homework, and it would make
# the conformance suite tautological. They come from an INDEPENDENT
# third-party ABNF parser:
#
#     npm `abnf` 5.0.4  ==  https://github.com/hildjj/node-abnf
#
# installed into a scratch directory, used, and thrown away. It is NOT a
# dependency of this package.
#
# Classification rule:
#   valid     the oracle parses it, and every rulename it references is
#             either defined in the file or is an RFC 5234 Appendix B.1 core
#             rule (which a conformant compiler splices in). Rules defined
#             but never referenced are irrelevant.
#   invalid   the oracle refuses to parse it.
#   fragment  parses, but references rules nothing defines. Neither
#             must-accept nor must-reject, so it is excluded from BOTH halves
#             of the measurement — and counted, so the exclusion stays
#             visible instead of quietly shrinking the corpus.
#
#   sh test/classify-abnf-corpus.sh            # print to stdout
#   sh test/classify-abnf-corpus.sh --write    # overwrite the manifest
#
# Re-running this after re-pinning a SHA in fetch-abnf-corpus.sh is the
# supported way to grow the corpus. Do NOT hand-edit a row to move a file out
# of the valid set because this compiler fails it — narrowing the corpus to
# flatter the score is the exact failure this suite exists to prevent.

set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORPUS="$ROOT/test/abnf-corpus"
ORACLE_VERSION="5.0.4"
WORK="${TMPDIR:-/tmp}/tabnas-abnf-oracle"

if [ ! -d "$CORPUS" ]; then
  echo "corpus missing; run: sh test/fetch-abnf-corpus.sh" >&2
  exit 1
fi

mkdir -p "$WORK"
if [ ! -d "$WORK/node_modules/abnf" ]; then
  echo "installing third-party oracle abnf@$ORACLE_VERSION into $WORK" >&2
  (cd "$WORK" && npm init -y >/dev/null 2>&1 &&
    npm install --no-audit --no-fund "abnf@$ORACLE_VERSION" >/dev/null)
fi

OUT=$(cd "$WORK" && CORPUS="$CORPUS" node -e '
const abnf = require("abnf")
const fs = require("fs"), path = require("path")
const D = process.env.CORPUS
// RFC 5234 Appendix B.1 core rules: spliced in when referenced, not defined.
const CORE = new Set(["ALPHA","BIT","CHAR","CR","CRLF","CTL","DIGIT","DQUOTE",
  "HEXDIG","HTAB","LF","LWSP","OCTET","SP","VCHAR","WSP"])
const files = []
;(function walk(d) {
  for (const e of fs.readdirSync(d).sort()) {
    const p = path.join(d, e)
    if (fs.statSync(p).isDirectory()) walk(p)
    else if (p.endsWith(".abnf")) files.push(p)
  }
})(D)
const rows = ["path\tclass\tsource-note"]
for (const f of files) {
  const rel = path.relative(D, f)
  let cls, why
  try {
    const diags = abnf.checkRefs(abnf.parseString(fs.readFileSync(f, "utf8"), rel)) || []
    const unknown = [...new Set(diags
      .filter((d) => /unknown rule/i.test(String(d.message || d)))
      .map((d) => (String(d.message || d).match(/unknown rule "([^"]+)"/) || [])[1])
      .filter((n) => n && !CORE.has(n.toUpperCase())))]
    if (unknown.length) { cls = "fragment"; why = "unresolved refs: " + unknown.join(",") }
    else { cls = "valid"; why = "parses; all refs resolve (core rules allowed)" }
  } catch (e) {
    cls = "invalid"
    why = String(e.message).split("\n")[0].replace(/\s+/g, " ").slice(0, 110)
  }
  rows.push([rel, cls, why].join("\t"))
}
process.stdout.write(rows.join("\n") + "\n")
')

if [ "${1:-}" = "--write" ]; then
  printf '%s\n' "$OUT" >"$ROOT/test/corpus/manifest.tsv"
  echo "wrote $ROOT/test/corpus/manifest.tsv" >&2
  awk -F'\t' 'NR>1{c[$2]++} END{for (k in c) printf "  %-9s %d\n", k, c[k]}' \
    "$ROOT/test/corpus/manifest.tsv" >&2
else
  printf '%s\n' "$OUT"
fi
