#!/usr/bin/env bash
# Rust port gate. Kept in one script so local and hosted validation cannot
# quietly drift apart: .github/workflows/rust.yml runs this file, and so can
# you. `make test-rs` is the fast inner loop; this is the full gate.
#
# The engine and the shared BNF-family compiler are PATH DEPENDENCIES on
# sibling checkouts (rs/Cargo.toml: `tabnas = { path = "../../parser/rs" }`
# and `tabnas-bnf = { path = "../../bnf/rs" }`), and neither crate is
# published, so there is no registry version to fall back on. Clone
# https://github.com/tabnas/parser and https://github.com/tabnas/bnf next
# to this repo before running. The test suite also needs
# https://github.com/tabnas/support, a dev-dependency on a third sibling.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)

# Every path dependency of rs/Cargo.toml, dev-dependencies included,
# checked before cargo gets a chance to fail on one with a less useful
# message.
for SIBLING in parser bnf support; do
  if [[ ! -f "$ROOT/../$SIBLING/rs/Cargo.toml" ]]; then
    echo "no $SIBLING checkout at $ROOT/../$SIBLING/rs" >&2
    echo "clone https://github.com/tabnas/$SIBLING as a sibling of $(basename "$ROOT")" >&2
    exit 1
  fi
done

cd "$ROOT/rs"

# Run through the MSRV toolchain when one is available. The workflow
# installs it explicitly, but a contributor running this script gets
# whatever `cargo` is on their PATH -- and a newer toolchain accepts code
# and formatting that the MSRV rejects, so the "local and hosted cannot
# drift" claim this script exists for would hold everywhere except the
# compiler version. Loud rather than silent when the toolchain is absent,
# because a quiet fallback is the drift.
# `rustup` treats `1.85` and `1.85.1` as DISTINCT versioned channels. A
# machine carrying only `1.85.1-x86_64-unknown-linux-gnu` satisfies the
# MSRV and matches the check below, but `cargo +1.85` on it does not run
# that toolchain: it resolves the `1.85` channel, finds it absent, and
# tries to DOWNLOAD it -- which fails outright for an offline
# contributor and silently installs a second toolchain for everyone
# else. So capture the full name of the toolchain that matched and run
# Cargo through that name, never through the `major.minor` prefix that
# only happened to match it.
MSRV=$(awk -F'"' '/^rust-version = /{print $2; exit}' Cargo.toml)
CARGO=(cargo)
if [[ -n "$MSRV" ]]; then
  MSRV_RE=$(printf '%s' "$MSRV" | sed 's/\./\\./g')
  TOOLCHAIN=""
  if command -v rustup >/dev/null 2>&1; then
    # `rustup toolchain list` marks the active one with a parenthetical
    # suffix, so take the first field. The pattern anchors the version
    # and requires a separator after it, so `1.85` does not match
    # `1.850`; either `1.85-<host>` or `1.85.<patch>-<host>` satisfies
    # the MSRV, and the first listed wins.
    #
    # `|| true` because NO MATCH IS THE NORMAL CASE this block exists to
    # handle. `grep` exits 1 when it matches nothing, `set -o pipefail`
    # makes that the pipeline's status, and `set -e` then kills the
    # whole script on the assignment -- silently, with status 1, before
    # the warning below ever prints. That is what a contributor without
    # the MSRV toolchain saw: a gate that exits 1 and says nothing.
    TOOLCHAIN=$(rustup toolchain list 2>/dev/null \
      | awk '{print $1}' \
      | grep -E "^${MSRV_RE}([.-]|$)" \
      | head -n 1) || true
  fi
  if [[ -n "$TOOLCHAIN" ]]; then
    CARGO=(cargo "+$TOOLCHAIN")
  else
    echo "warning: MSRV $MSRV is not installed; running on $(rustc --version 2>/dev/null)" >&2
    echo "         install it with: rustup toolchain install $MSRV" >&2
    echo "         a newer toolchain can accept what $MSRV rejects" >&2
  fi
