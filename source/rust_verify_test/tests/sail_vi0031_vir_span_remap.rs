#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// Sail fork regression test for VI-0031: Verus records span strings in the
// VIR it exports, and a standard library span reads `/rustc/<commit>/...` or
// `<sysroot>/lib/rustlib/src/rust/...` depending on whether the exporting
// toolchain had `rust-src`. `SAIL_VIR_SPAN_REMAP=<from>=<to>` rewrites that
// prefix of imported span strings, and nothing else; without it, imported
// strings are unchanged. Verus prints such a string where it cannot resolve
// an imported label span, with `dbg!`, whose printed location must stay
// stock's.

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
    set_remap(remap);
    let result = verify_one_file("sail_vi0031_vir_span_remap", CLIENT.to_string(), &options);
    set_remap(None);
    assert!(result.is_ok(), "verification failed: {:?}", result);
    std::fs::read_to_string(logs.join("crate.impl_names")).expect("impl names log")
}

// Each line's span string, the last `   ###   ` field.
fn spans(log: &str) -> Vec<&str> {
    log.lines().filter_map(|line| line.rsplit("   ###   ").next()).collect()
}

// An extension impl with no matching standard library impl: the error labels
// core's `impl<T: Display> Display for &T`, an imported span Verus cannot
// resolve, so it prints the span's stored text with `dbg!` (found by Sail).
const NO_MATCHING_IMPL: &str = verus_code_str! {
    use vstd::prelude::*;
    use vstd::std_specs::fmt::DisplaySpecImpl;
    pub struct S;
    impl core::fmt::Display for S {
        #[verifier::external_body]
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            Ok(())
        }
    }
    impl<'a> DisplaySpecImpl for &'a S {}
};

fn set_remap(remap: Option<&str>) {
    // SAFETY: this file's tests run their child processes one at a time.
    unsafe {
        match remap {
            Some(value) => std::env::set_var("SAIL_VIR_SPAN_REMAP", value),
            None => std::env::remove_var("SAIL_VIR_SPAN_REMAP"),
        }
    }
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fallback_stderr(dir: &std::path::Path, remap: Option<&str>) -> String {
    let file = dir.join("test.rs");
    std::fs::write(&file, NO_MATCHING_IMPL).expect("write input");
    set_remap(remap);
    let output = run_verus(&[], dir, &file, true, false);
    set_remap(None);
    assert!(!output.status.success(), "the fixture must be rejected");
    String::from_utf8(output.stderr).expect("stderr")
}

// The `dbg!` lines of the fallback, and the stored span text it prints.
fn fallback(stderr: &str) -> (Vec<&str>, String) {
    let lines: Vec<&str> = stderr.lines().filter(|l| l.starts_with("[rust_verify/")).collect();
    let span = lines
        .iter()
        .find_map(|l| l.split("&sp.as_string = \"").nth(1))
        .expect("the fixture must reach the stored-span fallback")
        .trim_end_matches('"')
        .to_owned();
    (lines, span)
}

#[test]
fn sail_vi0031_fallback_prints_the_dbg_location_of_the_source() {
    let _serial = SERIAL.lock();
    let root = tempfile::tempdir().expect("test directory");
    let stderr = fallback_stderr(root.path(), None);
    let (lines, _) = fallback(&stderr);
    // The location `dbg!` prints is that of the reporter's fallback line; the
    // fork's CI requires that line to be upstream's (check-dbg-locations.py).
    let source = include_str!("../../rust_verify/src/verifier.rs");
    let line = 1 + source
        .lines()
        .position(|l| l.trim() == "dbg!(&note, &sp.as_string);")
        .expect("reporter fallback");
    let location = format!("[rust_verify/src/verifier.rs:{line}:17]");
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| l.starts_with(&location)), "{:?} is not at {}", lines, location);
}

#[test]
fn sail_vi0031_fallback_span_text_is_remapped_only_when_requested() {
    let _serial = SERIAL.lock();
    let root = tempfile::tempdir().expect("test directory");
    let plain = fallback_stderr(root.path(), None);
    let (_, span) = fallback(&plain);
    let from = &span[..span.find("library/core/src/").expect("a core span")];
    assert!(from.starts_with('/'), "unexpected span {}", span);
    let remapped = fallback_stderr(root.path(), Some(&format!("{from}=/sail-remapped/")));
    // Exactly the stored text changes; every other byte of stderr is the same.
    assert_eq!(remapped, plain.replace(from, "/sail-remapped/"));
    assert_ne!(remapped, plain);
    assert_eq!(fallback_stderr(root.path(), Some("relative=/sail-remapped/")), plain);
}

#[test]
fn sail_vi0031_imported_span_prefix_is_remapped_only_when_requested() {
    let _serial = SERIAL.lock();
    let plain = impl_names(None);
    // The recorded standard library prefix: everything before `library/`.
    let std_span = spans(&plain)
        .into_iter()
        .find(|span| span.contains("/library/alloc/src/"))
        .expect("fixture must import a standard library trait implementation");
    let from = &std_span[..std_span.find("library/").expect("library path")];
    assert!(from.starts_with('/'), "unexpected standard library span {}", std_span);

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
