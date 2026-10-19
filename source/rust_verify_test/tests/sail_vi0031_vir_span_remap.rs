#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0031: Verus records span strings in the
// VIR it exports, and a standard library span reads `/rustc/<commit>/...` or
// `<sysroot>/lib/rustlib/src/rust/...` depending on whether the exporting
// toolchain had `rust-src`. `SAIL_VIR_SPAN_REMAP=<from>=<to>` rewrites that
// prefix of imported span strings, and nothing else; without it, imported
// strings are unchanged.

const CLIENT: &str = verus_code_str! {
    use vstd::prelude::*;
    fn f(v: &Vec<u8>) -> Vec<u8> {
        v.clone()
    }
};

// The trait-implementation names log of a run, which records the imported
// vstd's standard library trait implementations with their span strings.
fn impl_names(remap: Option<&str>) -> String {
    let root = tempfile::tempdir().expect("test directory");
    let logs = root.path().join("logs");
    let log_dir = format!("--sail-raw={}", logs.display());
    let options =
        ["vstd", "--sail-raw=--log", "--sail-raw=impl-names", "--sail-raw=--log-dir", &log_dir];
    // SAFETY: this file's only test sets the variable for its child runs.
    unsafe {
        match remap {
            Some(value) => std::env::set_var("SAIL_VIR_SPAN_REMAP", value),
            None => std::env::remove_var("SAIL_VIR_SPAN_REMAP"),
        }
    }
    let result = verify_one_file("sail_vi0031_vir_span_remap", CLIENT.to_string(), &options);
    unsafe { std::env::remove_var("SAIL_VIR_SPAN_REMAP") };
    assert!(result.is_ok(), "verification failed: {:?}", result);
    std::fs::read_to_string(logs.join("crate.impl_names")).expect("impl names log")
}

// Each line's span string, the last `   ###   ` field.
fn spans(log: &str) -> Vec<&str> {
    log.lines().filter_map(|line| line.rsplit("   ###   ").next()).collect()
}

#[test]
fn sail_vi0031_imported_span_prefix_is_remapped_only_when_requested() {
    let plain = impl_names(None);
    // The recorded standard library prefix: everything before `library/`.
    let std_span = spans(&plain)
        .into_iter()
        .find(|span| span.contains("/library/alloc/src/"))
        .expect("fixture must import a standard library trait implementation");
    let from = &std_span[..std_span.find("library/").expect("library path")];
    assert!(from.starts_with('/'), "unexpected standard library span {std_span}");

    let remapped = impl_names(Some(&format!("{from}=/sail-remapped/")));
    let plain_spans = spans(&plain);
    let remapped_spans = spans(&remapped);
    assert_eq!(plain_spans.len(), remapped_spans.len(), "the same implementations are logged");
    for (before, after) in plain_spans.iter().zip(&remapped_spans) {
        match before.strip_prefix(from) {
            Some(rest) => assert_eq!(*after, format!("/sail-remapped/{rest}")),
            None => assert_eq!(after, before, "spans outside the prefix are unchanged"),
        }
    }
    assert!(remapped_spans.iter().all(|span| !span.starts_with(from)));

    // A malformed value selects nothing.
    assert_eq!(impl_names(Some("not-absolute=/sail-remapped/")), plain);
}
