#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0033: closure types and aliased trait
// functions were named after rustc's stable crate ID (and the definition hash
// it seeds). Cargo derives `-C metadata`, and so that ID, from the build
// profile of the crate and of every dependency, so building the proc-macro
// crates at another opt-level changed the solver input. The SMT input must
// not depend on `-C metadata`.

const CODE: &str = verus_code_str! {
    pub trait Size {
        spec fn size(&self) -> nat;
    }

    pub struct Pair {
        pub a: u8,
    }

    impl Size for Pair {
        open spec fn size(&self) -> nat {
            2
        }
    }

    pub fn apply(x: u8) -> (r: u8)
        ensures r == x,
    {
        let f = |y: u8| -> (z: u8) ensures z == y { y };
        f(x)
    }

    proof fn pair_size(p: Pair)
        ensures p.size() == 2,
    {
    }
};

fn smt(metadata: &str, logs: &std::path::Path) -> String {
    let log_dir = format!("--sail-raw={}", logs.display());
    let metadata = format!("--sail-raw=metadata={metadata}");
    let options = [
        "vstd",
        "--crate-name=sail_vi0033",
        "--sail-raw=-C",
        metadata.as_str(),
        "--sail-raw=--log",
        "--sail-raw=smt",
        "--sail-raw=--log-dir",
        log_dir.as_str(),
    ];
    let result = verify_one_file("sail_vi0033_metadata_names", CODE.to_string(), &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let mut files: Vec<_> = std::fs::read_dir(logs)
        .expect("log directory")
        .map(|e| e.expect("log entry").path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("smt2"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no SMT log in {}", logs.display());
    let mut smt = String::new();
    for file in files {
        smt.push_str(&format!("== {}\n", file.file_name().unwrap().to_string_lossy()));
        smt.push_str(&std::fs::read_to_string(&file).expect("SMT log"));
    }
    smt
}

#[test]
fn sail_vi0033_crate_metadata_does_not_change_smt() {
    let a = tempfile::tempdir().expect("log directory");
    let b = tempfile::tempdir().expect("log directory");
    let smt_a = smt("sail0", a.path());
    let smt_b = smt("sail1", b.path());
    assert!(smt_a.contains("anonymous_closure%"), "the fixture no longer declares a closure type");
    assert!(smt_a.contains("%sail%fn%"), "the fixture no longer aliases a trait function");
    // Before the fix both families embedded the stable crate ID, for example
    // `anonymous_closure%9153844881615033698_5343687445435464143.`.
    assert!(
        smt_a == smt_b,
        "-C metadata changed the SMT input:\n{}",
        first_difference(&smt_a, &smt_b)
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
