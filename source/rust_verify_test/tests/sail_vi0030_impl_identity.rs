#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;
use std::{collections::BTreeMap, path::Path};

// Source-only controls: no execution result is claimed until the compiler gate.
const SLOT: &str = "/*INSERT_EMPTY_IMPL*/";
const SIMPLE: &str = verus_code_str! {
    pub struct Unrelated {}
    /*INSERT_EMPTY_IMPL*/
    #[derive(Clone)]
    pub struct Kept {}
    pub trait Keep {
        fn run(&self, x: u8) -> (r: u8) ensures r == x;
    }
    impl Keep for Kept {
        fn run(&self, x: u8) -> (r: u8) {
            let same = |v: u8| -> (out: u8) ensures out == v { v };
            same(x)
        }
    }
    pub trait Outside { fn external_probe(&self, x: u8) -> u8; }
    #[verifier(external)]
    impl Outside for Kept { fn external_probe(&self, x: u8) -> u8 { x } }
    mod client {
        use vstd::prelude::*;
        use crate::Keep;
        pub fn call(s: &crate::Kept, x: u8) -> (r: u8) ensures r == x { s.run(x) }
        pub fn copy(s: &crate::Kept) -> crate::Kept { s.clone() }
    }
};

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
    std::fs::write(&file, format!("use vstd::prelude::*;\n{code}")).unwrap();
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

#[test]
fn sail_vi0030_empty_impl_preserves_method_closure_and_complete_input() {
    let dir = tempfile::tempdir().unwrap();
    let (a, alogs) = checked(dir.path(), SIMPLE, "before", &[]);
    let a = paths(&a);
    let ain = inputs(&alogs);
    let (b, blogs) = checked(dir.path(), &inserted(SIMPLE), "after", &[]);
    assert_eq!(a, paths(&b));
    let run_path = method_path(&a, "::Kept::run");
    assert!(run_path.contains("impl&%s2"));
    let vir = std::fs::read_to_string(alogs.join("crate.vir")).unwrap();
    // This external method's path was recorded while the registry was disabled.
    // Its containing trait impl must receive the same naming policy after refresh.
    let external = vir
        .split("(external_fn")
        .skip(1)
        .find(|part| part.split("\n\n").next().unwrap().contains("external_probe"))
        .expect("early external method record");
    let external = external.split("\n\n").next().unwrap();
    assert!(external.contains("impl&%s2"), "early path was not refreshed after coherence");
    let bvir = std::fs::read_to_string(blogs.join("crate.vir")).unwrap();
    assert!(bvir.contains(external), "external trait-method identity shifted");
    assert!(
        vir.split(":stable_impl_key").skip(1).any(|tail| tail.trim_start().starts_with('"')),
        "fixture must exercise stable closure identity"
    );
    assert_eq!(ain, inputs(&blogs), "complete literal inputs changed; no normalization is allowed");
}

#[test]
fn sail_vi0030_inherent_split_duplicate_and_ghost_blocks_remain_legacy() {
    let code = verus_code_str! {
        pub struct S {}
        impl S {} impl S {}
        impl S { spec fn hidden(&self) -> bool { true } }
        impl S { pub fn first(&self, x: u8) -> (r: u8) ensures r == x { x } }
        impl S { pub fn second(&self, x: u8) -> (r: u8) ensures r == x { x } }
    };
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = checked(dir.path(), code, "legacy", &[]);
    let a = paths(&a);
    let first = method_path(&a, "::S::first");
    let second = method_path(&a, "::S::second");
    assert!(!first.contains("impl&%s2"));
    assert!(!second.contains("impl&%s2"));
    assert_ne!(first.rsplit_once("::").unwrap().0, second.rsplit_once("::").unwrap().0);
}

