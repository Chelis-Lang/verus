#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// VI-0010: the first bit-vector query lazily writes an AIR prelude. Identity
// metadata must follow that prelude; relaxing consumer adjacency is unsafe.
#[test]
fn sail_vi0010_air_identity_follows_lazy_prelude() {
    let logs = tempfile::tempdir().expect("log directory");
    let log_dir = format!("--sail-raw={}", logs.path().display());
    let code = verus_code! {
        spec fn recursive(n: nat) -> nat decreases n {
            if n == 0 { 0 } else { recursive((n - 1) as nat) }
        }
        proof fn bitvector(x: u8) { assert(x ^ x == 0) by(bit_vector); }
        proof fn nonlinear(x: int) { assert(x * x >= 0) by(nonlinear_arith); }
        proof fn ordinary() { assert(true); }
    };
    let options = [
        "vstd",
        "--output-json",
        "--sail-raw=--time",
        "--sail-raw=--log",
        "--sail-raw=air",
        "--sail-raw=--log",
        "--sail-raw=air-final",
        "--sail-raw=--log-dir",
        log_dir.as_str(),
    ];
    let result = verify_one_file("sail_vi0010_air_identity_follows_lazy_prelude", code, &options)
        .expect("verification succeeds");
    let output = result.json_output.expect("timing JSON");
    let mut timing = std::collections::BTreeMap::new();
    for module in output["times-ms"]["smt"]["smt-run-module-times"].as_array().expect("modules") {
        for function in module["function-breakdown"].as_array().expect("functions") {
            timing.insert(
                function["function-path"].as_str().expect("path").to_owned(),
                function["function"].as_str().expect("friendly").to_owned(),
            );
        }
    }
    let mut queries = 0;
    let mut bitvector_spinoff = false;
    let mut termination = false;
    let mut final_queries = false;
    for entry in std::fs::read_dir(logs.path()).expect("logs") {
        let path = entry.expect("log entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("air") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("AIR log");
        let filename = path.file_name().expect("filename").to_string_lossy();
        let mut previous = "";
        for line in text.lines() {
            if line.starts_with("(check-valid") {
                let payload =
                    previous.strip_prefix(";; sail-query-identity-v1 ").unwrap_or_else(|| {
                        panic!("nonadjacent identity in {}: {:?}", filename, previous)
                    });
                let identity: serde_json::Value =
                    serde_json::from_str(payload).expect("identity JSON");
                assert_eq!(identity["schema"], 1);
                let path = identity["function_path"].as_str().expect("identity path");
                assert_eq!(timing.get(path).map(String::as_str), identity["function"].as_str());
                queries += 1;
                bitvector_spinoff |= path.ends_with("::bitvector") && filename.contains("._");
                termination |=
                    path.ends_with("::recursive") && identity["kind"] == "Spec-Termination";
                final_queries |= filename.ends_with("-final.air");
            }
            previous = line;
        }
    }
    assert!(queries >= 4, "no meaningful AIR query coverage");
    assert!(bitvector_spinoff, "first bit-vector spinoff was not exercised");
    assert!(termination, "spec termination query was not exercised");
    assert!(final_queries, "final AIR logs were not exercised");
}