fi

# Assert the lock's entry for THIS crate still matches the manifest,
# BEFORE anything runs cargo. Without `--locked` (see below) a cargo
# command silently rewrites Cargo.lock in the runner, so a version bump
# that updates rs/Cargo.toml and forgets rs/Cargo.lock passes every
# check and ships a stale lock. This has to come first -- after a cargo
# command the lock has already been fixed up and the check can never fail.
#
# Only this crate's entry is asserted. The engine's entry legitimately
# moves whenever the sibling checkout does, which is the same reason
# blanket `--locked` is wrong here.
CRATE=$(awk -F'"' '/^name = /{print $2; exit}' Cargo.toml)
WANT=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
HAVE=$(awk -v c="$CRATE" -F'"' '
  $0 == "name = \"" c "\"" { f = 1; next }
  f && /^version = / { print $2; exit }
' Cargo.lock)

if [[ "$WANT" != "$HAVE" ]]; then
  echo "Cargo.lock records $CRATE ${HAVE:-<missing>}, but Cargo.toml says $WANT" >&2
  echo "run a cargo command and commit the updated rs/Cargo.lock" >&2
  exit 1
fi

# That version check is the common case stated clearly; it is NOT the whole
# check. A pull request that adds, removes or re-pins a DEPENDENCY leaves
# the crate's own version alone, so it sails past the comparison above while
# leaving the committed lock stale -- cargo then regenerates it in the
# runner and everything goes green.
#
# So the whole resolution is compared, before and after cargo runs, with one
# exemption per path dependency: the sibling crate's recorded version. That
# entry legitimately moves whenever the sibling checkout does, and exempting
# exactly those is what makes a full comparison usable here when blanket
# `--locked` is not.
lock_without_sibling_versions() {
  awk '
    /^\[\[package\]\]$/         { sib = 0 }
    /^name = "tabnas"$/         { sib = 1 }
    /^name = "tabnas-bnf"$/     { sib = 1 }
    /^name = "tabnas-support"$/ { sib = 1 }
    sib && /^version = /        { print "version = \"<sibling>\""; next }
                                { print }
  ' "$1"
}

LOCK_BEFORE=$(mktemp)
cp Cargo.lock "$LOCK_BEFORE"
# On any exit, a red run included, put the lock back if a cargo command
# rewrote it, then drop the snapshot: the tree is left as it was found.
trap 'if [ -f "$LOCK_BEFORE" ] && ! cmp -s "$LOCK_BEFORE" Cargo.lock; then cp "$LOCK_BEFORE" Cargo.lock; fi; rm -f "$LOCK_BEFORE"' EXIT

# NOT `--locked`, deliberately, and this is the one place the plugin gate
# differs from the engine's own (parser ci/rust/run.sh does pass it).
#
# Cargo.lock records the siblings by version, and each is resolved from a
# checkout of MAIN. So the day parser, bnf or support bumps its crate
# version, `--locked` here fails with "cannot update the lock file" on
# every pull request in this repo, including ones that touch no Rust at
# all -- a red build caused by another repository's release.
#
# NOT `--all` on fmt either. cargo defines it as "all packages, and also
# their local path-based dependencies", and every sibling IS such a
# dependency, so `--all` reaches into the parser, bnf and support
# checkouts: an unformatted file over there fails this gate even when
# every file here is clean.
# The conformance suite reads the third-party corpus and FAILS when it
# is absent, never skips, so the fetch happens before the tests rather
# than inside them. The script is idempotent and pins exact commits, so
# a second run is a no-op. The suite fetches it too, as a fallback for
# anyone running cargo directly.
if [[ ! -d "$ROOT/test/abnf-corpus" ]]; then
  ( cd "$ROOT" && sh test/fetch-abnf-corpus.sh )
fi

# rs/tests/divergence_test.rs measures the CANONICAL half of every
# DIVERGENCE.md entry by running node over ts/dist/abnf.js, so that an
# entry which closes from the TypeScript side fails as loudly as one
# that closes from the Rust side. That build is not committed and this
# gate does not produce it. Say plainly when it is missing and turn the
# canonical half off, rather than failing a Rust gate on a TypeScript
# build it never asked for -- loud, for the same reason the MSRV
# fallback above is loud, because a quiet fallback is the drift.
#
# `${ABNF_CANONICAL:-}` and never `$ABNF_CANONICAL`: `set -u` aborts on an
# unset name, and UNSET IS THE NORMAL CASE here. That is the same way the
# MSRV block above once died -- a line that cannot fail, failing, taking
# the whole gate with it and printing nothing. The `else` arm prints on a
# green run for the same reason: "the canonical half ran" has to be
# visible, because the only alternative is inferring it from silence.
if [[ "off" == "${ABNF_CANONICAL:-}" ]]; then
  echo "warning: ABNF_CANONICAL=off arrived from the environment" >&2
  echo "         THE CANONICAL HALF OF EVERY DIVERGENCE.md ENTRY IS NOT MEASURED" >&2
  echo "         in this run, and nothing below will say so again." >&2
elif ! command -v node >/dev/null 2>&1 || [[ ! -f "$ROOT/ts/dist/abnf.js" ]]; then
  echo "warning: the canonical TypeScript is not runnable here" >&2
  echo "         (needs node and $ROOT/ts/dist/abnf.js)" >&2
  echo "         THE CANONICAL HALF OF EVERY DIVERGENCE.md ENTRY IS NOT MEASURED" >&2
  echo "         in this run: rs/tests/divergence_test.rs pins only the Rust side," >&2
  echo "         so a divergence that closes in ts/ will not be reported." >&2
  echo "         Build it with: (cd ts && npm install && npm run build)" >&2
  export ABNF_CANONICAL=off
else
  echo "the canonical TypeScript is runnable: both halves of every"
  echo "DIVERGENCE.md entry are measured in this run."
fi

"${CARGO[@]}" fmt --check
"${CARGO[@]}" build --all-targets
"${CARGO[@]}" test --all-targets
# `--all-targets` does NOT include doctests -- cargo documents the selector
# as "Test all targets (does not include doctests)" -- so a broken example
# in the crate docs (and the README, which is doctested) passes a gate
# that only runs it.
"${CARGO[@]}" test --doc
"${CARGO[@]}" clippy --all-targets --all-features -- -D warnings

# Now that cargo has had every chance to rewrite it, the lock must still
# describe the same resolution it did when committed.
if ! diff -q <(lock_without_sibling_versions "$LOCK_BEFORE") \
             <(lock_without_sibling_versions Cargo.lock) >/dev/null; then
  echo "rs/Cargo.lock does not match rs/Cargo.toml -- cargo rewrote it:" >&2
  diff <(lock_without_sibling_versions "$LOCK_BEFORE") \
       <(lock_without_sibling_versions Cargo.lock) >&2 || true
  echo >&2
  echo "run a cargo command and commit the updated rs/Cargo.lock" >&2
  cp "$LOCK_BEFORE" Cargo.lock   # leave the tree as it was found
  exit 1
fi
# The exempted sibling versions may still have moved, and cargo wrote them
# into the lock. Put the lock back so a green gate leaves the tree exactly
# as it found it, whatever the siblings did: updating the committed lock is
# a deliberate cargo run and commit, never a side effect of running the gate.
if ! cmp -s "$LOCK_BEFORE" Cargo.lock; then
  cp "$LOCK_BEFORE" Cargo.lock
fi

# A green run leaves the tree exactly as it found it too. The exempted
# sibling versions may have moved and cargo may have rewritten them;
# updating the committed lock is a deliberate cargo run and commit, never
# a gate side effect.
if ! cmp -s "$LOCK_BEFORE" Cargo.lock; then
  cp "$LOCK_BEFORE" Cargo.lock
fi
