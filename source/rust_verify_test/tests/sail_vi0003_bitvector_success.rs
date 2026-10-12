#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0003: a function whose only failing query
// is a bit-vector assertion was reported with `"success": true` in the
// `--output-json --time` function breakdown, although the error was reported.
#[test]
fn sail_vi0003_failed_bit_vector_function_is_not_successful() {
    let code = verus_code! {
        proof fn bv_bad(x: u32) { assert(x & 1 == 0) by (bit_vector); }
        proof fn good() { assert(true); }
    };
    let result = verify_one_file(
        "sail_vi0003_failed_bit_vector_function_is_not_successful",
        code,
        &["--output-json", "--sail-raw=--time"],
    );
    let err = match result {
        Err(err) => err,
        Ok(_) => panic!("expected the bit-vector assertion to fail"),
    };
    assert_eq!(err.errors.len(), 1);
    let json = err.json_output.expect("--output-json output");
    let mut success = std::collections::BTreeMap::new();
    for module in json["times-ms"]["smt"]["smt-run-module-times"].as_array().expect("modules") {
        for entry in module["function-breakdown"].as_array().expect("breakdown") {
            success.insert(
                entry["function"].as_str().expect("name").to_owned(),
                entry["success"].as_bool().expect("success"),
            );
        }
    }
    assert_eq!(success.get("test_crate::bv_bad"), Some(&false), "{success:?}");
    assert_eq!(success.get("test_crate::good"), Some(&true), "{success:?}");
}
