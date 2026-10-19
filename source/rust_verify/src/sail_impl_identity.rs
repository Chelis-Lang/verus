//! VI-0030: concrete trait-implementation identity, independent of erased members.
//!
//! Inherent impls retain legacy names. Positive implementations of a ground
//! nominal trait reference are unique after coherence; no member/peer inventory
//! or source/body/span/index enters their key.

use rustc_hir::definitions::DefPathData;
use rustc_middle::ty::{self, TyCtxt, TyKind};
use rustc_span::def_id::DefId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use vir::ast::Ident;

const IMPL_KEY_LIMIT: usize = 512;
const CLOSURE_KEY_LIMIT: usize = 1024;

#[derive(Clone)]
pub(crate) struct StableImpl {
    key: Arc<Vec<u8>>,
    pub(crate) ident: Ident,
}

/// One compiler's cache, shared with all path and closure lowering.
/// Only enabled after the custom HIR check has completed coherence successfully.
#[derive(Default)]
pub(crate) struct ImplIdentities {
    ready: AtomicBool,
    cache: Mutex<HashMap<DefId, Option<StableImpl>>>,
}

impl ImplIdentities {
    /// Extra semantic queries become legal only after the custom HIR check.
    pub(crate) fn enable_after_hir_check(&self) {
        self.ready.store(true, Ordering::Release);
    }

    pub(crate) fn get(&self, tcx: TyCtxt<'_>, id: DefId) -> Option<StableImpl> {
        if !self.ready.load(Ordering::Acquire) {
            return None;
        }
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(name) = cache.get(&id) {
                return name.clone();
            }
        }
        let name = simple_trait_key(tcx, id).map(|key| StableImpl {
            ident: Arc::new(format!("impl&%s2{}", hex::encode(&key))),
            key: Arc::new(key),
        });
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).insert(id, name.clone());
        name
    }

    /// Replace only the impl parent; retain the accepted relative DefKey suffix.
    /// A closure-local insertion is outside VI-0030's stability promise.
    pub(crate) fn closure_key(&self, tcx: TyCtxt<'_>, id: DefId) -> Option<Ident> {
        let mut relative = Vec::new();
        let mut current = id;
        loop {
            let key = tcx.def_key(current);
            if matches!(key.disambiguated_data.data, DefPathData::Impl) {
                let parent = self.get(tcx, current)?;
                let mut encoded = Key::new(CLOSURE_KEY_LIMIT, b"c1");
                encoded.field(&parent.key)?;
                relative.reverse();
                for data in relative {
                    encode_relative(&mut encoded, data)?;
                }
                return Some(Arc::new(hex::encode(encoded.bytes)));
            }
            relative.push(key.disambiguated_data);
            current = tcx.opt_parent(current)?;
        }
    }
}

/// Encode only the semantic ground trait reference and its containing module.
///
/// Associated members, impl generics and predicates are deliberately not read:
/// ghost erasure can remove private/restricted associated items and empty whole
/// blocks. Coherence forbids distinct positive implementations of this same
/// ground trait reference. Unused impl lifetimes and already-satisfied ground
/// predicates do not distinguish its implementation identity either.
fn simple_trait_key(tcx: TyCtxt<'_>, id: DefId) -> Option<Vec<u8>> {
    let trait_id = tcx.impl_opt_trait_id(id)?; // Every inherent impl stays legacy.
    // Marker traits explicitly permit overlapping positive implementations.
    if tcx.trait_def(trait_id).is_marker {
        return None;
    }
    let header = tcx.impl_trait_header(id);
    if header.polarity != ty::ImplPolarity::Positive {
        return None;
    }
    let self_ty = tcx.type_of(id).instantiate_identity();
    let TyKind::Adt(adt, args) = self_ty.kind() else { return None };
    if !args.is_empty() {
        return None;
    }
    let tr = header.trait_ref.instantiate_identity().skip_norm_wip();
    if tr.args.len() != 1 || tr.self_ty() != self_ty {
        return None;
    }
    // Safety, constness and defaultness do not select a second implementation
    // of the same ground trait reference. They are not eligibility switches:
    // changing them under cfg must not make one side silently choose legacy.
    let mut key = Key::new(IMPL_KEY_LIMIT, b"i2");
    key.field(&tcx.stable_crate_id(id.krate).as_u64().to_be_bytes())?;
    key.field(&named_path(tcx, tcx.parent(id), true)?)?;
    key.field(&named_path(tcx, adt.did(), false)?)?;
    key.field(b"trait")?;
    key.field(&named_path(tcx, trait_id, false)?)?;
    Some(key.bytes)
}

