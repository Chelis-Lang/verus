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
use rustc_span::SourceFile;
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
    let files = tcx.sess.source_map().files();
    let sources = source_records(files.iter().map(|file| &**file));
    let mut tracked: Vec<_> =
        tcx.sess.file_depinfo.borrow().iter().map(|path| path.as_str().to_string()).collect();
    tracked.sort();
    let environment: BTreeMap<_, _> = tcx
        .sess
        .env_depinfo
        .borrow()
        .iter()
        .map(|(name, value)| (name.as_str().to_string(), value.map(|v| v.as_str().to_string())))
        .collect();
    let document = sources.map(|sources| {
        json!({
            "sources": sources,
            // Reopened hashes do not attest these earlier reads. The consumer
            // must account for extra tracked files separately or refuse.
            "extra_tracked_files": tracked,
            "tracked_environment": environment,
        })
    });
    let document = document.filter(|value| {
        serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 4 * 1024 * 1024)
    });
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
    let document = json!({
        "kind": "sail-compiler-read-inputs",
        "schema": 1,
        "binding": request.binding,
        "pid": std::process::id(),
        "phases": pending.phases,
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
    use super::{Pending, Request, source_records};
    use rustc_span::{FileName, RealFileName, SourceFile, SourceFileHashAlgorithm};
    use serde_json::json;
    use std::collections::BTreeMap;

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
