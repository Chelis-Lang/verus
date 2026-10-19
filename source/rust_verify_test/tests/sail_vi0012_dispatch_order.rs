#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for cost-ordered bucket dispatch (Sail #244):
// parallel workers take the buckets estimated to be most expensive first.
// Only the dispatch order may change. Errors stay in bucket order (VI-0007),
// "verifying <bucket>" notes stay in bucket order (VI-0012), and every
// query is byte-identical to a single-threaded run. Here the last module is
// the largest, so it is dispatched first.

fn code() -> String {
    let mut code = String::from("::verus_builtin_macros::verus! {\n");
    for i in 1..=7 {
        code.push_str(&format!("mod m{i} {{ proof fn fail{i}() {{ assert(false); }} }}\n"));
    }
    code.push_str("mod m8 {\nuse vstd::prelude::*;\n");
    for j in 0..12 {
        code.push_str(&format!(
            "proof fn w{j}(a: int, b: int, c: int) {{ assert((a + b + {j}) * c == a * c + b * c + {j} * c) by (nonlinear_arith); }}\n"
        ));
    }
    code.push_str("proof fn fail8() { assert(false); }\n}\n}\n");
    code
}

#[test]
fn sail_vi0012_dispatch_order_keeps_error_order() {
    for round in 0..3 {
        let result = verify_one_file(
            "sail_vi0012_dispatch_order_keeps_error_order",
            code(),
            &["--sail-raw=--num-threads", "--sail-raw=2"],
        );
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("expected eight errors"),
        };
        assert_eq!(err.errors.len(), 8, "round {round}: {:?}", err.errors);
        let lines: Vec<usize> =
            err.errors.iter().map(|e| relevant_error_span(&e.spans).line_start).collect();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted, "round {round}: errors not in module order");
    }
}

#[test]
fn sail_vi0012_dispatch_order_keeps_start_note_order() {
    let modules: Vec<String> = (1..=8).map(|i| format!("--sail-raw=m{i}")).collect();
    let mut options = vec!["--no-report-long-running", "--sail-raw=--num-threads", "--sail-raw=2"];
    for module in &modules {
        options.push("--sail-raw=--verify-module");
        options.push(module.as_str());
    }
    let expected: Vec<String> = (1..=8).map(|i| format!("verifying module m{i}")).collect();
    for round in 0..5 {
        let result =
            verify_one_file("sail_vi0012_dispatch_order_keeps_start_note_order", code(), &options);
        let diags = match result {
            Err(err) => err,
            Ok(_) => panic!("expected eight errors"),
        };
        let notes: Vec<String> = diags
            .notes
            .iter()
            .filter(|n| n.message.starts_with("verifying "))
            .map(|n| n.message.clone())
            .collect();
        assert_eq!(notes, expected, "round {round}: bucket start notes out of order");
    }
}

fn smt_logs(threads: &str, logs: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let log_dir = format!("--sail-raw={}", logs.display());
    let threads = format!("--sail-raw={threads}");
    let options = [
        "--sail-raw=--num-threads",
        threads.as_str(),
        "--sail-raw=--log",
        "--sail-raw=smt",
        "--sail-raw=--log-dir",
        log_dir.as_str(),
    ];
    let _ = verify_one_file("sail_vi0012_dispatch_order_smt", code(), &options);
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(logs)
        .expect("log directory")
        .map(|e| e.expect("log entry").path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("smt2"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&p).expect("SMT log"))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn sail_vi0012_dispatch_order_keeps_queries() {
    let one = tempfile::tempdir().expect("log directory");
    let four = tempfile::tempdir().expect("log directory");
    let sequential = smt_logs("1", one.path());
    let parallel = smt_logs("4", four.path());
    assert!(sequential.len() >= 8, "expected an SMT log per module: {}", sequential.len());
    let names: Vec<_> = sequential.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(names, parallel.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
    for ((name, a), (_, b)) in sequential.iter().zip(parallel.iter()) {
        assert!(a == b, "{name}: SMT differs between 1 and 4 threads");
    }
}
