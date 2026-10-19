//! Sail: span strings of a prebuilt crate's VIR as a local build records them.
//!
//! Verus records each span's text (`Span::as_string`) in the VIR it exports.
//! A standard library span reads `/rustc/<rustc commit>/library/...` when the
//! exporting toolchain has no `rust-src` component, and
//! `<sysroot>/lib/rustlib/src/rust/library/...` when it has one, because rustc
//! then translates the virtual path to the local copy. Verus prints the stored
//! text where it cannot resolve an imported span. Sail ships vstd prebuilt
//! with the virtual names (Sail VI-0031); for a user whose toolchain has
//! `rust-src`, a vstd compiled on that machine would hold the local names.
//!
//! The same holds for the prebuilt crates' own registry sources, which Sail
//! records under rustc's virtual rustc-dev base (Sail ADR 0048) where a local
//! build names the user's registry directory.
//!
//! `SAIL_VIR_SPAN_REMAP` holds one `<from>=<to>` pair per line. The import
//! rewrites every span string of imported VIR that starts with a pair's
//! `<from>` to start with its `<to>` instead; pairs are tried in order and the
//! first match wins. Sail sets the pairs only when it serves prebuilt VIR:
//! `/rustc/<rustc commit>/=<sysroot>/lib/rustlib/src/rust/` (rustc's own
//! translation) when the toolchain has `rust-src`, and the rustc-dev base of
//! the registry sources to the user's registry directory. Each prefix must be
//! an absolute path; a malformed line selects nothing at all. Without the
//! variable nothing changes.

/// The variable naming the recorded prefixes and their replacements, one
/// `<from>=<to>` pair per line.
pub const ENV: &str = "SAIL_VIR_SPAN_REMAP";

/// The selected pairs, in order: each split at its first `=`, both prefixes
/// absolute paths. Any malformed line, or a non-UTF-8 value, selects none.
pub(crate) fn remap() -> Vec<(String, String)> {
    let Some(value) = std::env::var_os(ENV) else {
        return Vec::new();
    };
    let Some(value) = value.to_str() else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    for line in value.lines() {
        match line.split_once('=') {
            Some((from, to)) if from.starts_with('/') && to.starts_with('/') => {
                pairs.push((from.to_owned(), to.to_owned()))
            }
            _ => return Vec::new(),
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn imported(span: &str) -> String {
        let original = vir::messages::Span {
            raw_span: std::sync::Arc::new(()),
            id: 0,
            data: vec![1, 2],
            as_string: span.to_owned(),
        };
        let config = bincode_next::config::legacy();
        let bytes = bincode_next::serde::encode_to_vec(&original, config).unwrap();
        let decoded: vir::messages::Span = vir::messages::with_imported_span_remap(remap(), || {
            bincode_next::serde::decode_from_slice(&bytes, config).unwrap().0
        });
        assert_eq!(decoded.data, vec![1, 2]);
        decoded.as_string
    }

    #[test]
    fn imported_span_strings_change_prefix_only_when_selected() {
        let std_span = "/rustc/0123/library/core/src/marker.rs:1025:10: 1025:17 (#0)";
        let local = "/sysroot/lib/rustlib/src/rust/library/core/src/marker.rs:1025:10: 1025:17 (#0)";
        let other = "/home/user/src/lib.rs:3:1: 3:9 (#0)";
        with_env(Some("/rustc/0123/=/sysroot/lib/rustlib/src/rust/"), || {
            assert_eq!(imported(std_span), local);
            assert_eq!(imported(other), other);
        });
        with_env(None, || assert_eq!(imported(std_span), std_span));
        // Several pairs: each applies to its own prefix, the first match wins.
        let registry = "/rustc-dev/0123/vstd-1/src/seq.rs:5:1: 5:9 (#0)";
        let pairs = "/rustc/0123/=/sysroot/lib/rustlib/src/rust/\n\
                     /rustc-dev/0123/=/home/user/.cargo/registry/src/index/\n\
                     /rustc/=/never/";
        with_env(Some(pairs), || {
            assert_eq!(imported(std_span), local);
            assert_eq!(imported(registry), "/home/user/.cargo/registry/src/index/vstd-1/src/seq.rs:5:1: 5:9 (#0)");
            assert_eq!(imported(other), other);
        });
        // Malformed or relative values select nothing.
        with_env(Some("rustc/0123/=/sysroot/"), || assert_eq!(imported(std_span), std_span));
        with_env(Some("/rustc/0123/"), || assert_eq!(imported(std_span), std_span));
        with_env(Some("/rustc/0123/=/sysroot/\nrelative=/x/"), || {
            assert_eq!(imported(std_span), std_span)
        });
    }
}
