#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0015: automatic trigger selection broke
// score ties in `HashSet`/`HashMap` iteration order, which changes between
// map instances even within one process. Eight identical quantifiers in one
// file must therefore get identical automatic triggers.
#[test]
fn sail_vi0015_identical_quantifiers_get_identical_triggers() {
    let code = verus_code! {
        pub trait Distance { spec fn dist(&self, other: &Self) -> int; }
        proof fn q0<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q1<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q2<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q3<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q4<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q5<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q6<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
        proof fn q7<T: Distance>() ensures forall|x: &T, y: &T, z: &T| x.dist(y) <= x.dist(z) + z.dist(y) { assume(false); }
    };
    let result = verify_one_file(
        "sail_vi0015_identical_quantifiers_get_identical_triggers",
        code,
        &["--sail-raw=--triggers"],
    );
    let diags = match result {
        Ok(diags) => diags,
        Err(err) => panic!("verification failed: {:?}", err),
    };
    // For each quantifier (by source line), the column ranges of every
    // reported trigger term, in reporting order.
    let mut by_line: std::collections::BTreeMap<usize, Vec<(usize, usize)>> = Default::default();
    for note in diags.notes.iter().filter(|n| n.message.contains("trigger")) {
        for span in &note.spans {
            by_line.entry(span.line_start).or_default().push((span.column_start, span.column_end));
        }
    }
    assert_eq!(by_line.len(), 8, "expected trigger notes for eight quantifiers: {by_line:?}");
    let first = by_line.values().next().expect("trigger notes").clone();
    for (line, triggers) in &by_line {
        assert_eq!(triggers, &first, "quantifier on line {line} got different triggers");
    }
}
