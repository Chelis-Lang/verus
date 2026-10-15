#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0019: closure types were named after
// rustc's crate-wide `DefIndex` (`anonymous_closure%<N>`), so inserting any
// item renumbered every later closure in the crate and changed the SMT input
// of modules the edit did not touch. A closure's name must depend only on its
// own definition path.

const M1_BEFORE: &str = verus_code_str! {
    mod m1 {
        pub fn a() {}
    }
};

// The same module with one more item, which shifts every later `DefIndex`.
const M1_AFTER: &str = verus_code_str! {
    mod m1 {
        pub fn a() {}
        pub fn inserted() {
            let g = |x: u8| -> (r: u8) ensures r == x { x };
            let _ = g(1);
        }
    }
};

const M2: &str = verus_code_str! {
    mod m2 {
        use vstd::prelude::*;
        pub fn b() -> (out: u8)
            ensures out == 3
        {
            let f = |x: u8| -> (r: u8) ensures r == x { x };
            f(3)
        }
    }
};

// The SMT log of module `m2`, without `;;` comment lines (they hold source
// positions, which an insertion above shifts).
fn m2_smt(m1: &str, logs: &std::path::Path) -> String {
    let log_dir = format!("--sail-raw={}", logs.display());
    let options = [
        "vstd",
        "--crate-name=sail_vi0019",
        "--sail-raw=--log",
        "--sail-raw=smt",
        "--sail-raw=--log-dir",
        log_dir.as_str(),
    ];
    let code = m1.to_string() + M2;
    let result = verify_one_file("sail_vi0019_closure_ids", code, &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let mut files: Vec<_> = std::fs::read_dir(logs)
        .expect("log directory")
        .map(|e| e.expect("log entry").path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with("m2") && name.ends_with(".smt2")
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no SMT log for m2 in {}", logs.display());
    let mut smt = String::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("SMT log");
        for line in text.lines().filter(|l| !l.trim_start().starts_with(";;")) {
            smt.push_str(line);
            smt.push('\n');
        }
    }
    smt
}

#[test]
fn sail_vi0019_inserted_item_keeps_other_modules_closure_names() {
    let before = tempfile::tempdir().expect("log directory");
    let after = tempfile::tempdir().expect("log directory");
    let smt_before = m2_smt(M1_BEFORE, before.path());
    let smt_after = m2_smt(M1_AFTER, after.path());
    assert!(
        smt_before.contains("anonymous_closure%"),
        "m2's SMT declares no closure type; the test no longer exercises closure names"
    );
    // On stock Verus m2's closure is renamed (for example
    // `anonymous_closure%13` becoming `anonymous_closure%16`).
    assert!(
        smt_before == smt_after,
        "inserting an item in m1 changed m2's SMT:\n{}",
        first_difference(&smt_before, &smt_after)
    );
}

fn first_difference(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n- {}\n+ {}", i + 1, x, y);
        }
    }
    format!("lengths differ: {} and {} lines", a.lines().count(), b.lines().count())
}
