#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0007: with several verification threads,
// errors from different modules were printed in the order their buckets
// first reported, which varies between runs. They must be printed in bucket
// (module) order. The first module does more work than the others, so on
// stock Verus its error almost always arrives last.
#[test]
fn sail_vi0007_errors_from_modules_in_module_order() {
    let code = verus_code! {
        mod m1 {
            use super::*;
            proof fn w0(a: int, b: int, c: int) { assert((a + b) * c == a * c + b * c) by (nonlinear_arith); }
            proof fn w1(a: int, b: int, c: int) { assert((a - b) * c == a * c - b * c) by (nonlinear_arith); }
            proof fn w2(a: int, b: int, c: int) { assert(a * (b + c) == a * b + a * c) by (nonlinear_arith); }
            proof fn w3(a: int, b: int, c: int) { assert(a * (b - c) == a * b - a * c) by (nonlinear_arith); }
            proof fn w4(a: int, b: int) { assert((a + b) * (a + b) == a * a + 2 * a * b + b * b) by (nonlinear_arith); }
            proof fn w5(a: int, b: int) { assert((a - b) * (a - b) == a * a - 2 * a * b + b * b) by (nonlinear_arith); }
            proof fn w6(a: int, b: int) { assert((a + b) * (a - b) == a * a - b * b) by (nonlinear_arith); }
            proof fn w7(a: int, b: int, c: int) { assert((a + b + c) * (a + b + c) == a * a + b * b + c * c + 2 * a * b + 2 * b * c + 2 * a * c) by (nonlinear_arith); }
            proof fn fail1() { assert(false); }
        }
        mod m2 { proof fn fail2() { assert(false); } }
        mod m3 { proof fn fail3() { assert(false); } }
        mod m4 { proof fn fail4() { assert(false); } }
        mod m5 { proof fn fail5() { assert(false); } }
    };
    for round in 0..3 {
        let result = verify_one_file(
            "sail_vi0007_errors_from_modules_in_module_order",
            code.clone(),
            &["--sail-raw=--num-threads", "--sail-raw=4"],
        );
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("expected five errors"),
        };
        assert_eq!(err.errors.len(), 5, "round {round}: {:?}", err.errors);
        let lines: Vec<usize> =
            err.errors.iter().map(|e| relevant_error_span(&e.spans).line_start).collect();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted, "round {round}: errors not in module order");
    }
}
