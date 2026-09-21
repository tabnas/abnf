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
//     terminate in this compiler, in any runtime. Exceeding the budget
//     is recorded as a failure to accept, never as a pass and never as a
//     skip.
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
            if BUDGET_BYTES < resident_bytes() || Instant::now() > deadline {
                std::process::exit(3);
            }
        }
    });

    let mut src = fs::read_to_string(&file).unwrap_or_else(|error| {
        eprintln!("{file}: {error}");
        std::process::exit(2);
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

/// This process's resident size in bytes, or 0 where the platform does
/// not say.
fn resident_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let Ok(statm) = fs::read_to_string("/proc/self/statm") else {
            return 0;
        };
        statm
            .split_whitespace()
            .nth(1)
            .and_then(|pages| pages.parse::<u64>().ok())
            .map_or(0, |pages| pages * 4096)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// What one budgeted compile answered.
#[derive(Debug, Default)]
struct Outcome {
    ok: bool,
    names: BTreeSet<String>,
    error: String,
    budget: bool,
}

/// Run one corpus compile in its own process, under the budget.
fn compile_budgeted(rel: &str, append: &str) -> Outcome {
    let exe = std::env::current_exe().expect("the test binary has a path");
    let out = std::env::temp_dir().join(format!(
        "tabnas-abnf-conformance-{}-{}.json",
        std::process::id(),
        rel.replace('/', "_")
    ));
    let _ = fs::remove_file(&out);
    let mut child = Command::new(&exe)
        .args([
            "--exact",
            "conformance_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env(ENV_FILE, corpus_dir().join(rel))
        .env(ENV_APPEND, append)
        .env(ENV_OUT, &out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| {
            // Deliberately fatal rather than counted as a budget
            // failure: "the child could not start" and "the compiler
            // did not finish" are different facts, and reporting the
            // first as the second would move a row in known-gaps.tsv
            // for a reason that has nothing to do with the compiler.
            panic!(
                "could not start the budgeted child {}: {error}\n                   The parent re-executes this test binary, so this \
                 usually means it was rebuilt while the sweep was \
                 running.",
                exe.display()
            )
        });

    let deadline = Instant::now() + BUDGET + Duration::from_secs(10);
    let finished = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break false,
        }
    };

    let parsed = fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let _ = fs::remove_file(&out);
    let Some(parsed) = parsed.filter(|_| finished) else {
        return Outcome {
            budget: true,
            ..Outcome::default()
        };
    };
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

    let mut valid_gaps = BTreeSet::new();
    let mut invalid_gaps = BTreeSet::new();
    let mut over_budget = BTreeSet::new();

    // --- half 1: valid grammars compile AND yield every declared rule ---
    for rel in &valid {
        let result = compile_budgeted(rel, "");
        if result.budget {
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
    for rel in &invalid {
        if compile_budgeted(rel, "").ok {
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
    println!(
        "\n  ABNF conformance dial (Rust), as measured by this run:\
         \n    valid   accepted + value-correct : {}/{}\
         \n    invalid rejected                 : {}/{}\
         \n    excluded fragments               : {}\
         \n    over budget (counted as failures): {}",
        valid.len() - valid_gaps.len(),
        valid.len(),
        invalid.len() - invalid_gaps.len() + mutant_total - mutant_leaks,
        invalid.len() + mutant_total,
        fragment.len(),
        over_budget.len()
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
        &over_budget,
        &pinned_budget,
    );
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
