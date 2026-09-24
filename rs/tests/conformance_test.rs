// Third-party ABNF conformance, Rust half.
//
// The mirror of `ts/test/conformance.test.js` and
// `go/conformance_test.go`. All three halves read the SAME corpus
// (`test/abnf-corpus/`, fetched by `test/fetch-abnf-corpus.sh` at pinned
// commit SHAs, never committed), the SAME third-party classification
// manifest (`test/corpus/manifest.tsv`), the SAME mutation table
// (`test/corpus/mutations.tsv`) and the SAME pinned residual gaps
// (`test/corpus/known-gaps.tsv`, `rust` rows). No runtime can report a
// different conformance from another without one of them going red.
//
// Four things bear repeating:
//
//   - IT CANNOT SKIP. A missing corpus is a failure naming the fetch
//     command, and the fetch is attempted first anyway. A conformance
//     test that quietly does not run is worse than no test, because the
//     green tick is a lie.
//
//   - VALID GRAMMARS GET A VALUE ASSERTION, not merely "it did not
//     error": every rulename the source declares (RFC 5234 section 4,
//     `rule = rulename defined-as elements c-nl`) must be reachable in
//     the compiled grammar as a rule, a fixed token or a match token.
//
//   - EVERY BASE COMPILE IS BUDGETED, in its own process. Two real
//     published grammars in the corpus (RFC 5322 email, Dhall) do not
//     terminate in this compiler, in any runtime, and one more
//     (ex_abnf's RFC 5322) finishes here only at about 161s, well past
//     the budget, where node and `go test` clear it in 13s and 16s.
//     Exceeding the budget is recorded as a failure to accept, never as
//     a pass and never as a skip. On the INVALID half that means it is
//     never scored as a rejection either: the child reports
//     `{budget: true, ok: false}`, which is indistinguishable from a
//     refusal unless the budget flag is read, and a nontermination on
//     invalid input would otherwise leave this suite green. That third
//     file is how the omission was found.
//
//   - THE RESIDUAL GAPS ARE AN EXACT SET, not a ratchet. Fixing one
//     fails the suite as loudly as regressing one; the fix is to delete
//     its row from `known-gaps.tsv`. Never edit a row to silence a
//     failure you did not fix.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use regex::Regex;
use tabnas_abnf::abnf_convert;

use common::repo_root;

/// The budget one corpus compile gets, matching the other two halves.
const BUDGET_BYTES: u64 = 256 << 20;
const BUDGET: Duration = Duration::from_secs(60);

/// Set in the subprocess by `compile_budgeted`, consumed by
/// `conformance_child`.
const ENV_FILE: &str = "ABNF_CONFORMANCE_FILE";
const ENV_APPEND: &str = "ABNF_CONFORMANCE_APPEND";
const ENV_OUT: &str = "ABNF_CONFORMANCE_OUT";

/// Print the measured `rust` rows of `known-gaps.tsv` instead of
/// asserting. For the maintainer who has just changed the compiler; it
/// weakens nothing, it only reports.
const ENV_RECORD: &str = "ABNF_CONFORMANCE_RECORD";

/// The child's exit code when its OWN watchdog stops it on the wall
/// clock. `EXIT_BUDGET_MEMORY` is the same stop on the resident cap.
///
/// These are the only exit statuses that mean "the compiler did not
/// finish", so they are the only ones the sweep may score as a budget
/// failure. Everything else -- a panic, an abort, a stack that ran out,
/// a loader failure, a binary rebuilt underneath the sweep -- is a
/// CHILD CRASH, and a crash must fail the parent rather than be counted
/// as the compiler correctly rejecting an invalid grammar. The whole
/// invalid half is scored on `!ok`, so folding a crash into the budget
/// would let a compiler that ABORTS on bad input read as a compiler
/// that refuses it, and the suite would stay green through the
/// regression it exists to catch.
const EXIT_BUDGET: i32 = 3;

/// The child's exit code when its own watchdog stops it on the 256 MB
/// resident cap. Scored exactly like `EXIT_BUDGET`; it is kept apart only
/// so that a `budget-timing` row, which waives a host-dependent wall-clock
/// stop, can never waive a memory blow-up.
const EXIT_BUDGET_MEMORY: i32 = 4;