#[test]
fn sail_vi0030_generic_self_and_trait_arguments_remain_legacy() {
    let code = verus_code_str! {
        pub struct Generic<T> { pub item: T }
        pub trait A { fn generic(&self, x: u8) -> (r: u8) ensures r == x; }
        impl<T> A for Generic<T> { fn generic(&self, x: u8) -> (r: u8) { x } }
        pub struct S {}
        pub trait B<T> { fn parameterized(&self, x: u8) -> (r: u8) ensures r == x; }
        impl B<u8> for S { fn parameterized(&self, x: u8) -> (r: u8) { x } }
    };
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = checked(dir.path(), code, "legacy", &[]);
    let rows = paths(&a);
    for name in ["::generic", "::S::parameterized"] {
        assert!(!method_path(&rows, name).contains("impl&%s2"));
    }
}

fn import_check(producer: &str, client: &str, expected: &[(&str, bool)]) {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("dep.vir");
    let extra = vec![
        "--compile".to_string(),
        "--sail-raw=--export".to_string(),
        format!("--sail-raw={}", export.display()),
    ];
    let (a, _) = checked(dir.path(), producer, "producer", &extra);
    let producer_paths = paths(&a);
    let extra = vec![
        "--crate-name=sail_vi0030_client".to_string(),
        "--sail-raw=--extern".to_string(),
        format!("--sail-raw=sail_vi0030={}", dir.path().join("libtest.rlib").display()),
        "--sail-raw=--import".to_string(),
        format!("--sail-raw=sail_vi0030={}", export.display()),
    ];
    let client = format!("extern crate sail_vi0030 as dep;\n{client}");
    let (_, logs) = checked(dir.path(), &client, "consumer", &extra);
    let vir = std::fs::read_to_string(logs.join("crate.vir")).unwrap();
    for (name, stable) in expected {
        let path = method_path(&producer_paths, name);
        assert_eq!(path.contains("impl&%s2"), *stable, "unexpected policy for {name}");
        assert!(vir.contains(path), "consumer metadata identity differs for {name}: {path}");
    }
    // Each consumer genuinely verifies calls through separately compiled native
    // metadata against the producer's exported contracts, not a toy certificate.
}

#[test]
fn sail_vi0030_import_private_restricted_ghost_and_duplicate_inherent_peers() {
    let producer = verus_code_str! {
        pub struct S {}
        impl S {
            spec fn hidden(&self) -> bool { true }
            spec const HIDDEN: u8 = 1;
            pub(crate) spec const RESTRICTED: u8 = 2;
            pub(crate) spec fn restricted(&self) -> bool { true }
            pub fn inherent_id(&self, x: u8) -> (r: u8) ensures r == x { x }
        }
        impl S { proof fn ghost_only(&self) {} }
        impl S {} impl S {}
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl Id for S {
            fn id(&self, x: u8) -> (r: u8) {
                let same = |v: u8| -> (r: u8) ensures r == v { v };
                same(x)
            }
        }
    };
    let client = verus_code_str! {
        use dep::Id;
        pub fn check(s: &dep::S, x: u8) -> (r: u8) ensures r == x {
            let y = s.inherent_id(x);
            s.id(y)
        }
    };
    import_check(producer, client, &[("::S::id", true), ("::S::inherent_id", false)]);
}

#[test]
fn sail_vi0030_import_unused_impl_lifetime_and_ground_predicate() {
    let producer = verus_code_str! {
        pub struct S {} pub trait Marker {} impl Marker for S {}
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl<'a> Id for S where S: Marker { fn id(&self, x: u8) -> (r: u8) { x } }
    };
    let client = verus_code_str! {
        use dep::Id;
        pub fn check(s: &dep::S, x: u8) -> (r: u8) ensures r == x { s.id(x) }
    };
    import_check(producer, client, &[("::S::id", true)]);
}

#[test]
fn sail_vi0030_import_over_limit_header_stays_legacy_on_both_sides() {
    let module = "very_long_module_name".repeat(30);
    let body = verus_code_str! {
        pub struct S {}
        pub trait Id { fn id(&self, x: u8) -> (r: u8) ensures r == x; }
        impl Id for S { fn id(&self, x: u8) -> (r: u8) { x } }
    };
    let producer = format!("pub mod {module} {{ use vstd::prelude::*; {body} }}");
    let client_body = verus_code_str! {
        pub fn check(s: &S, x: u8) -> (r: u8) ensures r == x { s.id(x) }
    };
    let client = format!("use dep::{module}::{{S, Id}}; {client_body}");
    import_check(&producer, &client, &[("::S::id", false)]);
}

