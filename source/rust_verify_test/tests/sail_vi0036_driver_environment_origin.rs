#![feature(rustc_private)]
#[macro_use]
mod common;

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const COMMON: &str = " __VERUS_DRIVER_ARGS__";

fn cargo_verus() -> PathBuf {
    assert!(std::env::var_os("VERUS_IN_VARGO").is_some());
    let executable = std::env::current_exe().expect("test executable");
    let profile = executable.parent().expect("deps").parent().expect("profile");
    profile
        .ancestors()
        .nth(2)
        .expect("source directory")
        .join("target-verus")
        .join(profile.file_name().expect("profile name"))
        .join(if cfg!(windows) { "cargo-verus.exe" } else { "cargo-verus" })
}

fn verify(project: &Path, observation: Option<&Path>) -> Output {
    let mut command = Command::new(cargo_verus());
    command.current_dir(project).args([
        "verify",
        "--lib",
        "--",
        "-Zchecksum-hash-algorithm=sha256",
    ]);
    command.env_remove("SAIL_COMPILER_INPUTS").env_remove("SAIL_COMPILER_INPUTS_BINDING");
    if let Some(directory) = observation {
        command
            .env("SAIL_COMPILER_INPUTS", directory)
            .env("SAIL_COMPILER_INPUTS_BINDING", "a".repeat(64));
    }
    command.output().expect("ordinary Cargo-Verus invocation")
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Ordinary Cargo-Verus compiles the actual source in GHOST and EXEC. This is
// producer evidence only: the fixture grants no retained proof authority and
// installs no private compiler/process provider.
#[test]
fn cargo_driver_bookkeeping_is_disjoint_from_actual_phase_reads() {
    let root = tempfile::tempdir().expect("fixture directory");
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join("src")).expect("fixture source directory");
    let vstd = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("source").join("vstd");
    let manifest = format!(
        r#"[package]
name = "driver-origin-fixture"
version = "0.1.0"
edition = "2024"

[workspace]

[package.metadata.verus]
verify = true

[dependencies]
vstd = {{ path = {} }}
"#,
        serde_json::to_string(&vstd.to_str().expect("vstd path")).expect("quoted path")
    );
    std::fs::write(project.join("Cargo.toml"), manifest).expect("manifest");
    let source = project.join("src/lib.rs");
    std::fs::write(
        &source,
        "use vstd::prelude::*; verus! { pub proof fn warm() { assert(true); } }",
    )
    .expect("warmup source");
    // Build dependencies normally before requesting a per-target observation.
    // Later source edits schedule only this requested crate; each gets a fresh
    // observation directory and the original normal Cargo environment.
    succeeded(&verify(&project, None));
    for (index, (ghost, exec)) in
        [(false, false), (true, false), (false, true), (true, true)].iter().copied().enumerate()
    {
        let mut code = format!("use vstd::prelude::*;\n// fixture revision {index}\n");
        if exec {
            code.push_str("#[cfg(not(verus_keep_ghost_body))]\npub const EXEC_READ: Option<&str> = option_env!(\" __VERUS_DRIVER_ARGS__\");\n");
        }
        code.push_str("verus! { pub proof fn checked() {\n");
        if ghost {
            code.push_str("let observed = option_env!(\" __VERUS_DRIVER_ARGS__\");\nassert(match observed { Some(_) => true, None => true });\n");
        } else {
            code.push_str("assert(true);\n");
        }
        code.push_str("} }\n");
        std::fs::write(&source, code).expect("fixture revision");
        let directory = root.path().join(format!("observation-{index}"));
        std::fs::create_dir(&directory).expect("fresh observation");
        let output = verify(&project, Some(&directory));
        succeeded(&output);
        let document: Value = serde_json::from_slice(
            &std::fs::read(directory.join("inputs.json")).expect("actual compiler observation"),
        )
        .expect("observation JSON");
        assert_eq!(document["schema"], 2);
        assert_eq!(document["complete"], true);
        assert!(document["pid"].as_u64().is_some_and(|pid| pid > 0));
        let driver = document["driver_environment"].as_object().expect("driver environment");
        assert_eq!(driver.len(), 1);
        let effective = driver.get(COMMON).expect("actual driver declaration");
        assert!(effective.is_string());
        for (phase, source_read) in [("verification", ghost), ("compilation", exec)] {
            let phase = &document["phases"][phase];
            assert!(
                phase["sources"]
                    .as_object()
                    .expect("sources")
                    .keys()
                    .any(|path| Path::new(path).ends_with("src/lib.rs"))
            );
            let observed = phase["tracked_environment"].as_object().expect("tracked environment");
            assert_eq!(observed.get(COMMON), source_read.then_some(effective));
        }
    }
}