/// The child could not read the grammar it was handed. A missing or
/// unreadable corpus file is a fault in the harness, not a measurement,
/// so it is fatal too.
const EXIT_UNREADABLE: i32 = 2;

const FETCH_HINT: &str = concat!(
    "\n  Fetch it with:  sh test/fetch-abnf-corpus.sh   (or: make abnf-corpus)",
    "\n  `make test-rs` does this for you.",
    "\n  This test MUST NOT be skipped: a conformance run that silently does",
    "\n  not happen is the exact defect this suite exists to prevent."
);

fn corpus_dir() -> PathBuf {
    repo_root().join("test").join("abnf-corpus")
}

// ---- the budgeted child ---------------------------------------------

/// Compile one grammar under a hard heap and wall-clock cap, write the
/// result as JSON, and exit.
///
/// This is a test rather than a `main` because an integration test
/// binary has no other entry point. It is `#[ignore]`d so that an
/// ordinary run never reaches it, and the parent re-executes this same
/// binary with `--ignored --exact` to select it.
#[test]
#[ignore = "the budgeted child of the conformance sweep; run by the parent, never on its own"]
fn conformance_child() {
    let Ok(file) = std::env::var(ENV_FILE) else {
        return;
    };
    let out = std::env::var(ENV_OUT).expect("the parent names an output file");
    let append = std::env::var(ENV_APPEND).unwrap_or_default();

    // A watchdog on this process's own resident size. The two grammars
    // that blow up exceed the wall clock as well, so the clock alone
    // decides the dial; this is here so a runaway compile cannot take
    // the machine down with it. Linux only, because that is where the
    // measurement is free; elsewhere the wall clock stands alone.
    std::thread::spawn(|| {
        let deadline = Instant::now() + BUDGET;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            if BUDGET_BYTES < resident_bytes() {
                std::process::exit(EXIT_BUDGET_MEMORY);
            }
            if Instant::now() > deadline {
                std::process::exit(EXIT_BUDGET);
            }
        }
    });

    let mut src = fs::read_to_string(&file).unwrap_or_else(|error| {
        eprintln!("{file}: {error}");
        std::process::exit(EXIT_UNREADABLE);
    });
    if !append.is_empty() {
        src = apply_mutation(&src, &append);
    }

    let result = match abnf_convert(&src, None) {
        Ok(spec) => serde_json::json!({ "ok": true, "names": spec_names(&spec) }),
        Err(error) => serde_json::json!({
            "ok": false,
            "error": error.to_string().lines().next().unwrap_or_default(),
        }),
    };
    let mut handle = fs::File::create(&out).expect("the output file is writable");
    handle
        .write_all(result.to_string().as_bytes())
        .expect("the result is written");
    drop(handle);
    std::process::exit(0);
}

/// This process's resident size in BYTES, or 0 where the platform does
/// not say.
///
/// Read from `VmRSS` in `/proc/self/status`, which the kernel reports as
/// a size. The resident field of `/proc/self/statm` is a count of HOST
/// pages, and multiplying it by a hard-coded 4096 silently measures the
/// wrong thing wherever the host page is not four kibibytes: on a 64 KiB
/// page system the same resident size reads as a sixteenth of itself, so
/// a 256 MB watchdog would admit about four gigabytes before firing.
/// `resident_from_status` carries the unit conversion and is unit tested.
fn resident_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/self/status")
            .ok()
            .map_or(0, |status| resident_from_status(&status))
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// The `VmRSS` line of a `/proc/<pid>/status` file, in bytes.
///
/// The kernel writes this line as `VmRSS:\t   <n> kB`, and the unit is
/// always kibibytes whatever the host page size is -- which is the whole
/// reason to read it rather than to scale a page count by a guess. An
/// absent or unparsable line answers 0, the same "the platform does not
/// say" this function's caller already treats as no measurement.
fn resident_from_status(status: &str) -> u64 {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| {
            let mut fields = rest.split_whitespace();
            let size = fields.next()?.parse::<u64>().ok()?;
            match fields.next() {
                Some("kB") | Some("kb") | None => Some(size * 1024),
                // A unit this parser does not know is not a number to
                // guess at: answer "no measurement" and leave the wall
                // clock to decide, as it does off Linux.
                Some(_) => None,
            }
        })
        .unwrap_or(0)
}

