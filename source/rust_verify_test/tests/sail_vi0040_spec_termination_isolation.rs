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
    assert!(result.is_ok(), "verification failed: {result:?}");
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
        open spec fn down(n: nat) -> nat decreases n {
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
        open spec fn identity(n: nat) -> nat { n }
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
        open spec fn first(n: nat) -> nat decreases n {
            if n == 0 { 0 } else { first((n - 1) as nat) }
        }
        open spec fn measure(n: nat) -> nat { n + first(n) * 0 }
        open spec fn second(n: nat) -> nat decreases measure(n) {
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
    let later = text.find(";; Function-Def test_crate::measure").expect("later definition");
    let second = text.find(";; Spec-Termination test_crate::second").expect("second termination");
    assert!(
        first < later && later < second,
        "the companion must receive newly admitted definitions"
    );
}

test_verify_one_file! {
    #[test] sail_vi0040_a_false_termination_check_still_fails => verus_code! {
        spec fn spin(n: nat) -> nat decreases n { spin(n) }
    } => Err(err) => assert_one_fails(err)
}
