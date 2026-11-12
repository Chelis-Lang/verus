//! Optional observations of the bytes actually imported/exported as VIR.
//! Never reopen a path to manufacture an earlier read/write checksum.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Serialize)]
pub(crate) struct Artifact {
    name: Option<String>,
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Default)]
struct Inventory {
    imports: Vec<Artifact>,
    exports: Vec<Artifact>,
    failed: bool,
}

static INVENTORY: Mutex<Inventory> =
    Mutex::new(Inventory { imports: Vec::new(), exports: Vec::new(), failed: false });

pub(crate) fn begin() {
    if let Ok(mut inventory) = INVENTORY.lock() {
        *inventory = Inventory::default();
    }
}

pub(crate) fn inventory() -> Option<(Vec<Artifact>, Vec<Artifact>)> {
    let inventory = INVENTORY.lock().ok()?;
    (!inventory.failed).then(|| (inventory.imports.clone(), inventory.exports.clone()))
}

struct Observed {
    digest: Sha256,
    bytes: Option<u64>,
}

impl Observed {
    fn new() -> Option<Self> {
        crate::sail_compiler_inputs::campaign_requested()
            .then(|| Self { digest: Sha256::new(), bytes: Some(0) })
    }
    fn accept(&mut self, bytes: &[u8]) {
        self.digest.update(bytes);
        self.bytes = self.bytes.and_then(|old| old.checked_add(bytes.len() as u64));
    }
    fn artifact(&self, file: &File, path: &Path, name: Option<&str>) -> Option<Artifact> {
        let bytes = self.bytes?;
        // A partial buffered read does not attest a whole imported artifact.
        if file.metadata().ok()?.len() != bytes {
            return None;
        }
        Some(Artifact {
            name: name.map(str::to_owned),
            path: std::path::absolute(path).ok()?.to_str()?.to_owned(),
            sha256: hex::encode(self.digest.clone().finalize()),
            bytes,
        })
    }
}

fn retain(artifact: Option<Artifact>, importing: bool) {
    if let Ok(mut inventory) = INVENTORY.lock() {
        if let Some(artifact) = artifact {
            let files = if importing { &mut inventory.imports } else { &mut inventory.exports };
            if files.len() >= 1024
                || files
                    .iter()
                    .any(|old| old.path == artifact.path || importing && old.name == artifact.name)
            {
                inventory.failed = true;
            } else {
                files.push(artifact);
            }
        } else {
            inventory.failed = true;
        }
    }
}

pub(crate) struct Reader {
    file: File,
    observed: Option<Observed>,
}
impl Reader {
    pub(crate) fn new(file: File) -> Self {
        Self { file, observed: Observed::new() }
    }
    pub(crate) fn retain(&self, name: &str, path: &str) {
        if let Some(observed) = &self.observed {
            retain(observed.artifact(&self.file, Path::new(path), Some(name)), true);
        }
    }
}
impl Read for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let result = self.file.read(bytes);
        if let Some(observed) = &mut self.observed {
            match &result {
                Ok(count) => observed.accept(&bytes[..*count]),
                Err(_) => observed.bytes = None,
            }
        }
        result
    }
}

pub(crate) struct Writer {
    file: File,
    observed: Option<Observed>,
}
impl Writer {
    pub(crate) fn new(file: File) -> Self {
        Self { file, observed: Observed::new() }
    }
    pub(crate) fn retain(&self, path: &Path, flushed: bool) {
        if let Some(observed) = &self.observed {
            retain(flushed.then(|| observed.artifact(&self.file, path, None)).flatten(), false);
        }
    }
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.file.write(bytes);
        if let Some(observed) = &mut self.observed {
            match &result {
                Ok(count) => observed.accept(&bytes[..*count]),
                Err(_) => observed.bytes = None,
            }
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