/// The watchdog reads a SIZE, not a count of pages.
#[test]
fn the_resident_watchdog_does_not_assume_a_page_size() {
    assert_eq!(
        resident_from_status("Name:\tabnf\nVmRSS:\t  262144 kB\nThreads:\t2\n"),
        256 << 20
    );
    assert_eq!(resident_from_status("VmRSS:\t0 kB\n"), 0);
    assert_eq!(
        resident_from_status("VmHWM:\t 100 kB\n"),
        0,
        "the wrong field"
    );
    assert_eq!(resident_from_status(""), 0);

    // What the statm field this replaces would have said about the same
    // process on a host whose page is 64 KiB: 4096 pages, which the old
    // `pages * 4096` turned into 16 MB. The 256 MB watchdog would then
    // have let the child reach about four gigabytes.
    let pages = (256u64 << 20) / (64 << 10);
    assert_eq!(pages * 4096, 16 << 20);
    assert_eq!(pages * (64 << 10), 256 << 20);

    // And on the host running this test the live reader answers
    // something, which is what the watchdog depends on.
    #[cfg(target_os = "linux")]
    assert!(0 < resident_bytes(), "the watchdog measured nothing");
}

/// What one budgeted compile answered.
#[derive(Debug, Default)]
struct Outcome {
    ok: bool,
    names: BTreeSet<String>,
    error: String,
    budget: bool,
    /// With `budget`: the stop was the resident cap, not the wall clock.
    memory: bool,
}

/// How one budgeted compile is scored, in ONE place so the two halves
/// cannot disagree about what a watchdog stop means.
#[derive(Debug, PartialEq, Eq)]
enum Score {
    Budget,
    Accepted,
    Rejected,
}

/// Reads the budget flag BEFORE `ok`. A child stopped by its own watchdog
/// answers `{budget: true, ok: false}`, which is indistinguishable from a
/// refusal to a caller that reads `ok` alone, and a half that reads it
/// alone counts a nontermination as the compiler correctly rejecting the
/// grammar. That was tabnas/abnf#74, open in all three runtimes and fixed
/// here first; the same scorer now stands in `ts/test/conformance.test.js`
/// and `go/conformance_test.go`.
fn score_corpus(result: &Outcome) -> Score {
    if result.budget {
        Score::Budget
    } else if result.ok {
        Score::Accepted
    } else {
        Score::Rejected
    }
}

/// The scorer, pinned. Cheap to assert, and it is what the sweep means by
/// "never scored a pass".
#[test]
fn a_watchdog_stop_is_never_scored_as_a_rejection_or_a_pass() {
    let budget = Outcome {
        budget: true,
        ..Outcome::default()
    };
    assert_eq!(score_corpus(&budget), Score::Budget);
    let accepted = Outcome {
        ok: true,
        ..Outcome::default()
    };
    assert_eq!(score_corpus(&accepted), Score::Accepted);
    let rejected = Outcome {
        error: "no".to_string(),
        ..Outcome::default()
    };
    assert_eq!(score_corpus(&rejected), Score::Rejected);
}

/// How one budgeted child ended.
///
/// The distinction this type exists to make: "the compiler did not
/// finish" and "the child died" are different facts, and only the first
/// is a measurement. Collapsing them is how a compiler that ABORTS on an
/// invalid grammar reads as a compiler that rejects one.
enum Finish {
    /// The child ran the compiler to a verdict and wrote it out.
    Completed,
    /// The child's own watchdog stopped it, or the parent's did. `memory`
    /// is true only for the child's resident cap.
    Budget { memory: bool },
    /// Anything else, described for the panic that follows.
    Crashed(String),
}

/// A copy of this test binary, taken once, that the children are run
/// from.
///
/// The parent re-executes itself for every one of the ~1500 budgeted
/// compiles in this sweep, which takes minutes. On Unix a rebuild during
/// that window REPLACES or unlinks the path `current_exe` answers --
/// `cargo test` writes a new `target/debug/deps/conformance_test-<hash>`
/// and relinks -- so later children run a different compiler from
/// earlier ones, or fail to spawn at all (`Os { code: 2, kind:
/// NotFound }`, seen in practice and green on a re-run). One conformance
/// run must measure one artifact, so the artifact is pinned before the
/// first child starts.
struct PinnedBinary(PathBuf);