#[test]
fn sail_vi0030_failed_proof_keeps_complete_diagnostics_and_verdict_vector() {
    let code = SIMPLE.replace("mod client {", "proof fn broken() { assert(false); }\nmod client {");
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = run(dir.path(), &code, "failed-before", &[]);
    let (b, _) = run(dir.path(), &inserted(&code), "failed-after", &[]);
    assert!(!a.status.success());
    assert!(!b.status.success());
    assert!(!a.stderr.is_empty());
    assert_eq!(a.stderr, b.stderr);
    let vector = |raw: &[u8]| {
        let report: serde_json::Value = serde_json::from_slice(raw).unwrap();
        assert_eq!(report["verification-results"]["success"], false);
        assert!(report["verification-results"]["errors"].as_u64().unwrap() > 0);
        let mut rows = Vec::new();
        for module in report["times-ms"]["smt"]["smt-run-module-times"].as_array().unwrap() {
            for f in module["function-breakdown"].as_array().unwrap() {
                rows.push((
                    module["module"].as_str().unwrap().to_owned(),
                    f["function"].as_str().unwrap().to_owned(),
                    f["mode:"].as_str().unwrap().to_owned(),
                    f["success"].as_bool().unwrap(),
                    f["function-path"].as_str().unwrap().to_owned(),
                ));
            }
        }
        assert!(rows.iter().any(|row| !row.3));
        rows.sort();
        rows
    };
    assert_eq!(vector(&a.stdout), vector(&b.stdout));
}

#[test]
fn sail_vi0030_coherence_errors_precede_identity_activation() {
    let code = verus_code_str! {
        pub struct Unrelated {}
        /*INSERT_EMPTY_IMPL*/
        pub struct S {} pub trait T { fn id(&self) -> u8; }
        impl T for S { fn id(&self) -> u8 { 1 } }
        impl T for S { fn id(&self) -> u8 { 2 } }
    };
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = run(dir.path(), code, "coherence-before", &[]);
    let (b, _) = run(dir.path(), &inserted(code), "coherence-after", &[]);
    assert!(!a.status.success());
    assert!(!b.status.success());
    assert_eq!(a.stderr, b.stderr);
    let stderr = String::from_utf8(a.stderr).unwrap();
    assert!(stderr.contains("E0119"), "must exercise actual conflicting-impl coherence");
    assert!(!stderr.contains("impl&%s2"), "identity activated before coherence completed");
}

#[test]
fn sail_vi0030_module_items_retain_containing_module_and_map_classification() {
    let code = verus_code_str! {
        mod empty {}
        mod real {
            pub fn active(x: u8) -> (r: u8) ensures r == x { x }
            #[verifier(external)]
            pub fn ignored(x: u8) -> u8 { x }
            mod also_empty {}
        }
    };
    let dir = tempfile::tempdir().unwrap();
    let (report, logs) = checked(dir.path(), code, "modules", &[]);
    let vir = std::fs::read_to_string(logs.join("crate.vir")).unwrap();
    // A Mod item belongs to its parent module. Empty child modules do not
    // create used-module entries simply because their own item is classified.
    let mut modules: Vec<_> = vir.lines().filter(|line| line.starts_with("(module_id ")).collect();
    modules.sort();
    assert_eq!(modules, vec!["(module_id sail_vi0030::)", "(module_id sail_vi0030::real)"]);
    assert!(vir.contains("sail_vi0030::real::ignored"));
    // The vector's external record and owner map retain classification: ignored
    // is not a verified function, while the active member is checked normally.
    let rows = paths(&report);
    assert_eq!(rows.iter().filter(|(name, _)| name.ends_with("::real::active")).count(), 1);
    assert!(!rows.iter().any(|(name, _)| name.ends_with("::ignored")));
}
