#!/bin/sh
# Fetch the external ABNF reference corpus into test/abnf-corpus/.
#
# Four other ABNF implementations, kept only for the grammar corpora they
# ship — 68 `.abnf` files between them, including the collected ABNF of
# RFC 3261, 3986, 4566, 5322, 7405, JSON, JSONPath, TOML and Dhall. There is
# no official IETF conformance suite for RFC 5234, so this is the closest
# thing: the ABNF that real RFCs publish, as collected by other people.
#
# The conformance suites in BOTH runtimes read it (ts/test/conformance.test.js
# and go/conformance_test.go) and FAIL LOUDLY when it is absent — they never
# skip. You should not have to run this by hand: `npm test` fetches via the
# `pretest` hook, `go test` fetches from TestMain, and `make test-go` depends
# on `make abnf-corpus`.
#
# Each file's valid/invalid/fragment class lives in test/corpus/manifest.tsv,
# decided by an independent third-party ABNF parser — see
# test/classify-abnf-corpus.sh.
#
# The corpus is NOT vendored — it is .gitignore'd. Four separately-licensed
# upstream repositories have no business in this tree, and committing a
# checkout that still carries its own .git would silently create a mode
# 160000 gitlink (a submodule reference with no .gitmodules), which ships an
# EMPTY directory to everyone who clones. This script therefore deletes each
# .git after checkout and leaves the upstream commit in `.pinned-sha`.
#
#   sh test/fetch-abnf-corpus.sh      # or: make abnf-corpus
#
# Each repository is pinned to the commit the figures were measured against.
# To re-pin, change the SHA here and re-run with FORCE=1.

set -e

DIR="$(cd "$(dirname "$0")" && pwd)/abnf-corpus"

# name|url|pinned commit
CORPUS='
ex_abnf|https://github.com/marcelog/ex_abnf|49bcca0fc736b239b44b200a1ae543c878ad8538
go-abnf|https://github.com/pandatix/go-abnf|c5a80352df0a5efe44f11a73a2322a9b0fdbc661
node-abnf|https://github.com/hildjj/node-abnf|ff5965f960fed68f231e25b24ec31c13b8d00fc8
tree-sitter-abnf|https://github.com/jmitchell/tree-sitter-abnf|f68bb6e3cfcc2a3eebf11f47e27efffb128c001f
'

mkdir -p "$DIR"

echo "$CORPUS" | while IFS='|' read -r name url sha; do
  [ -n "$name" ] || continue
  dest="$DIR/$name"

  if [ -f "$dest/.pinned-sha" ] && [ -z "$FORCE" ]; then
    have="$(cat "$dest/.pinned-sha")"
    if [ "$have" = "$sha" ]; then
      echo "$name already at $sha"
      continue
    fi
    echo "$name is at $have, want $sha — re-fetching."
  fi

  rm -rf "$dest"
  echo "Fetching $name at $sha ..."
  git init -q "$dest"
  git -C "$dest" remote add origin "$url"
  # Fetch the pinned commit directly rather than cloning HEAD, so the
  # measurement is reproducible even after upstream moves on.
  git -C "$dest" fetch -q --depth 1 origin "$sha"
  git -C "$dest" checkout -q FETCH_HEAD
  # The whole point: no nested .git survives into this tree.
  rm -rf "$dest/.git"
  echo "$sha" > "$dest/.pinned-sha"
done

echo "Done: $(find "$DIR" -name '*.abnf' | wc -l | tr -d ' ') .abnf files under $DIR"
