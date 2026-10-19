//! Sail fork (#248): hash maps and sets for VIR's hot paths, with rustc's
//! deterministic Fx hasher instead of `RandomState` SipHash. Iteration order
//! then depends only on the keys and insertion history, never on a per-process
//! seed; no output may depend on it either way.

pub type HashMap<K, V> = std::collections::HashMap<K, V, rustc_hash::FxBuildHasher>;
pub type HashSet<T> = std::collections::HashSet<T, rustc_hash::FxBuildHasher>;
