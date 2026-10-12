#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0010: two trait impls of a method named
// `check` for the same struct share the friendly `function` name in the
// `--output-json --time` breakdown. The new `function-path` field must tell
// them apart, and the `function` field must be unchanged.
#[test]
fn sail_vi0010_function_path_distinguishes_trait_impls() {
    let code = verus_code! {
        pub trait A { proof fn check(&self); }
        pub trait B { proof fn check(&self); }
        pub struct S { }
        mod first {
            use super::*;
            impl A for S { proof fn check(&self) { assert(true); } }
        }
        mod second {
            use super::*;
            impl B for S { proof fn check(&self) { assert(true); } }
        }
    };
    let result = verify_one_file(
        "sail_vi0010_function_path_distinguishes_trait_impls",
        code,
        &["--output-json", "--sail-raw=--time"],
    );
    let diags = match result {
        Ok(diags) => diags,
        Err(err) => panic!("verification failed: {:?}", err),
    };
    let json = diags.json_output.expect("--output-json output");
    let mut paths = std::collections::BTreeSet::new();
    let mut friendly = Vec::new();
    for module in json["times-ms"]["smt"]["smt-run-module-times"].as_array().expect("modules") {
        for entry in module["function-breakdown"].as_array().expect("breakdown") {
            if entry["function"] == "test_crate::S::check" {
                friendly.push(entry["function"].clone());
                paths.insert(entry["function-path"].as_str().expect("function-path").to_owned());
            }
        }
    }
    assert_eq!(friendly.len(), 2, "{json}");
    assert_eq!(paths.len(), 2, "function-path does not distinguish the impls: {paths:?}");
}
