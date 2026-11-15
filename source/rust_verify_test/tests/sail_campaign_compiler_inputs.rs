#![feature(rustc_private)]
#[macro_use]
mod common;

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn verify(project: &Path, normal: Option<&Path>, campaign: Option<(&Path, &str)>) -> Output {
    let clean = Command::new("cargo")
        .current_dir(project)
        .args(["clean", "-p", "campaign-input-fixture"])
        .output()
        .expect("clean owned fixture target");
    assert!(clean.status.success(), "{}", String::from_utf8_lossy(&clean.stderr));
    let mut command = Command::new(cargo_verus());
    // Both controls suppress Cargo's elapsed-time footer while every compiler
    // output byte remains subject to the original exact comparison.
    command.current_dir(project).args([
        "verify",
        "--lib",
        "--quiet",
        "--",
        "-Zchecksum-hash-algorithm=sha256",
    ]);
    for name in [
        "SAIL_COMPILER_INPUTS",
        "SAIL_COMPILER_INPUTS_BINDING",
        "SAIL_CAMPAIGN_COMPILER_INPUTS",
        "SAIL_CAMPAIGN_COMPILER_BINDING",
    ] {
        command.env_remove(name);
    }
    command.env("SAIL_TEST_CAMPAIGN_VALUE", "ok");
    if let Some(directory) = normal {
        command
            .env("SAIL_COMPILER_INPUTS", directory)
            .env("SAIL_COMPILER_INPUTS_BINDING", "a".repeat(64));
    }
    if let Some((directory, binding)) = campaign {
        command
            .env("SAIL_CAMPAIGN_COMPILER_INPUTS", directory)
            .env("SAIL_CAMPAIGN_COMPILER_BINDING", binding);
    }
    let output = command.output().expect("actual Cargo-Verus child");
    if let Some(case) = normal.or_else(|| campaign.map(|(path, _)| path)) {
        std::fs::write(case.with_extension("stdout"), &output.stdout).expect("retain stdout");
        std::fs::write(case.with_extension("stderr"), &output.stderr).expect("retain stderr");
    }
    output
}

fn document(directory: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(directory.join("inputs.json")).expect("original inputs"))
        .expect("original input JSON")
}

fn directories(root: &Path, name: &str) -> (PathBuf, PathBuf) {
    let normal = root.join(format!("{name}-normal"));
    let campaign = root.join(format!("{name}-campaign"));
    std::fs::create_dir(&normal).expect("normal observation directory");
    std::fs::create_dir(&campaign).expect("campaign observation directory");
    (normal, campaign)
}

fn check_artifacts(reads: &Value) {
    for kind in ["imports", "exports"] {
        let files = reads[kind].as_array().expect("explicit VIR inventory");
        assert!(!files.is_empty(), "fixture must actually import and export VIR");
        for file in files {
            let path = Path::new(file["path"].as_str().expect("actual artifact path"));
            assert_eq!(file["bytes"], std::fs::metadata(path).unwrap().len());
            let digest = Command::new("python3").args([
                "-c", "import hashlib,pathlib,sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())",
                path.to_str().unwrap()
            ]).output().expect("independent original artifact checksum");
            assert!(digest.status.success());
            assert_eq!(file["sha256"], String::from_utf8(digest.stdout).unwrap().trim());
            if kind == "imports" {
                assert_eq!(file["name"], "vstd");
            } else {
                assert!(file["name"].is_null());
            }
        }
    }
}

