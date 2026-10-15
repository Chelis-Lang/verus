#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork (M8.0): `--time-expanded` adds per-bucket and crate-level stage
// timers under `times-ms.sail-stages`; `--time` alone does not.
fn two_modules() -> String {
    verus_code! {
        mod a {
            use vstd::prelude::*;
            pub proof fn lemma_a() { assert(1int + 1 == 2); }
        }
        mod b {
            use vstd::prelude::*;
            pub proof fn lemma_b() { assert(2int + 2 == 4); }
            pub proof fn lemma_c() { assert(3int + 3 == 6); }
        }
    }
}

#[test]
fn sail_m8_stage_timers_with_time_expanded() {
    let diags = verify_one_file(
        "sail_m8_stage_timers_with_time_expanded",
        two_modules(),
        &["--output-json", "--sail-raw=--time-expanded"],
    )
    .unwrap_or_else(|err| panic!("should verify: {:?}", err));
    let json = diags.json_output.expect("--output-json output");
    let stages = &json["times-ms"]["sail-stages"];
    assert_eq!(stages["version"], 1);
    let crates = stages["crate"].as_array().expect("crate stages");
    assert_eq!(crates.len(), 1);
    let buckets = stages["buckets"].as_array().expect("bucket stages");
    assert_eq!(crates[0]["num-buckets"].as_u64().unwrap(), buckets.len() as u64);
    let names: Vec<&str> = buckets.iter().map(|b| b["bucket"].as_str().unwrap()).collect();
    assert!(names.contains(&"module a") && names.contains(&"module b"), "{:?}", names);
    for b in buckets {
        let start = b["start"].as_u64().unwrap();
        let end = b["end"].as_u64().unwrap();
        assert!(end >= start);
        // every bucket sends its prelude and context to a solver process
        assert!(b["smt-bytes"].as_u64().unwrap() > 0);
        assert_eq!(b["smt-digest"].as_str().unwrap().len(), 16);
        // the bucket-key proxy runs only with SAIL_M8_KEY_PROXY set
        let key_len = b["key-digest"].as_str().unwrap().len();
        assert!(key_len == 0 || key_len == 64);
        let parts: u64 = [
            "prune",
            "key",
            "ctx-new",
            "ast-to-sst",
            "poly",
            "air-context",
            "bucket-context",
            "op-generation",
            "air-command",
            "solver-spawn",
            "solver-wait",
            "other",
        ]
        .iter()
        .map(|k| b[*k].as_u64().unwrap())
        .sum();
        // the stages partition the bucket's wall time (up to rounding)
        assert!(parts <= end - start + 20, "{} > {}", parts, end - start);
    }
    let a = buckets.iter().find(|b| b["bucket"] == "module a").unwrap();
    let bb = buckets.iter().find(|b| b["bucket"] == "module b").unwrap();
    assert_ne!(a["smt-digest"], bb["smt-digest"]);
    assert_eq!(bb["functions"], 2);
    // front-end stages of the GHOST pass, in order
    let front: Vec<&str> = stages["front"]
        .as_array()
        .expect("front stages")
        .iter()
        .map(|s| s["stage"].as_str().unwrap())
        .collect();
    for name in ["import", "hir-typeck", "rust-to-vir", "well-formed", "autospec-modes", "lifetime-borrowck"] {
        assert!(front.contains(&name), "{:?}", front);
    }
}

#[test]
fn sail_m8_stage_timers_absent_without_time_expanded() {
    let diags = verify_one_file(
        "sail_m8_stage_timers_absent_without_time_expanded",
        two_modules(),
        &["--output-json", "--sail-raw=--time"],
    )
    .unwrap_or_else(|err| panic!("should verify: {:?}", err));
    let json = diags.json_output.expect("--output-json output");
    assert!(json["times-ms"].get("total").is_some());
    assert!(json["times-ms"].get("sail-stages").is_none());
}
