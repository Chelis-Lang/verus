#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0009: with `--verify-function`, the
// `--output-json --time` function breakdown also listed unselected functions,
// with `"success": true`, although none of their queries ran. Only selected
// functions may appear.
#[test]
fn sail_vi0009_timing_lists_only_selected_functions() {
    let code = verus_code! {
        pub proof fn good() { assert(true); }
        pub proof fn bad() { assert(false); }
    };
    let result = verify_one_file(
        "sail_vi0009_timing_lists_only_selected_functions",
        code,
        &[
            "--output-json",
            "--sail-raw=--time",
            "--sail-raw=--verify-root",
            "--sail-raw=--verify-function",
            "--sail-raw=good",
        ],
    );
    let diags = match result {
        Ok(diags) => diags,
        Err(err) => panic!("selected function should verify: {:?}", err),
    };
    let json = diags.json_output.expect("--output-json output");
    let mut functions = Vec::new();
    for module in json["times-ms"]["smt"]["smt-run-module-times"].as_array().expect("modules") {
        for entry in module["function-breakdown"].as_array().expect("breakdown") {
            functions.push(entry["function"].as_str().expect("name").to_owned());
        }
    }
    assert_eq!(functions, vec!["test_crate::good".to_owned()]);
}