#[test]
fn rejected_proofs_keep_diagnostic_reads_without_successful_reuse_authority() {
    let root = tempfile::tempdir().expect("fixture directory");
    let retained = root.path().to_owned();
    std::mem::forget(root);
    let root = retained;
    let project = root.join("project");
    std::fs::create_dir_all(project.join("src")).expect("source directory");
    let project = project.canonicalize().expect("actual project directory");
    let vstd = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("source").join("vstd");
    std::fs::write(
        project.join("Cargo.toml"),
        format!(
            r#"[package]
name = "campaign-input-fixture"
version = "0.1.0"
edition = "2024"
[workspace]
[package.metadata.verus]
verify = true
[dependencies]
vstd = {{ path = {} }}
"#,
            serde_json::to_string(&vstd.to_str().expect("vstd path")).expect("quoted path")
        ),
    )
    .expect("manifest");
    let source = project.join("src/lib.rs");
    std::fs::write(
        &source,
        "use vstd::prelude::*; verus! { pub proof fn warm() { assert(true); } }",
    )
    .expect("warm source");
    assert!(verify(&project, None, None).status.success(), "dependency warmup");

    let good = "use vstd::prelude::*; verus! { pub proof fn tracked() { assert(env!(\"SAIL_TEST_CAMPAIGN_VALUE\")@ == \"ok\"@); } }";
    std::fs::write(&source, good).expect("passing source");
    let binding = "a".repeat(64);
    let (control_normal, _) = directories(&root, "normal-control");
    let control = verify(&project, Some(&control_normal), None);
    assert!(control.status.success());
    let (normal, campaign) = directories(&root, "passing");
    let passing = verify(&project, Some(&normal), Some((&campaign, &binding)));
    assert!(passing.status.success(), "{}", String::from_utf8_lossy(&passing.stderr));
    assert_eq!(document(&normal)["kind"], "sail-compiler-read-inputs");
    assert_eq!(document(&normal)["schema"], 2);
    let reads = document(&campaign);
    assert_eq!(reads["kind"], "sail-campaign-compiler-inputs");
    assert_eq!(reads["schema"], 3);
    assert_eq!(reads["binding"], binding);
    assert_eq!(reads["complete"], true);
    assert_eq!(reads["verifier_success"], true);
    assert_eq!(reads["coverage"], "verification-and-compilation");
    check_artifacts(&reads);
    assert_eq!(document(&normal)["phases"], document(&control_normal)["phases"]);
    assert_eq!(passing.stdout, control.stdout);
    assert_eq!(passing.stderr, control.stderr);
    assert_eq!(reads["source_environment"]["verification"]["SAIL_TEST_CAMPAIGN_VALUE"], "ok");
    assert!(reads["source_environment"]["verification"].get("CARGO_MANIFEST_DIR").is_none());
    assert_eq!(
        reads["driver_tracked_environment"]["CARGO_MANIFEST_DIR"],
        project.to_str().unwrap()
    );
    assert_eq!(
        reads["phases"]["verification"]["tracked_environment"]["SAIL_TEST_CAMPAIGN_VALUE"],
        "ok"
    );

    std::fs::write(&source, good.replace("== \"ok\"@", "== \"wrong\"@")).expect("rejected source");
    let reference = verify(&project, None, None);
    assert!(!reference.status.success());
    let (normal, campaign) = directories(&root, "rejected");
    let rejected = verify(&project, Some(&normal), Some((&campaign, &binding)));
    assert_eq!(rejected.status.code(), reference.status.code());
    assert_eq!(rejected.stdout, reference.stdout);
    assert_eq!(rejected.stderr, reference.stderr);
    assert!(!normal.join("inputs.json").exists(), "failed proof grants no old reuse observation");
    let reads = document(&campaign);
    assert_eq!(reads["verifier_success"], false);
    assert_eq!(reads["coverage"], "verification-only");
    assert_eq!(reads["complete"], true);
    assert_eq!(reads["phases"].as_object().expect("phases").len(), 1);
    assert_eq!(
        reads["phases"]["verification"]["tracked_environment"]["SAIL_TEST_CAMPAIGN_VALUE"],
        "ok"
    );
    assert!(reads["pid"].as_u64().expect("actual compiler pid") > 0);

    // Source reads remain source reads even when their value and name exactly
    // match an earlier Cargo driver declaration. Never infer origin by value.
    std::fs::write(&source, format!(
        "use vstd::prelude::*; verus! {{ pub proof fn tracked() {{ assert(env!(\"CARGO_MANIFEST_DIR\")@ == {:?}@); }} }}",
        project.to_str().unwrap()
    )).expect("source reads a driver-declared name");
    let (normal, campaign) = directories(&root, "source-and-driver");
    let both = verify(&project, Some(&normal), Some((&campaign, &binding)));
    assert!(both.status.success(), "{}", String::from_utf8_lossy(&both.stderr));
    let reads = document(&campaign);
    assert_eq!(
        reads["source_environment"]["verification"]["CARGO_MANIFEST_DIR"],
        project.to_str().unwrap()
    );
    assert_eq!(
        reads["driver_tracked_environment"]["CARGO_MANIFEST_DIR"],
        project.to_str().unwrap()
    );

    std::fs::write(&source, good.replace("== \"ok\"@", "== \"wrong\"@")).expect("rejected source");

    let (normal, campaign) = directories(&root, "foreign-binding");
    let foreign = "b".repeat(64);
    assert!(!verify(&project, Some(&normal), Some((&campaign, &foreign))).status.success());
    assert!(
        !campaign.join("inputs.json").exists(),
        "foreign request must not supply another observation"
    );

    std::fs::write(&source, "use vstd::prelude::*; verus! { pub fn bad() { let x: u8 = true; } }")
        .expect("early compiler error source");
    let (normal, campaign) = directories(&root, "early-error");
    assert!(!verify(&project, Some(&normal), Some((&campaign, &binding))).status.success());
    assert!(!normal.join("inputs.json").exists());
    assert!(!campaign.join("inputs.json").exists(), "no completed verification read set");
}