impl PinnedBinary {
    fn new() -> Self {
        let exe = std::env::current_exe().expect("the test binary has a path");
        let pinned = std::env::temp_dir().join(format!(
            "tabnas-abnf-conformance-exe-{}{}",
            std::process::id(),
            std::env::consts::EXE_SUFFIX
        ));
        let _ = fs::remove_file(&pinned);
        fs::copy(&exe, &pinned).unwrap_or_else(|error| {
            panic!(
                "could not pin the test binary {} for the sweep: {error}",
                exe.display()
            )
        });
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&pinned, fs::Permissions::from_mode(0o755))
                .expect("the pinned binary is executable");
        }
        Self(pinned)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PinnedBinary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Run one corpus compile in its own process, under the budget.
fn compile_budgeted(exe: &Path, rel: &str, append: &str) -> Outcome {
    let stem = format!(
        "tabnas-abnf-conformance-{}-{}",
        std::process::id(),
        rel.replace('/', "_")
    );
    let out = std::env::temp_dir().join(format!("{stem}.json"));
    let errors = std::env::temp_dir().join(format!("{stem}.err"));
    let _ = fs::remove_file(&out);
    // A FILE rather than a pipe: the parent polls `try_wait` instead of
    // draining, and a child that filled a pipe buffer with panic output
    // would block there for ever.
    let error_log = fs::File::create(&errors).expect("the child's error log is writable");
    let mut child = Command::new(exe)
        .args([
            "--exact",
            "conformance_child",
            "--ignored",
            "--test-threads=1",
            // So that a panic message, and anything the child printed
            // on its way down, reaches the error log this parent reads
            // when the child fails. libtest captures both by default,
            // and a crash report nobody can see is half a diagnostic.
            "--nocapture",
        ])
        .env(ENV_FILE, corpus_dir().join(rel))
        .env(ENV_APPEND, append)
        .env(ENV_OUT, &out)
        .stdout(Stdio::null())
        .stderr(Stdio::from(error_log))
        .spawn()
        .unwrap_or_else(|error| {
            // Deliberately fatal rather than counted as a budget
            // failure: "the child could not start" and "the compiler
            // did not finish" are different facts, and reporting the
            // first as the second would move a row in known-gaps.tsv
            // for a reason that has nothing to do with the compiler.
            panic!(
                "could not start the budgeted child {}: {error}\n  \
                 The sweep runs a PINNED COPY of this test binary, so this is \
                 no longer a rebuild racing the sweep.",
                exe.display()
            )
        });

    let deadline = Instant::now() + BUDGET + Duration::from_secs(10);
    let finish = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Finish::Completed,
            Ok(Some(status)) if Some(EXIT_BUDGET) == status.code() => {
                break Finish::Budget { memory: false }
            }
            Ok(Some(status)) if Some(EXIT_BUDGET_MEMORY) == status.code() => {
                break Finish::Budget { memory: true }
            }
            Ok(Some(status)) => {
                break Finish::Crashed(format!(
                    "the child ended with {status}{}",
                    if Some(EXIT_UNREADABLE) == status.code() {
                        " (it could not read the grammar)"
                    } else {
                        ""
                    }
                ))
            }
            // The parent's own watchdog, ten seconds past the child's.
            // A child that outlives its own clock is still the compiler
            // failing to finish, so this stays a budget failure.
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break Finish::Budget { memory: false };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => break Finish::Crashed(format!("could not wait for the child: {error}")),
        }
    };

    let parsed = fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let _ = fs::remove_file(&out);
    let crash = match finish {
        Finish::Budget { memory } => {
            let _ = fs::remove_file(&errors);
            return Outcome {
                budget: true,
                memory,
                ..Outcome::default()
            };
        }
        Finish::Completed if parsed.is_some() => None,
        Finish::Completed => Some(
            "the child exited 0 without writing a readable result, which is \
             a harness fault and not a measurement"
                .to_string(),
        ),
        Finish::Crashed(what) => Some(what),
    };
    if let Some(what) = crash {
        // NOT `budget: true`. A crash scored as budget exhaustion is
        // scored as the compiler REJECTING the grammar, because the
        // invalid half reads `!ok` -- so a regression that aborts on
        // invalid input would leave this suite green. Fail the parent
        // and say which grammar did it.
        let log = fs::read_to_string(&errors).unwrap_or_default();
        let _ = fs::remove_file(&errors);
        panic!(
            "the budgeted child for {rel:?}{} FAILED: {what}\n  \
             This is not budget exhaustion, and it must not be counted as one: \
             only the child's own 256MB/60s watchdog (exit {EXIT_BUDGET} or {EXIT_BUDGET_MEMORY}) is.\n  \
             The child said:\n{}",
            if append.is_empty() {
                String::new()
            } else {
                format!(" with the mutation {append:?}")
            },
            if log.trim().is_empty() {
                "    (nothing)".to_string()
            } else {
                log.lines()
                    .map(|line| format!("    {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        );
    }
    let _ = fs::remove_file(&errors);
    let parsed = parsed.expect("a completed child wrote a result");
    Outcome {
        ok: parsed["ok"].as_bool().unwrap_or(false),
        names: parsed["names"]
            .as_array()
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| name.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        error: parsed["error"].as_str().unwrap_or_default().to_string(),
        budget: false,
        memory: false,
    }
}

/// Every name the compiled spec can reach: rules, fixed tokens, match
/// tokens.
fn spec_names(spec: &tabnas_abnf::GrammarSpec) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut add = |key: &str| {
        seen.insert(key.trim_start_matches('#').to_lowercase());
    };
    for name in spec.rule.keys() {
        add(name);
    }
    for section in ["fixed", "match"] {
        if let Some(tokens) = spec
            .options
            .get(section)
            .and_then(|group| group.get("token"))
            .and_then(serde_json::Value::as_object)
        {
            for key in tokens.keys() {
                add(key);
            }
        }
    }
    seen
}

// ---- the corpus ------------------------------------------------------

/// RFC 5234 section 4: `rule = rulename defined-as elements c-nl`. A
/// rulename starts in column 1 and `=` or `=/` follows.
fn declared_rules(src: &str) -> Vec<String> {
    let pattern =
        Regex::new(r"(?m)^([A-Za-z][A-Za-z0-9-]*)[ \t]*=/?").expect("the pattern is valid");
    let mut seen = BTreeSet::new();
    for captures in pattern.captures_iter(src) {
        seen.insert(captures[1].to_lowercase());
    }
    seen.into_iter().collect()
}

/// RFC 5234 lines are CRLF-terminated.
fn apply_mutation(base: &str, append: &str) -> String {
    let trimmed = base.trim_end_matches(['\n', '\r']);
    format!("{trimmed}\r\n{append}\r\n")
}

fn load_corpus_tsv(name: &str) -> Vec<Vec<String>> {
    let path = repo_root().join("test").join("corpus").join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("missing {}: {error}{FETCH_HINT}", path.display()));
    raw.lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim_end_matches('\r')
                .split('\t')
                .map(str::to_string)
                .collect()
        })
        .collect()
}

