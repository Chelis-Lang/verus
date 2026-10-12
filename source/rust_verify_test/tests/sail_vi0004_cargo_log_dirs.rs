#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

fn lib_with_bin() -> std::path::PathBuf {
    let current_exe = std::env::current_exe().unwrap();
    current_exe
        .ancestors()
        .nth(4)
        .unwrap()
        .join("rust_verify_test/tests/cargo-tests/verified/lib_with_bin")
}

// Sail fork regression test for VI-0004: every crate in a `cargo verus` run
// used the same `--log-dir`, and each crate's process cleared it, so the
// library's logs were deleted (or overwritten) by the binary's. Each crate
// must log to its own subdirectory.
#[test]
fn sail_vi0004_each_cargo_crate_keeps_its_logs() {
    let target = tempfile::tempdir().expect("target directory");
    let logs = tempfile::tempdir().expect("log directory");
    let log_dir = logs.path().to_str().expect("utf-8 path").to_owned();
    let run = run_cargo_verus_with_target(
        &["verify", "--", "--log", "air", "--log", "vir", "--log-dir", &log_dir],
        &lib_with_bin(),
        target.path(),
    );
    assert!(run.status.success(), "cargo verus verify failed");
    let lib = logs.path().join("lib_with_bin");
    let bin = logs.path().join("lib_with_bin-bin");
    let listing = list(logs.path());
    // The binary has no Verus items, so it writes crate-level logs only.
    assert!(lib.join("root.air").is_file(), "library logs missing: {:?}", listing);
    assert!(lib.join("crate.vir").is_file(), "library logs missing: {:?}", listing);
    assert!(bin.join("crate.vir").is_file(), "binary logs missing: {:?}", listing);
}

fn list(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(list(&path));
        } else {
            out.push(path.display().to_string());
        }
    }
    out.sort();
    out
}
