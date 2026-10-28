//! Optional compiler-read observations for Sail's disabled vstd experiment.
//!
//! Hashes come from rustc's source map, never reopened dep-info paths.
//! Verification and erased compilation are distinct actual source reads.
//! Observations remain in process memory until both phases complete. Missing,
//! duplicate or failed phases cannot produce a complete final observation.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_span::{SourceFile, symbol::Symbol};
use serde_json::{Value, json};

const DIRECTORY: &str = "SAIL_COMPILER_INPUTS";
const BINDING: &str = "SAIL_COMPILER_INPUTS_BINDING";

#[derive(PartialEq, Eq)]
struct Request {
    directory: PathBuf,
    binding: String,
}

struct Pending {
    request: Request,
    phases: BTreeMap<String, Value>,
    failed: bool,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

// Unknown origin cannot authorize reuse. An empty known map means a direct
// invocation has no Cargo driver bookkeeping; an owned absent value is a
// distinct, explicitly declared protocol read.
static DRIVER_ENVIRONMENT: Mutex<Option<BTreeMap<String, Option<String>>>> = Mutex::new(None);

pub(crate) fn begin_driver_environment(via_cargo: bool) {
    if let Ok(mut environment) = DRIVER_ENVIRONMENT.lock() {
        *environment = if via_cargo { None } else { Some(BTreeMap::new()) };
    }
}

pub(crate) fn driver_common(value: Option<Option<String>>) {
    if let Ok(mut environment) = DRIVER_ENVIRONMENT.lock() {
        *environment = value.map(|value| {
            BTreeMap::from([(crate::cargo_verus::VERUS_DRIVER_ARGS.to_owned(), value)])
        });
    }
}

fn driver_environment() -> Option<BTreeMap<String, Option<String>>> {
    DRIVER_ENVIRONMENT.lock().ok()?.clone()
}

fn temporary_common(
    names: impl Iterator<Item = String>,
    driver: &BTreeMap<String, Option<String>>,
) -> Option<(String, Option<String>)> {
    let common = crate::cargo_verus::VERUS_DRIVER_ARGS;
    if names.into_iter().any(|name| name == common) {
        None
    } else {
        driver.get(common).map(|value| (common.to_owned(), value.clone()))
    }
}

struct TemporaryCommon<'a> {
    session: &'a Session,
    inserted: Option<(Symbol, Option<Symbol>)>,
}

impl Drop for TemporaryCommon<'_> {
    fn drop(&mut self) {
        if let Some(value) = self.inserted {
            self.session.env_depinfo.borrow_mut().remove(&value);
        }
    }
}

/// Preserve Cargo's protocol declaration only while writing dep-info. The
/// source-read set remains unpolluted before and after the synchronous writer,
/// including unwinding. Existing source entries are never replaced or removed.
pub(crate) fn write_dep_info(tcx: TyCtxt<'_>) {
    let driver = driver_environment().unwrap_or_default();
    with_driver_common(tcx.sess, &driver, || rustc_interface::passes::write_dep_info(tcx));
}

fn with_driver_common<T>(
    session: &Session,
    driver: &BTreeMap<String, Option<String>>,
    write: impl FnOnce() -> T,
) -> T {
    let entry = temporary_common(
        session.env_depinfo.borrow().iter().map(|(name, _)| name.as_str().to_owned()),
        driver,
    );
    let inserted = entry.and_then(|(name, value)| {
        let value = (Symbol::intern(&name), value.as_deref().map(Symbol::intern));
        session.env_depinfo.borrow_mut().insert(value).then_some(value)
    });
    let _temporary = TemporaryCommon { session, inserted };
    write()
}

fn environment_records(
    values: impl Iterator<Item = (String, Option<String>)>,
) -> Option<BTreeMap<String, Option<String>>> {
    let mut environment = BTreeMap::new();
    for (name, value) in values {
        if environment.insert(name, value.clone()).is_some_and(|old| old != value) {
            return None;
        }
    }
    Some(environment)
}

fn request() -> Option<Request> {
    let directory = std::env::var_os(DIRECTORY)?;
    let binding = std::env::var(BINDING).ok()?;
    if binding.len() != 64 || !binding.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let directory = PathBuf::from(directory);
    directory.is_absolute().then_some(Request { directory, binding })
}