fn read_corpus_file(rel: &str) -> String {
    fs::read_to_string(corpus_dir().join(rel))
        .unwrap_or_else(|error| panic!("corpus file missing: {rel}: {error}{FETCH_HINT}"))
}

/// Fetch the corpus when it is not on disk. `cargo test` has no
/// `pretest` hook (the TypeScript side gets one from `package.json`) and
/// the shared CI workflow lives in another repository, so the fetch
/// happens here. The script is idempotent, so it is a no-op once the
/// corpus is at its pinned commits. A fetch that cannot run leaves the
/// assertions below to FAIL LOUDLY.
fn ensure_corpus() {
    if corpus_dir().is_dir() {
        return;
    }
    let script = repo_root().join("test").join("fetch-abnf-corpus.sh");
    if !script.is_file() {
        return;
    }
    let status = Command::new("sh")
        .arg(&script)
        .current_dir(repo_root())
        .status();
    if !matches!(status, Ok(status) if status.success()) {
        eprintln!(
            "WARNING: {} failed; the conformance test will FAIL, not skip.",
            script.display()
        );
    }
}

fn count_abnf_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                count_abnf_files(&path)
            } else {
                usize::from(path.extension().is_some_and(|ext| "abnf" == ext))
            }
        })
        .sum()
}

