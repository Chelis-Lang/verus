//! Opt-in observations from Cargo's ordinary native Rust compiler path.
//!
//! This is a single genuine compiler phase, not a verification record. The
//! consumer must bind the actual child PID, require its successful exit and
//! separately account for native dependencies and extra tracked inputs.

use rustc_middle::ty::TyCtxt;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(PartialEq, Eq)]
struct Request {
    directory: PathBuf,
    binding: String,
}

struct Pending {
    request: Request,
    inputs: Option<Value>,
    failed: bool,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

fn request() -> Option<Request> {
    let directory = PathBuf::from(std::env::var_os("SAIL_NATIVE_COMPILER_INPUTS")?);
    let binding = std::env::var("SAIL_NATIVE_COMPILER_INPUTS_BINDING").ok()?;
    if !directory.is_absolute()
        || binding.len() != 64
        || !binding.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    Some(Request { directory, binding })
}

impl Pending {
    fn insert(&mut self, request: &Request, inputs: Option<Value>) {
        // A second callback or a changed request cannot replace the earlier
        // compiler observation, even if a later one looks admissible.
        if self.request != *request || self.inputs.is_some() || inputs.is_none() {
            self.failed = true;
        } else {
            self.inputs = inputs;
        }
    }

    fn document(&self, request: &Request, pid: u32) -> Option<Value> {
        if self.failed || self.request != *request || pid == 0 {
            return None;
        }
        Some(json!({
            "kind": "sail-native-compiler-read-inputs",
            "schema": 1,
            "binding": request.binding,
            "pid": pid,
            "complete": true,
            "inputs": self.inputs.as_ref()?,
        }))
    }
}

/// Capture the actual source map after native Rust analysis. The callback
/// always continues normal compilation; observation failure changes no output.
pub(crate) fn record(tcx: TyCtxt<'_>) {
    let Some(request) = request() else {
        return;
    };
    let inputs = crate::sail_compiler_inputs::phase_record(tcx);
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    if let Some(pending) = pending.as_mut() {
        pending.insert(&request, inputs);
    } else {
        let mut state = Pending { request, inputs: None, failed: false };
        let same_request = Request {
            directory: state.request.directory.clone(),
            binding: state.request.binding.clone(),
        };
        state.insert(&same_request, inputs);
        *pending = Some(state);
    }
}

/// Only a normal return from the genuine native compiler can write this
/// completion record. The consumer must additionally check the child's exit.
pub(crate) fn finish() {
    let Some(request) = request() else {
        return;
    };
    let Ok(pending) = PENDING.lock() else {
        return;
    };
    let Some(document) =
        pending.as_ref().and_then(|pending| pending.document(&request, std::process::id()))
    else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(&document) else {
        return;
    };
    if bytes.len() > 8 * 1024 * 1024 {
        return;
    }
    if let Ok(mut file) =
        OpenOptions::new().write(true).create_new(true).open(request.directory.join("inputs.json"))
    {
        let _ = file.write_all(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::{Pending, Request};
    use serde_json::json;

    fn request() -> Request {
        Request { directory: "/sail/native-inputs".into(), binding: "a".repeat(64) }
    }

    #[test]
    fn native_observation_is_distinct_complete_and_bound() {
        let mut pending = Pending { request: request(), inputs: None, failed: false };
        assert!(pending.document(&request(), 42).is_none());
        pending.insert(
            &request(),
            Some(json!({"sources":{"lib.rs":{"checksum":"blake3=abc","size":3}}})),
        );
        let document = pending.document(&request(), 42).unwrap();
        assert_eq!(document["kind"], "sail-native-compiler-read-inputs");
        assert_eq!(document["pid"], 42);
        assert!(pending.document(&request(), 0).is_none());
        let mut other = request();
        other.binding = "b".repeat(64);
        assert!(pending.document(&other, 42).is_none());
        other = request();
        other.directory = "/sail/other".into();
        assert!(pending.document(&other, 42).is_none());
        pending.insert(&request(), Some(json!({"sources":{}})));
        assert!(pending.document(&request(), 42).is_none());
    }

    #[test]
    fn failed_or_changed_native_observations_never_recover() {
        let mut pending = Pending { request: request(), inputs: None, failed: false };
        pending.insert(&request(), None);
        pending.insert(&request(), Some(json!({"sources":{}})));
        assert!(pending.document(&request(), 42).is_none());
        let mut pending = Pending { request: request(), inputs: None, failed: false };
        let mut other = request();
        other.binding = "b".repeat(64);
        pending.insert(&other, Some(json!({"sources":{}})));
        assert!(pending.document(&request(), 42).is_none());
    }
}
