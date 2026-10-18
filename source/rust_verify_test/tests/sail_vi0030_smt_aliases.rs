#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

// Source-only controls: all new compiler runs remain uncompiled/unrun.
const SLOT: &str = "/*INSERT_EMPTY_IMPL*/";
fn source(body: &str) -> String {
    format!("use vstd::prelude::*;\n{body}")
}

fn inserted(source: &str) -> String {
    // Preserve all later source offsets so exact comparisons need no comment
    // removal or source-position exception. Only the impl syntax changes.
    assert_eq!(source.matches(SLOT).count(), 1);
    let edit = format!("{:width$}", "impl Unrelated {}", width = SLOT.len());
    assert_eq!(edit.len(), SLOT.len());
    source.replace(SLOT, &edit)
}

fn run(
    dir: &Path,
    code: &str,
    label: &str,
    extra: &[String],
) -> (std::process::Output, std::path::PathBuf) {
    let file = dir.join("test.rs");
    std::fs::write(&file, code).unwrap();
    let logs = dir.join(label);
    let mut args = vec![
        "--crate-name=sail_vi0030".to_string(),
        "--output-json".to_string(),
        "--sail-raw=--time".to_string(),
        "--sail-raw=--log".to_string(),
        "--sail-raw=smt-transcript".to_string(),
        "--sail-raw=--log".to_string(),
        "--sail-raw=vir".to_string(),
        "--sail-raw=--log-dir".to_string(),
        format!("--sail-raw={}", logs.display()),
    ];
    args.extend_from_slice(extra);
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let output = run_verus(&args, dir, &file, true, true);
    (output, logs)
}

fn checked(
    dir: &Path,
    code: &str,
    label: &str,
    extra: &[String],
) -> (serde_json::Value, std::path::PathBuf) {
    let (output, logs) = run(dir, code, label, extra);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.stderr.is_empty(),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verification-results"]["success"], true);
    assert_eq!(report["verification-results"]["errors"], 0);
    (report, logs)
}

fn paths(report: &serde_json::Value) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for module in report["times-ms"]["smt"]["smt-run-module-times"].as_array().unwrap() {
        for f in module["function-breakdown"].as_array().unwrap() {
            assert_eq!(f["success"], true);
            result.push((
                f["function"].as_str().unwrap().to_owned(),
                f["function-path"].as_str().unwrap().to_owned(),
            ));
        }
    }
    assert!(!result.is_empty());
    result.sort();
    result
}

fn method_path<'a>(rows: &'a [(String, String)], name: &str) -> &'a str {
    let rows: Vec<_> = rows.iter().filter(|(friendly, _)| friendly.ends_with(name)).collect();
    assert_eq!(rows.len(), 1, "expected one method {name}");
    &rows[0].1
}

// Exact existing transcript framing; query bytes and terminators remain intact.
fn literal(raw: &[u8]) -> Vec<u8> {
    const QUERY: &[u8] = b";;;>>> QUERY\n";
    const RESPONSE: &[u8] = b";;;>>> RESPONSE\n";
    const CLOSE: &[u8] = b";;;<<<\n";
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < raw.len() {
        assert!(raw[pos..].starts_with(QUERY));
        pos += QUERY.len();
        let length = raw[pos..].windows(CLOSE.len()).position(|x| x == CLOSE).unwrap();
        out.extend(&raw[pos..pos + length]);
        out.extend(b"\n(echo \"<<DONE>>\")\n");
        pos += length + CLOSE.len();
        assert!(raw[pos..].starts_with(RESPONSE));
        pos += RESPONSE.len();
        loop {
            let length = raw[pos..].iter().position(|x| *x == b'\n').unwrap();
            let line = &raw[pos..pos + length];
            pos += length + 1;
            if line == b";;;<<<" {
                break;
            }
        }
    }
    out
}

fn inputs(logs: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(logs).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "smt_transcript") {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            assert!(out.insert(name, literal(&std::fs::read(path).unwrap())).is_none());
        }
    }
    assert!(!out.is_empty());
    assert!(out.values().any(|x| !x.is_empty()));
    out
}

fn alias_symbols(logs: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    // Inspect complete literal inputs without changing or replacing any byte.
    // Empty sessions and duplicate bodies remain in the retained map.
    for bytes in inputs(logs).values() {
        let text = std::str::from_utf8(bytes).unwrap();
        for (_, rest) in text.match_indices("%sail%fn%").map(|(i, _)| (i, &text[i..])) {
            let hex = rest.strip_prefix("%sail%fn%").unwrap();
            let end = hex.bytes().take_while(|b| b.is_ascii_hexdigit()).count();
            assert!(end > 0 && hex.as_bytes()[end] == b'.');
            names.insert(rest[.."%sail%fn%".len() + end + 1].to_owned());
        }
    }
    names
}

