#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// The original VI-0040 reproduction is Sail's pinned abstract-domains package.
// Its unchanged proof query is unsat with stock's preceding context, unknown
// (incomplete quantifiers) with the fork's, and unsat when only the two earlier
// spec termination checks are removed from the latter. These smaller controls
// exercise the generic isolation and rejection behavior, not that full proof.
fn logged(code: String, name: &str) -> Vec<String> {
    let logs = tempfile::tempdir().expect("SMT log directory");
    let directory = format!("--sail-raw={}", logs.path().display());
    let options =
        ["vstd", "--sail-raw=--log", "--sail-raw=smt", "--sail-raw=--log-dir", directory.as_str()];
    let result = verify_one_file(name, code, &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let mut files: Vec<_> = std::fs::read_dir(logs.path())
        .expect("SMT logs")
        .map(|entry| entry.expect("SMT log entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "smt2"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no actual solver input recorded");
    files.into_iter().map(|path| std::fs::read_to_string(path).expect("SMT log")).collect()
}

#[test]
fn sail_vi0040_spec_termination_has_a_separate_solver_history() {
    let code = verus_code_str! {
        pub open spec fn down(n: nat) -> nat decreases n {
            if n == 0 { 0 } else { down((n - 1) as nat) }
        }
        proof fn check() {
            reveal_with_fuel(down, 2);
            assert(down(1) == 0);
        }
    };
    let logs =
        logged(code.to_owned(), "sail_vi0040_spec_termination_has_a_separate_solver_history");
    let mut isolated = 0;
    let mut ordinary = 0;
    for text in &logs {
        if text.contains(";; Spec-Termination ") {
            isolated += 1;
            assert!(text.contains("query spun off because: spec_termination"));
            assert!(text.contains("spec%param%"));
            assert_eq!(text.matches("(check-sat)").count(), 1);
            assert!(!text.contains(";; Function-Def test_crate::check"));
        } else if text.contains(";; Function-Def test_crate::check") {
            ordinary += 1;
            assert!(!text.contains("query spun off because:"));
            assert!(text.contains("(check-sat)"));
        }
    }
    assert_eq!(isolated, 1, "one actual recursive spec termination check");
    assert_eq!(ordinary, 1, "the ordinary proof keeps its own existing context");
}

#[test]
fn sail_vi0040_nonrecursive_specs_keep_the_existing_solver_context() {
    let code = verus_code_str! {
        pub open spec fn identity(n: nat) -> nat { n }
        proof fn check() { assert(identity(1) == 1); }
    };
    let logs =
        logged(code.to_owned(), "sail_vi0040_nonrecursive_specs_keep_the_existing_solver_context");
    assert!(logs.iter().any(|text| text.contains("(check-sat)")));
    assert!(logs.iter().all(|text| !text.contains("query spun off because:")));
}

#[test]
fn sail_vi0040_one_companion_receives_later_definitions() {
    let code = verus_code_str! {
        pub open spec fn first(n: nat) -> nat decreases n {
            if n == 0 { 0 } else { first((n - 1) as nat) }
        }
        pub open spec fn measure(n: nat) -> nat { n + first(n) * 0 }
        pub open spec fn second(n: nat) -> nat decreases measure(n) {
            if n == 0 { 0 } else { second((n - 1) as nat) }
        }
        proof fn check() {
            reveal_with_fuel(first, 2);
            reveal_with_fuel(second, 2);
            assert(first(1) == 0);
            assert(second(1) == 0);
        }
    };
    let logs = logged(code.to_owned(), "sail_vi0040_one_companion_receives_later_definitions");
    let isolated: Vec<_> =
        logs.iter().filter(|text| text.contains(";; Spec-Termination ")).collect();
    assert_eq!(isolated.len(), 1, "termination checks share one extra solver stream");
    let text = isolated[0];
    assert!(text.contains("query spun off because: spec_termination"));
    assert_eq!(text.matches("(check-sat)").count(), 2);
    let first = text.find(";; Spec-Termination test_crate::first").expect("first termination");
    let later = text.find(";; Function-Axioms test_crate::measure").expect("later definition");
    let second = text.find(";; Spec-Termination test_crate::second").expect("second termination");
    assert!(
        first < later && later < second,
        "the companion must receive newly admitted definitions"
    );
}

test_verify_one_file! {
    #[test] sail_vi0040_a_false_termination_check_still_fails verus_code! {
        spec fn spin(n: nat) -> nat decreases n {
            spin(n) // FAILS
        }
    } => Err(err) => assert_one_fails(err)
}

// Set the proxy only on this child, so parallel source-suite tests keep their
// original solver. It runs the actual selected Z3 (or Sail shim) and changes
// only its version response; no proof answer or input command is fabricated.
#[cfg(unix)]
#[test]
fn sail_vi0040_spec_only_still_rejects_a_solver_version_mismatch() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    assert!(std::env::var_os("VERUS_IN_VARGO").is_some());
    let executable = std::env::current_exe().expect("test executable");
    let profile = executable.parent().expect("deps").parent().expect("profile");
    let runtime = profile
        .ancestors()
        .nth(2)
        .expect("source")
        .join("target-verus")
        .join(profile.file_name().expect("profile name"));
    let solver = std::env::var_os("VERUS_Z3_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("../z3"));
    let solver = std::path::absolute(solver).expect("absolute selected solver path");
    let directory = tempfile::tempdir().expect("version fixture");
    let source = directory.path().join("spec.rs");
    std::fs::write(
        &source,
        "use vstd::prelude::*; verus! { pub open spec fn down(n:nat) -> nat decreases n { if n == 0 { 0 } else { down((n-1) as nat) } } }",
    )
    .expect("recursive-spec-only source");
    let proxy = directory.path().join("version-proxy.py");
    std::fs::write(
        &proxy,
        r#"#!/usr/bin/env python3
import os, subprocess, sys, threading
child = subprocess.Popen([os.environ['SAIL_SPEC_TEST_Z3'], *sys.argv[1:]],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def feed():
    try:
        for line in sys.stdin.buffer:
            child.stdin.write(line)
            child.stdin.flush()
        child.stdin.close()
    except BrokenPipeError:
        pass
threading.Thread(target=feed, daemon=True).start()
try:
    for line in child.stdout:
        if line.startswith(b'(:version '):
            line = b'(:version "0.0.0")\n'
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
except BrokenPipeError:
    child.terminate()
sys.exit(child.wait())
"#,
    )
    .expect("selected-solver proxy");
    std::fs::set_permissions(&proxy, std::fs::Permissions::from_mode(0o755))
        .expect("executable proxy");
    for check_version in [true, false] {
        let mut command = Command::new(runtime.join("verus"));
        command
            .arg(&source)
            .args(["--crate-type=lib", "--no-report-long-running"])
            .env("VERUS_Z3_PATH", &proxy)
            .env("SAIL_SPEC_TEST_Z3", &solver)
            .env("RUST_BACKTRACE", "0");
        if !check_version {
            command.args(["-V", "no-solver-version-check"]);
        }
        let output = command.output().expect("actual verifier invocation");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if check_version {
            assert!(!output.status.success(), "{}\n{}", stdout, stderr);
            assert!(stderr.contains("version") && stderr.contains("0.0.0"), "{}", stderr);
        } else {
            assert!(output.status.success(), "{}\n{}", stdout, stderr);
            assert!(stdout.contains("1 verified, 0 errors"), "{}", stdout);
        }
    }
}
