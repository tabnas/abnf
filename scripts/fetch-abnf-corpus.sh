#!/usr/bin/env bash
#
# fetch-abnf-corpus.sh — fetch the third-party ABNF conformance corpus.
#
# THE CORPUS IS NEVER COMMITTED. It is fetched into test/abnf-corpus/,
# which .gitignore excludes. Only this script and the classification
# manifest (test/corpus/manifest.tsv) are tracked. Same pattern as
# toml/ts/test/toml-test/ and xml/test/xmlconf/.
#
# There is NO single authoritative ABNF conformance suite — unlike TOML
# (toml-test) or XML (the W3C suite), the IETF publishes no machine-
# readable test corpus for RFC 5234. What exists instead is the set of
# ABNF grammars that real RFCs publish, collected as test data by other
# ABNF implementations. This script pins four such collections at exact
# commit SHAs. Between them they cover RFC 5234's own self-description
# (three independent transcriptions plus the Errata-3076-corrected
# form), RFC 7405, and real published grammars for TOML, JSON, JSONPath,
# Dhall, OpenPGP, XMPP JIDs, RFC 5322 email, RFC 3966 tel: URIs and
# IPv6 addresses.
#
# Each source is licensed to its own terms (Apache-2.0, MIT, ...). We do
# not redistribute any of it; you fetch it yourself, here, at run time.
#
# Idempotent: re-running is a no-op once a source is at its pinned SHA.
# Pass --force to re-fetch from scratch.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/test/abnf-corpus"

FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

# name|url|pinned commit SHA
SOURCES=(
  "ex_abnf|https://github.com/marcelog/ex_abnf.git|49bcca0fc736b239b44b200a1ae543c878ad8538"
  "node-abnf|https://github.com/hildjj/node-abnf.git|ff5965f960fed68f231e25b24ec31c13b8d00fc8"
  "go-abnf|https://github.com/pandatix/go-abnf.git|c5a80352df0a5efe44f11a73a2322a9b0fdbc661"
  "tree-sitter-abnf|https://github.com/jmitchell/tree-sitter-abnf.git|f68bb6e3cfcc2a3eebf11f47e27efffb128c001f"
)

mkdir -p "$DEST"

for entry in "${SOURCES[@]}"; do
  IFS='|' read -r name url sha <<<"$entry"
  dir="$DEST/$name"
  stamp="$dir/.pinned-sha"

  if [ "$FORCE" = 1 ]; then rm -rf "$dir"; fi

  if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$sha" ]; then
    echo "abnf-corpus: $name already at $sha"
    continue
  fi

  echo "abnf-corpus: fetching $name @ $sha"
  rm -rf "$dir"
  mkdir -p "$dir"
  (
    cd "$dir"
    git init -q .
    git remote add origin "$url"
    # Fetch exactly the pinned commit. Not a branch, not "latest".
    git fetch -q --depth 1 origin "$sha"
    git checkout -q FETCH_HEAD
  )
  # Drop the nested .git so the outer repo never sees a submodule-ish thing.
  rm -rf "$dir/.git"
  echo "$sha" >"$stamp"
done

count=$(find "$DEST" -name '*.abnf' | wc -l | tr -d ' ')
echo "abnf-corpus: ready at $DEST ($count .abnf files)"
