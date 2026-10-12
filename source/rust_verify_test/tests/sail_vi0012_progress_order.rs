#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0012: with a user filter, each bucket's
// "verifying <bucket>" note is printed immediately, and with several threads
// stock Verus printed the notes in the order the workers reached them, not
// the order in which they took buckets from the queue (module buckets are
// queued sorted by module name). The race is narrow: with 64 trivial modules
// and 16 threads, about one stock run in five misorders, so the test makes
// 20 runs (stock passes all of them about 1% of the time).
#[test]
fn sail_vi0012_bucket_start_notes_in_bucket_order() {
    const MODULES: usize = 64;
    let mut code = String::from("::verus_builtin_macros::verus! {\n");
    for i in 1..=MODULES {
        code.push_str(&format!("mod m{} {{ proof fn p{}() {{ assert(true); }} }}\n", i, i));
    }
    code.push_str("}\n");
    let modules: Vec<String> = (1..=MODULES).map(|i| format!("--sail-raw=m{}", i)).collect();
    let mut options = vec!["--no-report-long-running", "--sail-raw=--num-threads", "--sail-raw=16"];
    for module in &modules {
        options.push("--sail-raw=--verify-module");
        options.push(module.as_str());
    }
    let mut expected: Vec<String> =
        (1..=MODULES).map(|i| format!("verifying module m{}", i)).collect();
    expected.sort();
    for round in 0..20 {
        let result = verify_one_file(
            "sail_vi0012_bucket_start_notes_in_bucket_order",
            code.clone(),
            &options,
        );
        let diags = match result {
            Ok(diags) => diags,
            Err(err) => panic!("verification failed: {:?}", err),
        };
        let notes: Vec<String> = diags
            .notes
            .iter()
            .filter(|n| n.message.starts_with("verifying "))
            .map(|n| n.message.clone())
            .collect();
        assert_eq!(notes, expected, "round {}: bucket start notes out of order", round);
    }
}
