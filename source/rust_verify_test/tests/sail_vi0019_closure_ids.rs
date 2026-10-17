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

// Imported executable trait defaults retain their bodies in exported VIR.
// Their closure DefIndexes are local to each crate and can therefore tie.
const IMPORTED_DEFAULT: &str = verus_code_str! {
    pub trait T {
        fn f(&self, x: u8) -> (r: u8) ensures r == x {
            let c = |y: u8| -> (r: u8) ensures r == y { y };
            c(x)
        }
    }
};

fn checked_run(dir: &std::path::Path, code: &str, options: &[String]) {
    let file = dir.join("test.rs");
    std::fs::write(&file, code).expect("write input");
    let options: Vec<_> = options.iter().map(String::as_str).collect();
    let result = run_verus(&options, dir, &file, true, false);
    assert!(
        result.status.success(),
        "verification failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
}

#[test]
fn sail_vi0019_imported_closure_index_ties_have_stable_order() {
    let root = tempfile::tempdir().expect("test directory");
    let mut imports = Vec::new();
    for name in ["dep_a", "dep_b"] {
        let dir = root.path().join(name);
        std::fs::create_dir(&dir).expect("dependency directory");
        let export = dir.join("export.vir");
        checked_run(
            &dir,
            IMPORTED_DEFAULT,
            &[
                format!("--crate-name={name}"),
                "--compile".to_string(),
                "--sail-raw=--export".to_string(),
                format!("--sail-raw={}", export.display()),
            ],
        );
        imports.extend([
            "--sail-raw=--extern".to_string(),
            format!("--sail-raw={name}={}", dir.join("libtest.rlib").display()),
            "--sail-raw=--import".to_string(),
            format!("--sail-raw={name}={}", export.display()),
        ]);
    }
    let client = verus_code_str! {
        struct S;
        impl dep_a::T for S {}
        impl dep_b::T for S {}
        fn test() -> (r: u8) ensures r == 3 {
            let s = S;
            dep_a::T::f(&s, dep_b::T::f(&s, 3))
        }
    };
    let client = format!("extern crate dep_a; extern crate dep_b;\n{client}");
    let mut first = None;
    for round in 0..16 {
        let dir = root.path().join(format!("client{round}"));
        std::fs::create_dir(&dir).expect("client directory");
        let logs = dir.join("logs");
        let mut options = imports.clone();
        options.extend([
            "--crate-name=sail_vi0019_client".to_string(),
            "--sail-raw=--log".to_string(),
            "--sail-raw=vir".to_string(),
            "--sail-raw=--log".to_string(),
            "--sail-raw=vir-simple".to_string(),
            "--sail-raw=--log-dir".to_string(),
            format!("--sail-raw={}", logs.display()),
        ]);
        checked_run(&dir, &client, &options);
        let vir = std::fs::read_to_string(logs.join("crate.vir")).expect("VIR log");
        let simple =
            std::fs::read_to_string(logs.join("crate-simple.vir")).expect("simplified VIR log");
        let mut ids: Vec<_> = vir
            .split("(ClosureId :name ")
            .skip(1)
            .map(|id| {
                let mut words = id.split_whitespace();
                let name = words.next().expect("closure name").parse::<u64>().expect("number");
                assert_eq!(words.next(), Some(":stable_crate_id"));
                let stable_crate_id =
                    words.next().expect("stable crate ID").parse::<u64>().expect("number");
                assert_eq!(words.next(), Some(":order"));
                let order = words
                    .next()
                    .expect("closure order")
                    .trim_end_matches(')')
                    .parse::<usize>()
                    .expect("number");
                (order, stable_crate_id, name)
            })
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 2, "fixture must retain both foreign closures");
        assert_eq!(ids[0].0, ids[1].0, "fixture must exercise an index tie");
        assert_ne!(ids[0].1, ids[1].1, "the two source-identical crates need distinct identities");
        let actual: Vec<(u64, u64)> = simple
            .lines()
            .filter_map(|line| {
                let rest =
                    line.split("(Datatype :name (Dt Path crate::anonymous_closure%").nth(1)?;
                let name = rest.split(')').next().expect("closure name");
                let (stable_crate_id, local_hash) = name.split_once('_').expect("full closure ID");
                Some((
                    stable_crate_id.parse().expect("crate ID number"),
                    local_hash.parse().expect("local hash number"),
                ))
            })
            .collect();
        assert_eq!(
            actual,
            ids.iter()
                .map(|(_, stable_crate_id, name)| (*stable_crate_id, *name))
                .collect::<Vec<_>>()
        );
        if let Some(expected) = &first {
            assert_eq!(&actual, expected, "closure declaration order changed across runs");
        } else {
            first = Some(actual);
        }
    }
}
