#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0008: `--output-json` listed `func-details`
// in `HashMap` order, which changes between runs. Entries must be sorted by
// function name.
#[test]
fn sail_vi0008_func_details_sorted_by_name() {
    let code = verus_code! {
        fn f8() {}
        fn f3() {}
        fn f6() {}
        fn f1() {}
        fn f7() {}
        fn f2() {}
        fn f5() {}
        fn f4() {}
    };
    let result =
        verify_one_file("sail_vi0008_func_details_sorted_by_name", code, &["--output-json"]);
    let diags = match result {
        Ok(diags) => diags,
        Err(err) => panic!("verification failed: {:?}", err),
    };
    let json = diags.json_output.expect("--output-json output");
    let names: Vec<String> =
        json["func-details"].as_object().expect("func-details object").keys().cloned().collect();
    let expected: Vec<String> = (1..=8).map(|i| format!("test_crate::f{i}")).collect();
    // On stock Verus this order is random (1 in 40320 runs pass by chance).
    assert_eq!(names, expected);
}
