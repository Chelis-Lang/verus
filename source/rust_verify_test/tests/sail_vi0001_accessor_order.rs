#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0001: the `arrow_*` accessors that
// `enum_synthesize` generates were emitted in `HashMap` order, so their AIR
// declarations (and the solver input) changed order between runs. They must
// follow the order of the fields in the source.
#[test]
fn sail_vi0001_arrow_accessors_in_source_order() {
    let logs = tempfile::tempdir().expect("log directory");
    let log_dir = format!("--sail-raw={}", logs.path().display());
    let code = verus_code! {
        pub enum E {
            A { fa: u8, fb: u8 },
            B { fc: u8, fd: u8 },
            C { fe: u8, ff: u8, fg: u8 },
        }
        pub open spec fn sum(e: E) -> int {
            match e {
                E::A { .. } => e->fa + e->fb,
                E::B { .. } => e->fc + e->fd,
                E::C { .. } => e->fe + e->ff + e->fg,
            }
        }
        proof fn uses(e: E) { assert(sum(e) == sum(e)); }
    };
    let options = ["--sail-raw=--log", "--sail-raw=air", "--sail-raw=--log-dir", log_dir.as_str()];
    let result = verify_one_file("sail_vi0001_arrow_accessors_in_source_order", code, &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    let air = std::fs::read_to_string(logs.path().join("root.air")).expect("root.air log");
    let fields = ["fa", "fb", "fc", "fd", "fe", "ff", "fg"];
    let positions: Vec<usize> = fields
        .iter()
        .map(|f| {
            air.find(&format!("arrow_{f}")).unwrap_or_else(|| panic!("no arrow_{} in root.air", f))
        })
        .collect();
    let mut sorted = positions.clone();
    sorted.sort();
    // On stock Verus this order is random (1 in 5040 runs pass by chance).
    assert_eq!(positions, sorted, "arrow_* accessors are not declared in source order");
}