impl Pending {
    fn insert(&mut self, phase: &str, document: Option<Value>) {
        if !matches!(phase, "verification" | "compilation") || self.phases.contains_key(phase) {
            self.failed = true;
        } else if let Some(document) = document {
            self.phases.insert(phase.to_owned(), document);
        } else {
            self.failed = true;
        }
    }

    fn complete(&self) -> bool {
        !self.failed
            && self.phases.contains_key("verification")
            && self.phases.contains_key("compilation")
    }
}

/// Retain each requested compiler phase. Failures latch refusal without
/// changing compiler output; the consumer must require a complete record.
pub(crate) fn record(tcx: TyCtxt<'_>, phase: &str) {
    let Some(request) = request() else {
        return;
    };
    let document = phase_record(tcx);
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    if let Some(pending) = pending.as_mut() {
        if pending.request != request {
            pending.failed = true;
        }
        pending.insert(phase, document);
    } else {
        let mut state = Pending { request, phases: BTreeMap::new(), failed: false };
        state.insert(phase, document);
        *pending = Some(state);
    }
}

/// Compiler-owned inputs for one actual phase. The caller supplies its own
/// request, multiplicity and successful-completion protocol.
pub(crate) fn phase_record(tcx: TyCtxt<'_>) -> Option<Value> {
    let files = tcx.sess.source_map().files();
    let sources = source_records(files.iter().map(|file| &**file));
    let mut tracked: Vec<_> =
        tcx.sess.file_depinfo.borrow().iter().map(|path| path.as_str().to_string()).collect();
    tracked.sort();
    let environment =
        environment_records(tcx.sess.env_depinfo.borrow().iter().map(|(name, value)| {
            (name.as_str().to_string(), value.map(|v| v.as_str().to_string()))
        }))?;
    let document = sources.map(|sources| {
        json!({
            "sources": sources,
            // Reopened hashes do not attest these earlier reads. The consumer
            // must account for extra tracked files separately or refuse.
            "extra_tracked_files": tracked,
            "tracked_environment": environment,
        })
    });
    document
        .filter(|value| serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 4 * 1024 * 1024))
}

/// Called once after successful verification and erased compilation. Only
/// a complete pair can create the record. The consumer must independently
/// require the actual compiler child to exit successfully.
pub(crate) fn finish(success: bool) {
    let Some(request) = request().filter(|_| success) else {
        return;
    };
    let Ok(pending) = PENDING.lock() else {
        return;
    };
    let Some(pending) = pending.as_ref() else {
        return;
    };
    if pending.request != request || !pending.complete() {
        return;
    }
    let Some(driver_environment) = driver_environment() else {
        return;
    };
    let document = json!({
        "kind": "sail-compiler-read-inputs",
        "schema": 2,
        "binding": request.binding,
        "pid": std::process::id(),
        "phases": pending.phases,
        "driver_environment": driver_environment,
        "complete": true,
    });
    let Ok(bytes) = serde_json::to_vec(&document) else {
        return;
    };
    if bytes.len() > 8 * 1024 * 1024 {
        return;
    }
    // The caller creates a fresh private directory. Never overwrite a record.
    // Phase multiplicity is checked in memory before this completion write.
    if let Ok(mut file) =
        OpenOptions::new().write(true).create_new(true).open(request.directory.join("inputs.json"))
    {
        let _ = file.write_all(&bytes);
    }
}

fn source_records<'a>(
    files: impl Iterator<Item = &'a SourceFile>,
) -> Option<BTreeMap<String, Value>> {
    let mut sources = BTreeMap::new();
    for file in files.filter(|file| file.is_real_file() && !file.is_imported()) {
        if sources.len() >= 16_384 {
            return None;
        }
        let checksum = file.checksum_hash?;
        let name = file.name.prefer_local_unconditionally().to_string();
        if sources
            .insert(
                name,
                json!({
                    "checksum": checksum.to_string(),
                    "size": file.unnormalized_source_len,
                }),
            )
            .is_some()
        {
            return None;
        }
    }
    (!sources.is_empty()).then_some(sources)
}