const CLONE: &str = verus_code_str! {
    pub struct Unrelated {}
    /*INSERT_EMPTY_IMPL*/
    #[derive(Clone)] pub struct Kept {}
    pub fn copy(s: &Kept) -> Kept { s.clone() }
    proof fn check() { assert(true); }
};

#[test]
fn sail_vi0030_clone_smt_alias_survives_edit_but_original_paths_stay_original() {
    let dir = tempfile::tempdir().unwrap();
    let original = source(CLONE);
    let (a, alogs) = checked(dir.path(), &original, "before", &[]);
    let first_names = alias_symbols(&alogs);
    assert!(!first_names.is_empty(), "must exercise a real eligible Clone contract");
    let first_input = inputs(&alogs);
    let (_, repeat) = checked(dir.path(), &original, "repeat", &[]);
    assert_eq!(first_input, inputs(&repeat), "unchanged program's complete literal input changed");
    let (b, blogs) = checked(dir.path(), &inserted(&original), "after", &[]);
    assert_eq!(first_names, alias_symbols(&blogs));
    let a = paths(&a);
    let b = paths(&b);
    let before = method_path(&a, "::Kept::clone");
    let after = method_path(&b, "::Kept::clone");
    assert!(before.contains("impl&%") && after.contains("impl&%"));
    assert_ne!(before, after, "original Rust/VIR identities must not be renamed");
    for logs in [&alogs, &blogs] {
        let vir = std::fs::read_to_string(logs.join("crate.vir")).unwrap();
        assert!(!vir.contains("%sail%fn%"), "SMT spelling leaked into original VIR");
    }
    // Complete edit-loop inputs are retained. This first screen does not claim
    // invariance of impl qids, inherent functions or accepted closure IDs.
    assert_eq!(first_input.len(), inputs(&blogs).len());
}

#[test]
fn sail_vi0030_inherent_and_generic_receiver_functions_keep_legacy_smt_spelling() {
    let body = verus_code_str! {
        pub struct S {} impl S { pub open spec fn value(&self, x: int) -> int { x + 1 } }
        pub struct Generic<T> { pub value: T }
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl<T> Id for Generic<T> { fn id(&self, x: u8) -> (r: u8) { x } }
        pub fn call(s: &Generic<u8>, x: u8) -> (r: u8) ensures r == x { s.id(x) }
        proof fn check(s: S, x: int) { assert(s.value(x) == x + 1); }
    };
    let dir = tempfile::tempdir().unwrap();
    let (_, logs) = checked(dir.path(), &source(body), "legacy", &[]);
    assert!(alias_symbols(&logs).is_empty());
}

#[test]
fn sail_vi0030_impossible_predicate_duplicate_methods_all_keep_legacy() {
    let body = verus_code_str! {
        pub struct S {} pub trait Impossible {}
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl Id for S { fn id(&self, x: u8) -> (r: u8) { x } }
        impl Id for S where S: Impossible { fn id(&self, x: u8) -> (r: u8) { x } }
        pub fn call(s: &S, x: u8) -> (r: u8) ensures r == x { s.id(x) }
    };
    let code =
        format!("#![feature(trivial_bounds)]\n#![allow(incomplete_features)]\n{}", source(body));
    let dir = tempfile::tempdir().unwrap();
    let (report, logs) = checked(dir.path(), &code, "duplicate", &[]);
    assert_eq!(paths(&report).iter().filter(|(n, _)| n.ends_with("::S::id")).count(), 2);
    assert!(
        alias_symbols(&logs).is_empty(),
        "impossible duplicate was dropped before collision selection"
    );
}

#[test]
fn sail_vi0030_const_specializing_ground_duplicate_methods_all_keep_legacy() {
    let body = verus_code_str! {
        pub struct S {}
        pub const trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl Id for S { default fn id(&self, x: u8) -> (r: u8) { x } }
        const impl Id for S { fn id(&self, x: u8) -> (r: u8) { x } }
        pub fn call(s: &S, x: u8) -> (r: u8) ensures r == x { s.id(x) }
    };
    let code = format!(
        "#![feature(specialization, const_trait_impl)]\n#![allow(incomplete_features)]\n{}",
        source(body)
    );
    let dir = tempfile::tempdir().unwrap();
    let (report, logs) = checked(dir.path(), &code, "specialization", &[]);
    assert_eq!(paths(&report).iter().filter(|(n, _)| n.ends_with("::S::id")).count(), 2);
    assert!(alias_symbols(&logs).is_empty(), "const child and nonconst parent alias each other");
}

