#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0017: `--output-json --time` listed a
// module's `function-breakdown` in `HashMap` order and modules by elapsed
// time, so identical runs printed different orders. Modules must be listed by
// name and functions by friendly name.
#[test]
fn sail_vi0017_timing_lists_sorted_by_name() {
    let code = verus_code! {
        mod m3 { pub proof fn g2() { assert(true); } pub proof fn g1() { assert(true); } }
        mod m1 { pub proof fn g2() { assert(true); } pub proof fn g1() { assert(true); } }
        mod m2 { pub proof fn g2() { assert(true); } pub proof fn g1() { assert(true); } }
        proof fn f8() { assert(true); }
        proof fn f3() { assert(true); }
        proof fn f6() { assert(true); }
        proof fn f1() { assert(true); }
        proof fn f7() { assert(true); }
        proof fn f2() { assert(true); }
        proof fn f5() { assert(true); }
        proof fn f4() { assert(true); }
    };
    let result = verify_one_file(
        "sail_vi0017_timing_lists_sorted_by_name",
        code,
        &["--output-json", "--sail-raw=--time"],
    );
    let diags = match result {
        Ok(diags) => diags,
        Err(err) => panic!("verification failed: {:?}", err),
    };
    let json = diags.json_output.expect("--output-json output");
    let times = &json["times-ms"];
    let modules = |list: &serde_json::Value| -> Vec<String> {
        list.as_array()
            .expect("module list")
            .iter()
            .map(|m| m["module"].as_str().expect("module").to_owned())
            .collect()
    };
    let expected_modules: Vec<String> =
        ["", "m1", "m2", "m3"].iter().map(|s| s.to_string()).collect();
    assert_eq!(modules(&times["total-verify-module-times"]), expected_modules);
    assert_eq!(modules(&times["air"]["module-times"]), expected_modules);
    assert_eq!(modules(&times["smt"]["smt-init-module-times"]), expected_modules);
    let run = &times["smt"]["smt-run-module-times"];
    assert_eq!(modules(run), expected_modules);
    let functions: Vec<String> = run[0]["function-breakdown"]
        .as_array()
        .expect("breakdown")
        .iter()
        .map(|f| f["function"].as_str().expect("function").to_owned())
        .collect();
    let expected: Vec<String> = (1..=8).map(|i| format!("test_crate::f{i}")).collect();
    // On stock Verus this order is random (1 in 40320 runs pass by chance).
    assert_eq!(functions, expected);
    for m in 1..=3 {
        let functions: Vec<String> = run[m]["function-breakdown"]
            .as_array()
            .expect("breakdown")
            .iter()
            .map(|f| f["function"].as_str().expect("function").to_owned())
            .collect();
        assert_eq!(functions, vec![format!("test_crate::m{m}::g1"), format!("test_crate::m{m}::g2")]);
    }
}