// ---- the sweep -------------------------------------------------------

/// One test, not many, because the three halves share one expensive
/// sweep and the mutation half needs to know which bases the compiler
/// could not finish at all.
#[test]
fn conformance() {
    ensure_corpus();

    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    let mut fragment = Vec::new();
    for row in load_corpus_tsv("manifest.tsv") {
        match row[1].as_str() {
            "valid" => valid.push(row[0].clone()),
            "invalid" => invalid.push(row[0].clone()),
            "fragment" => fragment.push(row[0].clone()),
            other => panic!("manifest row {:?} has unknown class {other:?}", row[0]),
        }
    }
    let mutations = load_corpus_tsv("mutations.tsv");
    assert!(
        50 <= valid.len() && 10 <= invalid.len() && 13 <= mutations.len(),
        "degenerate corpus: {} valid / {} invalid / {} mutation classes{FETCH_HINT}",
        valid.len(),
        invalid.len(),
        mutations.len()
    );

    let mut pinned_valid = BTreeSet::new();
    let mut pinned_invalid = BTreeSet::new();
    let mut pinned_budget = BTreeSet::new();
    // `budget-timing`: an invalid-half grammar whose compile time sits near
    // the 60s budget, so whether it finishes depends on the host. A
    // wall-clock stop of it is not asserted; a resident-cap stop is, and so
    // is finishing in under half the budget, which means the row is stale.
    let mut pinned_timing: BTreeMap<String, String> = BTreeMap::new();
    let mut pinned_leaks: BTreeMap<String, usize> = BTreeMap::new();
    for row in load_corpus_tsv("known-gaps.tsv") {
        if "rust" != row[0] {
            continue;
        }
        match row[1].as_str() {
            "valid-not-accepted" => {
                pinned_valid.insert(row[2].clone());
            }
            "invalid-accepted" => {
                pinned_invalid.insert(row[2].clone());
            }
            "budget-exceeded" => {
                pinned_budget.insert(row[2].clone());
            }
            "budget-timing" => {
                pinned_timing.insert(row[2].clone(), row[4].clone());
            }
            "mutation-leak" => {
                let count = row[3].parse::<usize>().unwrap_or_else(|_| {
                    panic!(
                        "known-gaps.tsv: mutation-leak {:?} has a non-numeric count {:?}",
                        row[2], row[3]
                    )
                });
                pinned_leaks.insert(row[2].clone(), count);
            }
            other => panic!("known-gaps.tsv row {:?} has unknown kind {other:?}", row[2]),
        }
    }
    for key in pinned_timing.keys() {
        assert!(
            invalid.contains(key),
            "known-gaps.tsv: budget-timing {key:?} is not an invalid-half grammar in manifest.tsv; \
             only a grammar every runtime rejects may have its budget outcome left open"
        );
        assert!(
            !pinned_budget.contains(key),
            "known-gaps.tsv: {key:?} is pinned both budget-exceeded and budget-timing; keep one"
        );
    }

    assert!(
        corpus_dir().is_dir(),
        "ABNF conformance corpus missing at {}{FETCH_HINT}",
        corpus_dir().display()
    );
    let files = count_abnf_files(&corpus_dir());
    assert!(
        60 <= files,
        "expected at least 60 .abnf corpus files, found {files}{FETCH_HINT}"
    );
    for row in load_corpus_tsv("manifest.tsv") {
        assert!(
            corpus_dir().join(&row[0]).is_file(),
            "the manifest names a corpus file that is not there: {}{FETCH_HINT}",
            row[0]
        );
    }

    let record = matches!(std::env::var(ENV_RECORD).as_deref(), Ok("1"));
    let mut recorded: Vec<String> = Vec::new();
    let mut note = |kind: &str, key: &str, count: usize, text: &str| {
        recorded.push(format!("rust\t{kind}\t{key}\t{count}\t{text}"));
    };
    // A budget-timing row is a declaration about the host, not a measurement,
    // so recording carries it over unchanged whichever way this run went.
    if record {
        for (key, text) in &pinned_timing {
            note("budget-timing", key, 1, text);
        }
    }

    let mut valid_gaps = BTreeSet::new();
    let mut invalid_gaps = BTreeSet::new();
    let mut over_budget = BTreeSet::new();

    // One sweep measures ONE compiler artifact. The copy is taken before
    // the first child starts and removed when this binding drops, the
    // assertions below included, because they unwind.
    let pinned = PinnedBinary::new();
    let exe = pinned.path();

    // --- half 1: valid grammars compile AND yield every declared rule ---
    for rel in &valid {
        let result = compile_budgeted(exe, rel, "");
        if Score::Budget == score_corpus(&result) {
            over_budget.insert(rel.clone());
            valid_gaps.insert(rel.clone());
            if record {
                note(
                    "budget-exceeded",
                    rel,
                    1,
                    "compiler does not terminate within 256MB / 60s",
                );
                note("valid-not-accepted", rel, 1, "budget exceeded");
            }
            continue;
        }
        if !result.ok {
            valid_gaps.insert(rel.clone());
            if record {
                note(
                    "valid-not-accepted",
                    rel,
                    1,
                    &format!("rejected: {}", result.error),
                );
            }
            continue;
        }
        let missing: Vec<String> = declared_rules(&read_corpus_file(rel))
            .into_iter()
            .filter(|name| !result.names.contains(name))
            .collect();
        if !missing.is_empty() {
            valid_gaps.insert(rel.clone());
            if record {
                note(
                    "valid-not-accepted",
                    rel,
                    1,
                    &format!(
                        "compiled, but declared rules vanished: {}",
                        missing.join(",")
                    ),
                );
            }
        }
    }

    // --- half 2a: corpus grammars the oracle rejects must be rejected ---
    //
    // A compile that never finishes is NOT a rejection. This half scores
    // on `!ok`, and a child stopped by its own watchdog answers
    // `{budget: true, ok: false}`, so testing `.ok` alone read a
    // nontermination as the compiler correctly refusing the grammar and
    // left the suite green through exactly the regression it exists to
    // catch. Budget exhaustion is therefore recorded in `over_budget`,
    // as the valid half records it, and counted out of the dial rather
    // than into it.
    //
    // It does NOT join `invalid_gaps`: that set is pinned against the
    // `invalid-accepted` rows of known-gaps.tsv and means "the compiler
    // accepted this", which is the opposite claim. `over_budget` is
    // pinned against the `budget-exceeded` rows, which is the claim
    // being made, and a new member of that set fails the assertion below
    // whichever half it came from.
    let mut invalid_over_budget = BTreeSet::new();
    // `budget-timing` grammars this run stopped on the WALL CLOCK: the one
    // outcome such a row waives. A resident-cap stop is not waived.
    let mut timing_waived = BTreeSet::new();
    // `budget-timing` grammars that finished in under half the budget: the
    // slowdown the row waives is gone, so the row must go too.
    let mut timing_stale = Vec::new();
    for rel in &invalid {
        let started = Instant::now();
        let result = compile_budgeted(exe, rel, "");
        let elapsed = started.elapsed();
        let score = score_corpus(&result);
        let timing = pinned_timing.contains_key(rel);
        if Score::Budget == score {
            over_budget.insert(rel.clone());
            invalid_over_budget.insert(rel.clone());
            // Waived: a budget-timing row, stopped on the wall clock.
            let waived = timing && !result.memory;
            if waived {
                timing_waived.insert(rel.clone());
            }
            if record && !waived {
                note(
                    "budget-exceeded",
                    rel,
                    1,
                    "compiler does not terminate within 256MB / 60s",
                );
            }
            continue;
        }
        if timing && elapsed < BUDGET / 2 {
            timing_stale.push(format!(
                "known-gaps.tsv: budget-timing {rel:?} finished in {:.1}s, under half the {}s \
                 budget. The slowdown that row waives is gone: delete the row.",
                elapsed.as_secs_f64(),
                BUDGET.as_secs()
            ));
        }
        if Score::Accepted == score {
            invalid_gaps.insert(rel.clone());
            if record {
                note(
                    "invalid-accepted",
                    rel,
                    1,
                    "accepted; the oracle rejects it",
                );
            }
        }
    }

    // --- half 2b: mutants violating a named RFC 5234 production ---------
    //
    // Bases the compiler cannot finish at all are excluded here and ONLY
    // here: a mutant of a base that never compiles measures nothing about
    // the mutation. That exclusion is pinned by name, so it cannot
    // quietly grow.
    let bases: Vec<&String> = valid
        .iter()
        .filter(|rel| !over_budget.contains(*rel))
        .collect();
    let mut mutation_leaks: BTreeMap<String, usize> = BTreeMap::new();
    for mutation in &mutations {
        let (name, append) = (&mutation[0], &mutation[1]);
        let mut leaked = 0;
        for rel in &bases {
            let src = apply_mutation(&read_corpus_file(rel), append);
            if abnf_convert(&src, None).is_ok() {
                leaked += 1;
            }
        }
        if 0 < leaked {
            mutation_leaks.insert(name.clone(), leaked);
            if record {
                note(
                    "mutation-leak",
                    name,
                    leaked,
                    &format!("{leaked}/{} bases accepted `{append}`", bases.len()),
                );
            }
        }
    }

    // --- the dial: what was actually measured ---------------------------
    let mutant_total = bases.len() * mutations.len();
    let mutant_leaks: usize = mutation_leaks.values().sum();
    // A grammar the compiler could not finish is subtracted from the
    // invalid half rather than added to it: it was neither accepted nor
    // rejected, so it is reported on its own line and nowhere else.
    println!(
        "\n  ABNF conformance dial (Rust), as measured by this run:\
         \n    valid   accepted + value-correct : {}/{}\
         \n    invalid rejected                 : {}/{}\
         \n    excluded fragments               : {}\
         \n    over budget (never scored a pass): {} ({} valid, {} invalid)",
        valid.len() - valid_gaps.len(),
        valid.len(),
        invalid.len() - invalid_gaps.len() - invalid_over_budget.len() + mutant_total
            - mutant_leaks,
        invalid.len() + mutant_total,
        fragment.len(),
        over_budget.len(),
        over_budget.len() - invalid_over_budget.len(),
        invalid_over_budget.len()
    );

    if record {
        println!(
            "\n# paste the `rust` rows of test/corpus/known-gaps.tsv:\n{}",
            recorded.join("\n")
        );
        return;
    }

    let mut failures = Vec::new();
    assert_set_equal(
        &mut failures,
        "valid RFC 5234 grammars this compiler does not fully accept",
        &valid_gaps,
        &pinned_valid,
    );
    assert_set_equal(
        &mut failures,
        "grammars the compiler cannot finish within 256MB / 60s",
        // A budget-timing grammar may land on either side of the WALL CLOCK
        // on a given host, so a wall-clock stop of one is left out here. A
        // resident-cap stop is not, and it is still scored above like every
        // invalid grammar, so accepting it fails as usual.
        &over_budget
            .iter()
            .filter(|rel| !timing_waived.contains(*rel))
            .cloned()
            .collect(),
        &pinned_budget,
    );
    failures.extend(timing_stale);
    assert_set_equal(
        &mut failures,
        "non-RFC-5234 corpus grammars this compiler accepts",
        &invalid_gaps,
        &pinned_invalid,
    );

    for (name, got) in &mutation_leaks {
        match pinned_leaks.get(name) {
            Some(want) if want == got => {}
            want => failures.push(format!(
                "mutation class {name:?} now leaks {got} bases, known-gaps.tsv pins {want:?}. \
                 Lower is better; update test/corpus/known-gaps.tsv when you improve one."
            )),
        }
    }
    for (name, want) in &pinned_leaks {
        if !mutation_leaks.contains_key(name) {
            failures.push(format!(
                "mutation class {name:?} no longer leaks (known-gaps.tsv pins {want}). \
                 If you fixed it, delete its row."
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn assert_set_equal(
    failures: &mut Vec<String>,
    what: &str,
    got: &BTreeSet<String>,
    want: &BTreeSet<String>,
) {
    if got == want {
        return;
    }
    failures.push(format!(
        "the set of {what} has changed.\n  measured now: {got:?}\n  known-gaps.tsv: {want:?}\n  \
         If you FIXED one, delete its row from test/corpus/known-gaps.tsv. If you BROKE one, that \
         is a regression. Never edit a row to silence a failure you did not fix."
    ));
}