#[test]
fn sail_vi0030_actual_producer_native_consumer_cfg_and_ghosts_keep_original_abi() {
    let body = verus_code_str! {
        pub struct S {}
        impl S {
            spec fn hidden(&self) -> bool { true }
            pub(crate) spec fn restricted(&self) -> bool { true }
            pub fn inherent_id(&self, x: u8) -> (r: u8) ensures r == x { x }
        }
        impl S { proof fn ghost_only(&self) {} } impl S {} impl S {}
        #[cfg_attr(verus_keep_ghost_body, marker)] pub trait Mark {}
        impl Mark for S {}
        pub fn require<T: Mark>(s: &T) {}
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl<'a> Id for S where S: Mark { fn id(&self, x: u8) -> (r: u8) { x } }
    };
    let producer = format!("#![feature(marker_trait_attr)]\n{}", source(body));
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("dep.vir");
    let extra = vec![
        "--compile".to_string(),
        "--sail-raw=--export".to_string(),
        format!("--sail-raw={}", export.display()),
    ];
    let (a, producer_logs) = checked(dir.path(), &producer, "producer", &extra);
    let rows = paths(&a);
    let original = method_path(&rows, "::S::id").to_owned();
    assert!(original.contains("impl&%") && !original.contains("%sail%fn%"));
    assert!(!alias_symbols(&producer_logs).is_empty());
    let client = source(verus_code_str! {
        use dep::Id;
        pub fn check(s: &dep::S, x: u8) -> (r: u8) ensures r == x {
            dep::require(s); let y = s.inherent_id(x); s.id(y)
        }
    });
    let client = format!("extern crate sail_vi0030 as dep;\n{client}");
    let extra = vec![
        "--crate-name=sail_vi0030_client".to_string(),
        "--sail-raw=--extern".to_string(),
        format!("--sail-raw=sail_vi0030={}", dir.path().join("libtest.rlib").display()),
        "--sail-raw=--import".to_string(),
        format!("--sail-raw=sail_vi0030={}", export.display()),
    ];
    let (_, logs) = checked(dir.path(), &client, "consumer", &extra);
    let vir = std::fs::read_to_string(logs.join("crate.vir")).unwrap();
    assert!(vir.contains(&original), "consumer stopped resolving the original exported contract");
    assert!(!vir.contains("%sail%fn%"));
}

#[test]
fn sail_vi0030_inline_air_flag_preserves_literal_names_and_disables_all_aliases() {
    let body = verus_code_str! {
        pub struct S {} pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl Id for S { fn id(&self, x: u8) -> (r: u8) { x } }
        proof fn raw() { inline_air_stmt("(assert true)"); }
        pub fn call(s: &S, x: u8) -> (r: u8) ensures r == x { s.id(x) }
    };
    let dir = tempfile::tempdir().unwrap();
    let (_, logs) = checked(
        dir.path(),
        &source(body),
        "raw-air",
        &["--sail-raw=-V".to_string(), "--sail-raw=allow-inline-air".to_string()],
    );
    assert!(alias_symbols(&logs).is_empty());
    let air = std::fs::read_to_string(logs.join("root.air")).unwrap();
    assert!(air.contains("(assert true)"));
}

#[test]
fn sail_vi0030_recursive_and_inherited_default_generated_paths_stay_consistent() {
    let body = verus_code_str! {
        pub struct S {}
        pub trait Value {
            spec fn value(&self, n: nat) -> nat;
            fn default_id(&self, x: u8) -> (r: u8) ensures r == x { x }
        }
        impl Value for S {
            open spec fn value(&self, n: nat) -> nat decreases n {
                if n == 0 { 0 } else { self.value((n-1) as nat) + 1 }
            }
        }
        proof fn check(s: &S) { assert(s.value(0) == 0); }
        pub fn call(s: &S, x: u8) -> (r: u8) ensures r == x { s.default_id(x) }
    };
    let dir = tempfile::tempdir().unwrap();
    let (_, logs) = checked(dir.path(), &source(body), "synthesized", &[]);
    assert!(!alias_symbols(&logs).is_empty());
}

#[test]
fn sail_vi0030_failed_proof_retains_complete_diagnostics_with_aliases_enabled() {
    let body = CLONE.replace("assert(true)", "assert(false)");
    let code = source(&body);
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = run(dir.path(), &code, "bad-before", &[]);
    let (b, _) = run(dir.path(), &inserted(&code), "bad-after", &[]);
    assert!(!a.status.success() && !b.status.success());
    assert!(!a.stderr.is_empty());
    assert_eq!(a.stderr, b.stderr);
    for output in [a, b] {
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["verification-results"]["success"], false);
        assert!(report["verification-results"]["errors"].as_u64().unwrap() > 0);
    }
}
