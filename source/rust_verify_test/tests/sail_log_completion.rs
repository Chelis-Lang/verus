#![feature(rustc_private)]
#[macro_use]
mod common;
use common::*;

// A specification-only bucket exercises a context with no solver process;
// ordinary and bit-vector proofs exercise the shared and spinoff owners.
const CODE: &str = verus_code_str! {
    mod specification_only {
        pub open spec fn identity(x: int) -> int { x }
    }
    mod checked {
        proof fn ordinary() { assert(true); }
        proof fn bits(x: u8) { assert(x & 0 == 0) by(bit_vector); }
    }
};

fn run_logs(logs: &std::path::Path, selected_logs: &[&str], spinoff: bool) {
    let log_path = format!("--sail-raw={}", logs.display());
    let mut options = vec!["vstd", "--crate-name=sail_log_completion"];
    for kind in selected_logs {
        options.push("--sail-raw=--log");
        options.push(kind);
    }
    options.extend(["--sail-raw=--log-dir", log_path.as_str()]);
    if spinoff {
        options.push("--sail-raw=-V");
        options.push("--sail-raw=spinoff-all");
    }
    let result = verify_one_file("sail_log_completion", CODE.to_string(), &options);
    assert!(result.is_ok(), "verification failed: {:?}", result);
}

#[test]
fn completed_bucket_and_spinoff_pairs() {
    for spinoff in [false, true] {
        let logs = tempfile::tempdir().expect("logs");
        run_logs(logs.path(), &["--sail-raw=air", "--sail-raw=smt-transcript"], spinoff);
        let mut zero = 0;
        let mut used = 0;
        for entry in std::fs::read_dir(logs.path()).expect("logs") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|s| s.to_str()) != Some("smt_transcript") {
                continue;
            }
            let transcript = std::fs::read_to_string(&path).expect("transcript");
            let air = std::fs::read_to_string(path.with_extension("air")).expect("paired AIR");
            assert_eq!(transcript.matches(";; SAIL-COMPLETE ").count(), 1);
            assert_eq!(air.matches(";; SAIL-COMPLETE ").count(), 1);
            if transcript == ";; SAIL-COMPLETE 1 transcript zero-process\n" {
                assert!(air.ends_with(";; SAIL-COMPLETE 1 air zero-process\n"));
                zero += 1;
            } else {
                assert!(
                    transcript.ends_with(";;;<<<\n;; SAIL-COMPLETE 1 transcript process-used\n")
                );
                assert!(air.ends_with(";; SAIL-COMPLETE 1 air process-used\n"));
                used += 1;
            }
        }
        assert!(zero > 0, "fixture must exercise a no-process context");
        assert!(used > 0, "fixture must exercise a process-used context");
    }
}

#[test]
fn independently_requested_logs_keep_their_original_format() {
    for selected in ["--sail-raw=air", "--sail-raw=smt-transcript"] {
        let logs = tempfile::tempdir().expect("logs");
        run_logs(logs.path(), &[selected], false);
        let mut count = 0;
        for entry in std::fs::read_dir(logs.path()).expect("logs") {
            let path = entry.expect("entry").path();
            let text = std::fs::read_to_string(path).expect("log");
            assert!(!text.contains(";; SAIL-COMPLETE "));
            count += 1;
        }
        assert!(count > 0, "fixture must create the selected logs");
    }
}