#[cfg(test)]
mod tests {
    use super::{Pending, Request, environment_records, source_records, temporary_common};
    use rustc_span::{FileName, RealFileName, SourceFile, SourceFileHashAlgorithm};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn common_guard_preserves_real_compiler_reads_on_return_and_unwind() {
        let executable = std::env::current_exe().expect("test executable");
        for source_reads in [false, true] {
            for value in [None, Some(""), Some("driver-options")] {
                let mut child = std::process::Command::new(&executable);
                child.args([
                    "--exact",
                    "sail_compiler_inputs::tests::common_guard_compiler_helper",
                    "--nocapture",
                ]);
                child.env("SAIL_TEST_COMMON_GUARD", if source_reads { "source" } else { "driver" });
                match value {
                    Some(value) => {
                        child.env(crate::cargo_verus::VERUS_DRIVER_ARGS, value);
                    }
                    None => {
                        child.env_remove(crate::cargo_verus::VERUS_DRIVER_ARGS);
                    }
                }
                let output = child.output().expect("ordinary compiler helper");
                assert!(
                    output.status.success(),
                    "source={source_reads}, value={value:?}: {}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    #[test]
    fn common_guard_compiler_helper() {
        let Ok(mode) = std::env::var("SAIL_TEST_COMMON_GUARD") else {
            return;
        };
        let directory =
            std::env::temp_dir().join(format!("sail-common-guard-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("fresh compiler fixture");
        let source = directory.join("input.rs");
        std::fs::write(&source, if mode == "source" {
            "#![no_std]\npub const OBSERVED: Option<&str> = option_env!(\" __VERUS_DRIVER_ARGS__\");\n"
        } else { "#![no_std]\npub const UNOBSERVED: u8 = 0;\n" }).expect("fixture source");
        let sysroot = std::process::Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .expect("pinned rustc sysroot");
        assert!(sysroot.status.success());
        struct GuardCallbacks {
            source_reads: bool,
        }
        impl rustc_driver::Callbacks for GuardCallbacks {
            fn after_expansion<'tcx>(
                &mut self,
                _compiler: &rustc_interface::interface::Compiler,
                tcx: rustc_middle::ty::TyCtxt<'tcx>,
            ) -> rustc_driver::Compilation {
                let common = crate::cargo_verus::VERUS_DRIVER_ARGS;
                let value = std::env::var(common).ok();
                let driver = BTreeMap::from([(common.to_owned(), value.clone())]);
                let before = tcx.sess.env_depinfo.borrow().clone();
                assert_eq!(
                    before.iter().any(|(name, _)| name.as_str() == common),
                    self.source_reads
                );
                if self.source_reads {
                    assert!(before.iter().any(|(name, observed)| name.as_str() == common
                        && observed.map(|value| value.as_str().to_owned()) == value));
                }
                super::with_driver_common(tcx.sess, &driver, || {
                    assert!(
                        tcx.sess
                            .env_depinfo
                            .borrow()
                            .iter()
                            .any(|(name, observed)| name.as_str() == common
                                && observed.map(|value| value.as_str().to_owned()) == value)
                    );
                    rustc_interface::passes::write_dep_info(tcx);
                });
                assert_eq!(*tcx.sess.env_depinfo.borrow(), before);
                let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::with_driver_common(tcx.sess, &driver, || {
                        // Exercise ordinary unwinding without invoking the compiler's global ICE hook.
                        std::panic::resume_unwind(Box::new("intentional guard unwind"));
                    });
                }));
                assert!(unwind.is_err());
                assert_eq!(*tcx.sess.env_depinfo.borrow(), before);
                rustc_driver::Compilation::Stop
            }
        }
        let arguments = vec![
            "rustc".to_owned(),
            source.to_str().expect("fixture path").to_owned(),
            "--crate-type=lib".to_owned(),
            "--emit=dep-info,metadata".to_owned(),
            "--out-dir".to_owned(),
            directory.to_str().expect("fixture output").to_owned(),
            "--sysroot".to_owned(),
            String::from_utf8(sysroot.stdout).expect("sysroot UTF-8").trim().to_owned(),
        ];
        rustc_driver::run_compiler(
            &arguments,
            &mut GuardCallbacks { source_reads: mode == "source" },
        );
        let dep_info = std::fs::read_to_string(directory.join("input.d")).expect("actual dep-info");
        assert!(dep_info.lines().any(|line| line.starts_with("# env-dep: __VERUS_DRIVER_ARGS__")));
        std::fs::remove_dir_all(directory).expect("fixture cleanup");
    }

    #[test]
    fn common_bookkeeping_never_replaces_an_actual_source_read() {
        let common = crate::cargo_verus::VERUS_DRIVER_ARGS.to_owned();
        let driver = BTreeMap::from([(common.clone(), Some("driver".to_owned()))]);
        assert_eq!(
            temporary_common(std::iter::empty(), &driver),
            Some((common.clone(), Some("driver".to_owned())))
        );
        // Even a different value, or an explicit absent-value read, belongs
        // to the source and must not be replaced by the driver's declaration.
        for value in [None, Some("driver".to_owned()), Some("source".to_owned())] {
            let source = BTreeMap::from([(common.clone(), value.clone())]);
            assert_eq!(temporary_common(source.keys().cloned(), &driver), None);
            assert_eq!(
                environment_records(source.into_iter()),
                Some(BTreeMap::from([(common.clone(), value)]))
            );
        }
    }

    #[test]
    fn absent_driver_value_and_unknown_origin_are_not_source_absence() {
        let common = crate::cargo_verus::VERUS_DRIVER_ARGS.to_owned();
        assert_eq!(temporary_common(std::iter::empty(), &BTreeMap::new()), None);
        assert_eq!(
            temporary_common(std::iter::empty(), &BTreeMap::from([(common.clone(), None)])),
            Some((common, None))
        );
    }

    #[test]
    fn conflicting_phase_values_never_collapse_to_one_input() {
        let common = crate::cargo_verus::VERUS_DRIVER_ARGS.to_owned();
        assert!(
            environment_records(
                [
                    (common.clone(), Some("first".to_owned())),
                    (common.clone(), Some("second".to_owned()))
                ]
                .into_iter()
            )
            .is_none()
        );
        assert!(
            environment_records(
                [(common.clone(), None), (common.clone(), Some("second".to_owned()))].into_iter()
            )
            .is_none()
        );
        assert_eq!(
            environment_records(
                [
                    (common.clone(), Some("same".to_owned())),
                    (common.clone(), Some("same".to_owned()))
                ]
                .into_iter()
            ),
            Some(BTreeMap::from([(common, Some("same".to_owned()))]))
        );
    }

    fn source(bytes: &str) -> SourceFile {
        SourceFile::new(
            FileName::Real(RealFileName::from_virtual_path(std::path::Path::new(
                "/sail/compiler-input.rs",
            ))),
            bytes.to_owned(),
            SourceFileHashAlgorithm::Md5,
            Some(SourceFileHashAlgorithm::Blake3),
        )
        .unwrap()
    }

    #[test]
    fn source_records_keep_the_compiler_read_not_a_later_filesystem_hash() {
        let original = source("original\r\n");
        let modified = source("modified\r\n");
        let first = source_records(std::iter::once(&original)).unwrap();
        let second = source_records(std::iter::once(&modified)).unwrap();
        assert_ne!(first, second);
        assert_eq!(first["/sail/compiler-input.rs"]["size"], 10);
        assert_eq!(original.src.as_ref().unwrap().as_str(), "original\n");
        assert!(source_records([&original, &modified].into_iter()).is_none());
    }

    #[test]
    fn duplicate_failed_and_missing_compiler_phases_never_complete() {
        let mut pending = Pending {
            request: Request { directory: "/sail/inputs".into(), binding: "0".repeat(64) },
            phases: BTreeMap::new(),
            failed: false,
        };
        pending.insert("verification", Some(json!({})));
        assert!(!pending.complete());
        pending.insert("compilation", Some(json!({})));
        assert!(pending.complete());
        pending.insert("verification", Some(json!({})));
        assert!(!pending.complete());
        pending.failed = false;
        pending.phases.clear();
        pending.insert("verification", None);
        pending.insert("verification", Some(json!({})));
        pending.insert("compilation", Some(json!({})));
        assert!(!pending.complete());
    }
}
