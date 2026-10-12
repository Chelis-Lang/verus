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

// Sail fork regression test for VI-0005: rust_verify recorded the forwarded
// Verus arguments as dep-info environment dependencies, but its second
// (compile) rustc pass rewrote the dep-info file without them, so Cargo kept
// a verified crate fresh when only the forwarded arguments changed.
#[test]
fn sail_vi0005_changed_verus_arguments_reverify() {
    let target = tempfile::tempdir().expect("target directory");
    let project = lib_with_bin();
    let run = |rlimit: &str| {
        let run = run_cargo_verus_with_target(
            &["verify", "--", "--rlimit", rlimit],
            &project,
            target.path(),
        );
        assert!(run.status.success(), "cargo verus verify failed");
        String::from_utf8_lossy(&run.stdout).into_owned() + &String::from_utf8_lossy(&run.stderr)
    };
    let first = run("10");
    assert!(first.contains("verification results::"), "first run did not verify:\n{}", first);
    let unchanged = run("10");
    assert!(
        !unchanged.contains("verification results::"),
        "unchanged arguments re-verified:\n{}",
        unchanged
    );
    let changed = run("11");
    assert!(
        changed.contains("verification results::"),
        "changed Verus arguments did not re-verify:\n{}",
        changed
    );
}
