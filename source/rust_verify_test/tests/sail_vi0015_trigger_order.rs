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

// ADR 0115 / VI-0044: scheduling may reorder the printed quantifiers, but the
// explanatory summary always uses the least original location spelling.
#[test]
fn sail_vi0044_summary_attachment_is_deterministic() {
    let code = verus_code! {
        use vstd::prelude::*;
        uninterp spec fn p(n: int) -> bool;
        mod first {
            use super::*;
            proof fn one(reqs: Set<int>) {
                assume(exists|n: int| !reqs.contains(n) && p(n));
                let n = choose|n: int| !reqs.contains(n) && p(n);
                assert(!reqs.contains(n) && p(n));
            }
        }
        mod second {
            use super::*;
            proof fn two(reqs: Set<int>) {
                assume(exists|n: int| !reqs.contains(n) && p(n));
                let n = choose|n: int| !reqs.contains(n) && p(n);
                assert(!reqs.contains(n) && p(n));
            }
        }
    };
    let mut first_attachment = None;
    for iteration in 0..20 {
        // Preserve the individual compiler boundary: moving any original
        // diagnostic between iterations must still change libtest stderr.
        eprintln!("original compiler diagnostic iteration {iteration}");
        let diagnostics = with_sorted_diagnostic_echo(|| {
            verify_one_file(
                "sail_vi0044_summary_attachment",
                code.clone(),
                &["--sail-raw=--num-threads=4"],
            )
        })
        .expect("both original proof functions verify");
        let shown: Vec<_> = diagnostics
            .notes
            .iter()
            .filter(|note| note.message == "automatically chose triggers for this expression:")
            .flat_map(|note| note.spans.iter().filter(|span| span.is_primary))
            .collect();
        assert!(shown.len() >= 2, "multiple original low-confidence quantifiers");
        let expected = shown
            .iter()
            .min_by_key(|span| {
                format!("{}:{}:{}", span.file_name, span.line_start, span.column_start)
            })
            .expect("shown quantifiers");
        let summaries: Vec<_> = diagnostics
            .notes
            .iter()
            .filter(|note| {
                note.message.starts_with(
                    "Verus printed one or more automatically chosen quantifier triggers\n",
                )
            })
            .collect();
        assert_eq!(summaries.len(), 1, "one complete explanatory summary");
        let spans: Vec<_> = summaries[0].spans.iter().filter(|span| span.is_primary).collect();
        assert_eq!(spans.len(), 1, "one original summary attachment");
        let actual = spans[0];
        assert_eq!(actual.file_name, expected.file_name);
        let position = (actual.line_start, actual.column_start, actual.line_end, actual.column_end);
        assert_eq!(
            position,
            (expected.line_start, expected.column_start, expected.line_end, expected.column_end),
        );
        if let Some(first) = first_attachment {
            assert_eq!(position, first, "worker completion cannot choose another attachment");
        } else {
            first_attachment = Some(position);
        }
    }
}
