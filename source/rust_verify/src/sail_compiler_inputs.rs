//! Optional compiler-read observations for Sail's disabled vstd experiment.
//!
//! These are the hashes retained in rustc's source map, not hashes obtained by
//! reopening dep-info paths. Verification and erased compilation write separate
//! records because the driver runs both compiler phases over the source.
//! Missing records disable the experiment; they never change compiler output.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use rustc_middle::ty::TyCtxt;
use rustc_span::SourceFile;
use serde_json::{Value, json};

const DIRECTORY: &str = "SAIL_COMPILER_INPUTS";
const BINDING: &str = "SAIL_COMPILER_INPUTS_BINDING";

/// Observe a compiler phase only when the external experiment requests it.
/// Failure leaves no usable successful observation and emits no diagnostics.
pub(crate) fn record(tcx: TyCtxt<'_>, phase: &str) {
    let Some(directory) = std::env::var_os(DIRECTORY) else {
        return;
    };
    let Ok(binding) = std::env::var(BINDING) else {
        return;
    };
    if binding.len() != 64 || !binding.bytes().all(|b| b.is_ascii_hexdigit()) {
        return;
    }
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() || !matches!(phase, "verification" | "compilation") {
        return;
    }
    let files = tcx.sess.source_map().files();
    let Some(sources) = source_records(files.iter().map(|file| &**file)) else {
        return;
    };
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
    let document = json!({
        "kind": "sail-compiler-read-inputs",
        "schema": 1,
        "phase": phase,
        "binding": binding,
        "pid": std::process::id(),
        "sources": sources,
        // These paths are explicitly NOT attested by reopening them. The
        // consumer must account for them separately or refuse publication.
        "extra_tracked_files": tracked,
        "tracked_environment": environment,
    });
    let Ok(bytes) = serde_json::to_vec(&document) else {
        return;
    };
    if bytes.len() > 4 * 1024 * 1024 {
        return;
    }
    // The caller creates a fresh private directory. Never overwrite another
    // record, including one emitted by an unexpected additional compiler pass.
    if let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(format!("{phase}.json")))
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
    use super::source_records;
    use rustc_span::{FileName, RealFileName, SourceFile, SourceFileHashAlgorithm};

    fn source(bytes: &str) -> SourceFile {
        SourceFile::new(
            FileName::Real(RealFileName::LocalPath("/sail/compiler-input.rs".into())),
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
}
