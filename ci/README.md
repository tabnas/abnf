# ci/

Staging area for GitHub Actions workflow changes.

This directory exists because session credentials cannot write
`.github/workflows/*` — see admin `DECISIONS.md` ADR-8. To change CI:

1. Put the intended workflow file in `workflows/`.
2. A maintainer promotes it with the admin `rollout/apply-ci-folders.sh`
   script.

## Pending

- **`workflows/docs.yml`** — the prose gate: Vale over the reader-facing
  pages at the levels set in `.vale.ini`, on the file list
  `ts/scripts/gated-docs.cjs` produces. See `docs/STYLE-GUIDE.md`.

  It needs no sibling checkouts and no secrets, and pins its own Vale
  version. Errors fail the job; warnings go to the run summary as a
  report. `make prose` runs the identical check locally, and the test
  suite already runs the other half of the gate
  (`ts/test/docs.test.js`), so promoting this adds the spelling and
  Google-convention arm rather than the whole gate.

- **`workflows/rust.yml`** — the Rust gate: `ci/rust/run.sh` on the
  crate in `rs/`, which checks formatting, builds, runs the tests and
  the doctests, runs clippy with `-D warnings`, and compares
  `rs/Cargo.lock` against `rs/Cargo.toml` with each sibling crate's own
  version exempted.

  It needs the three sibling checkouts (`parser`, `bnf` and `support`),
  which it clones, and the third-party ABNF corpus, which the script
  fetches because the conformance suite fails rather than skips without
  it. No secrets. `make test-rs` runs the fast inner loop locally and
  `ci/rust/run.sh` runs the identical gate, so promoting this adds the
  hosted run rather than the checks themselves.