/// Module-qualified type/trait names only, including their full crate identity.
fn named_path(tcx: TyCtxt<'_>, id: DefId, module: bool) -> Option<Vec<u8>> {
    let mut current = id;
    if module && !matches!(tcx.def_kind(id), rustc_hir::def::DefKind::Mod) {
        return None;
    }
    while let Some(parent) = tcx.opt_parent(current) {
        if !matches!(tcx.def_kind(parent), rustc_hir::def::DefKind::Mod) {
            return None;
        }
        current = parent;
    }
    let path = tcx.def_path(id);
    let mut key = Key::new(IMPL_KEY_LIMIT, b"path");
    key.field(&tcx.stable_crate_id(id.krate).as_u64().to_be_bytes())?;
    for data in path.data {
        let DefPathData::TypeNs(name) = data.data else { return None };
        if data.disambiguator != 0 {
            return None;
        }
        key.field(name.as_str().as_bytes())?;
    }
    Some(key.bytes)
}

fn encode_relative(
    key: &mut Key,
    data: rustc_hir::definitions::DisambiguatedDefPathData,
) -> Option<()> {
    let tag: &[u8] = match data.data {
        DefPathData::ValueNs(name) => {
            key.field(b"value")?;
            key.field(name.as_str().as_bytes())?;
            b"named"
        }
        DefPathData::TypeNs(name) => {
            key.field(b"type")?;
            key.field(name.as_str().as_bytes())?;
            b"named"
        }
        DefPathData::Closure => b"closure",
        DefPathData::OpaqueTy => b"opaque",
        _ => return None,
    };
    key.field(tag)?;
    key.field(&data.disambiguator.to_be_bytes())
}

struct Key {
    bytes: Vec<u8>,
    limit: usize,
}
impl Key {
    fn new(limit: usize, domain: &[u8]) -> Self {
        Self { bytes: domain.to_vec(), limit }
    }
    fn field(&mut self, bytes: &[u8]) -> Option<()> {
        let length = u32::try_from(bytes.len()).ok()?;
        if self.bytes.len().checked_add(4)?.checked_add(bytes.len())? > self.limit {
            return None;
        }
        self.bytes.extend(length.to_be_bytes());
        self.bytes.extend(bytes);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::Key;
    #[test]
    fn complete_fields_do_not_alias_and_size_limit_never_truncates() {
        let mut a = Key::new(64, b"i1");
        a.field(b"ab").unwrap();
        a.field(b"c").unwrap();
        let mut b = Key::new(64, b"i1");
        b.field(b"a").unwrap();
        b.field(b"bc").unwrap();
        assert_ne!(a.bytes, b.bytes);
        let before = a.bytes.clone();
        assert!(a.field(&[0; 64]).is_none());
        assert_eq!(a.bytes, before);
    }
    #[test]
    fn exported_closure_identity_round_trips_complete_stable_parent() {
        let a = vir::ast::ClosureId {
            name: 0,
            stable_crate_id: 91,
            order: 12,
            stable_impl_key: Some(std::sync::Arc::new("01020300ff".to_string())),
        };
        let encoded = serde_json::to_vec(&a).unwrap();
        let imported: vir::ast::ClosureId = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(a, imported);
        assert_eq!(a.solver_name(), imported.solver_name());
        assert_eq!(a.stable_impl_key, imported.stable_impl_key);
    }
}
