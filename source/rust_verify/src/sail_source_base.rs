//! Sail: registry sources that a prebuilt crate names by rustc's virtual
//! rustc-dev source base.
//!
//! Sail ships vstd and its dependency closure prebuilt (Sail ADR 0048). The
//! release compiles them with `--remap-path-prefix <registry source
//! directory>=<virtual>`, where `<virtual>` is the virtual rustc-dev source
//! base this rustc was built with (`/rustc-dev/<rustc commit hash>`). When a
//! later compilation imports such a crate, rustc translates every file name
//! under that base to `real_rustc_dev_source_base_dir` joined with the rest of
//! the name (`rustc_metadata` `imported_source_file`) and reads the source
//! from there. Pointing that option at the user's own registry source
//! directory therefore makes diagnostics name and quote the user's real file,
//! exactly as for a crate compiled on the user's machine.
//!
//! `SAIL_RUSTC_DEV_SOURCE_BASE=<virtual>=<real>` selects this. Without the
//! variable nothing changes. Verus's exported VIR also records each source
//! file's canonical path for files rustc did not import (`FileStartEndPos`);
//! [`exported_name`] records those under `<virtual>` and [`imported_name`]
//! maps them back to `<real>`.

use std::path::PathBuf;

/// The variable naming the virtual base and the real registry source
/// directory, as `<virtual>=<real>`.
pub const ENV: &str = "SAIL_RUSTC_DEV_SOURCE_BASE";

struct Mapping {
    virtual_base: PathBuf,
    real_base: PathBuf,
}

fn mapping() -> Option<Mapping> {
    let value = std::env::var_os(ENV)?;
    let (virtual_base, real_base) = value.to_str()?.split_once('=')?;
    let mapping =
        Mapping { virtual_base: PathBuf::from(virtual_base), real_base: PathBuf::from(real_base) };
    (mapping.virtual_base.is_absolute() && mapping.real_base.is_absolute()).then_some(mapping)
}

/// Points rustc's rustc-dev source translation at the real registry source
/// directory. Every compilation the driver runs calls this from its
/// `Callbacks::config`.
pub fn configure(config: &mut rustc_interface::interface::Config) {
    if let Some(mapping) = mapping() {
        config.opts.real_rustc_dev_source_base_dir = Some(mapping.real_base);
    }
}

/// The canonical form of the real base, as Verus records exported file names
/// canonically.
fn canonical_real(mapping: &Mapping) -> PathBuf {
    mapping.real_base.canonicalize().unwrap_or_else(|_| mapping.real_base.clone())
}

/// The name an exported VIR crate records for a local source file: under the
/// virtual base when the file lies in the real registry source directory.
pub(crate) fn exported_name(path: PathBuf) -> PathBuf {
    let Some(mapping) = mapping() else {
        return path;
    };
    match path.strip_prefix(canonical_real(&mapping)) {
        Ok(rest) => mapping.virtual_base.join(rest),
        Err(_) => path,
    }
}

/// The file an imported VIR crate's recorded name refers to on this machine:
/// the canonical real registry source directory for a name under the virtual
/// base, as Verus would have recorded it had the crate been compiled here.
pub(crate) fn imported_name(path: PathBuf) -> PathBuf {
    let Some(mapping) = mapping() else {
        return path;
    };
    match path.strip_prefix(&mapping.virtual_base) {
        Ok(rest) => canonical_real(&mapping).join(rest),
        Err(_) => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn with_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        // SAFETY: library tests that touch this variable run in this one test.
        unsafe {
            match value {
                Some(value) => std::env::set_var(ENV, value),
                None => std::env::remove_var(ENV),
            }
        }
        let result = f();
        unsafe { std::env::remove_var(ENV) };
        result
    }

    #[test]
    fn names_map_between_virtual_and_real_bases_only_when_selected() {
        let dir = std::env::temp_dir().join(format!("sail-source-base-{}", std::process::id()));
        let real = dir.join("registry/src/index");
        std::fs::create_dir_all(&real).unwrap();
        let canonical = real.canonicalize().unwrap();
        let file = canonical.join("vstd-0.1.0/src/seq.rs");
        let virtual_file = Path::new("/rustc-dev/0123/vstd-0.1.0/src/seq.rs");
        let value = format!("/rustc-dev/0123={}", real.display());
        with_env(Some(&value), || {
            assert_eq!(exported_name(file.clone()), virtual_file);
            assert_eq!(imported_name(virtual_file.to_path_buf()), file);
            // Names elsewhere are unchanged in both directions.
            assert_eq!(
                exported_name(PathBuf::from("/elsewhere/a.rs")),
                Path::new("/elsewhere/a.rs")
            );
            assert_eq!(
                imported_name(PathBuf::from("/rustc/0123/a.rs")),
                Path::new("/rustc/0123/a.rs")
            );
        });
        with_env(None, || {
            assert_eq!(exported_name(file.clone()), file);
            assert_eq!(imported_name(virtual_file.to_path_buf()), virtual_file);
        });
        // A relative base is ignored.
        with_env(Some("rustc-dev/0123=registry"), || {
            assert_eq!(imported_name(virtual_file.to_path_buf()), virtual_file);
        });
        std::fs::remove_dir_all(dir).unwrap();
    }
}
