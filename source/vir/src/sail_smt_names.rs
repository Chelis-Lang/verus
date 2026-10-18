//! VI-0030: invocation-local function spellings, with original VIR identities intact.

use crate::ast::{CrateId, Dt, ExprX, Function, FunctionKind, ItemKind, Krate, Path, TypX};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(crate) const ALIAS_PREFIX: &str = "%sail%fn%";
const KEY_LIMIT: usize = 512;
const CANDIDATE_LIMIT: usize = 16_384;
const TOTAL_KEY_LIMIT: usize = 8 * 1024 * 1024;

/// An immutable spelling selection for one complete merged proof invocation.
/// It is never exported and never participates in VIR equality or lookup.
#[derive(Default)]
pub struct SmtAliases {
    names: HashMap<Path, Arc<String>>,
}

impl SmtAliases {
    /// Inspect the complete merged VIR, before any crate/module/function prune.
    /// Unsupported inputs and resource limits retain original spellings silently.
    pub fn from_merged_crate(krate: &Krate, allow_inline_air: bool) -> Self {
        if allow_inline_air {
            return Self::default();
        }
        let mut rows = Vec::new();
        let mut total_bytes = 0usize;
        let mut paths = HashSet::new();
        for function in &krate.functions {
            // Raw AIR can mention literal original solver names. Inspect all
            // imported/current headers and bodies, not just a retained bucket.
            if crate::ast_visitor::function_visitor_check(function, &mut |expr| {
                if matches!(&expr.x, ExprX::AirStmt(_)) { Err(()) } else { Ok(()) }
            })
            .is_err()
            {
                return Self::default();
            }
            let path = &function.x.name.path;
            if !paths.insert(path.clone()) || reserved_original(path) {
                return Self::default();
            }
            if let Some(key) = candidate_key(function) {
                total_bytes += key.len();
                if rows.len() == CANDIDATE_LIMIT || total_bytes > TOTAL_KEY_LIMIT {
                    return Self::default();
                }
                rows.push((path.clone(), key));
            }
        }
        Self::select(rows)
    }

    fn select(rows: Vec<(Path, Vec<u8>)>) -> Self {
        let mut counts = HashMap::new();
        for (_, key) in &rows {
            *counts.entry(key.clone()).or_insert(0usize) += 1;
        }
        let mut names = HashMap::new();
        for (path, key) in rows {
            // All duplicates retain legacy names, irrespective of source order.
            if counts.get(&key) == Some(&1) {
                names.insert(path, Arc::new(alias_name(&key)));
            }
        }
        Self { names }
    }

    pub(crate) fn get(&self, path: &Path) -> Option<&str> {
        self.names.get(path).map(|name| name.as_str())
    }
}

fn alias_name(key: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut name = String::with_capacity(ALIAS_PREFIX.len() + 2 * key.len() + 1);
    name.push_str(ALIAS_PREFIX);
    for byte in key {
        name.push(HEX[usize::from(byte >> 4)] as char);
        name.push(HEX[usize::from(byte & 15)] as char);
    }
    name.push('.');
    name
}

// Source crate names cannot start with '%'. Internal generated function names
// must also stay out of this explicitly reserved namespace. No other fixed AIR
// constructor uses this prefix. Preserve legacy behavior if a future internal
// declaration violates that reservation.
fn reserved_original(path: &Path) -> bool {
    matches!(path.krate, CrateId::Internal)
        && path.segments.first().is_some_and(|part| part.starts_with(ALIAS_PREFIX))
}

fn candidate_key(function: &Function) -> Option<Vec<u8>> {
    let (method, impl_path, trait_path, args) = match &function.x.kind {
        FunctionKind::TraitMethodImpl { method, impl_path, trait_path, trait_typ_args, .. }
        | FunctionKind::ForeignTraitMethodImpl { method, impl_path, trait_path, trait_typ_args } => {
            (method, impl_path, trait_path, trait_typ_args)
        }
        _ => return None,
    };
    if args.len() != 1 {
        return None;
    }
    let TypX::Datatype(Dt::Path(self_path), self_args, _) = &*args[0] else { return None };
    if !self_args.is_empty() {
        return None;
    }
    let module = function.x.owning_module.as_ref()?;
    let member = method.path.segments.last()?;
    // Only a direct actual impl member, not a synthesized default/helper path.
    if function.x.name.path.krate != module.krate
        || impl_path.krate != module.krate
        || impl_path.segments.len() != module.segments.len() + 1
        || !impl_path.segments.starts_with(&module.segments)
        || function.x.name.path.segments.len() != impl_path.segments.len() + 1
        || !function.x.name.path.segments.starts_with(&impl_path.segments)
        || function.x.name.path.segments.last()? != member
    {
        return None;
    }
    let index = impl_path.segments.last()?.strip_prefix("impl&%")?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut key = Key::new();
    key.field(b"sail-function-alias-03")?;
    key.path(module)?;
    key.path(self_path)?;
    key.path(trait_path)?;
    key.field(match function.x.item_kind {
        ItemKind::Function => b"fn",
        ItemKind::Const => b"const",
        ItemKind::Static => b"static",
    })?;
    key.field(member.as_bytes())?;
    Some(key.bytes)
}

struct Key {
    bytes: Vec<u8>,
}
impl Key {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    fn field(&mut self, bytes: &[u8]) -> Option<()> {
        let len = u32::try_from(bytes.len()).ok()?;
        if self.bytes.len().checked_add(4)?.checked_add(bytes.len())? > KEY_LIMIT {
            return None;
        }
        self.bytes.extend(len.to_be_bytes());
        self.bytes.extend(bytes);
        Some(())
    }
    fn path(&mut self, path: &Path) -> Option<()> {
        match &path.krate {
            CrateId::Internal => return None,
            CrateId::Core => self.field(b"core")?,
            CrateId::Alloc => self.field(b"alloc")?,
            CrateId::Vstd => self.field(b"vstd")?,
            CrateId::Id(name, id) => {
                self.field(b"id")?;
                self.field(name.as_bytes())?;
                self.field(&id.to_be_bytes())?;
            }
        }
        self.field(&u32::try_from(path.segments.len()).ok()?.to_be_bytes())?;
        for part in path.segments.iter() {
            // Ordinary typed module/type/trait names only. Anonymous synthetic
            // components stay legacy; their counters never enter a key.
            if part.is_empty() || !part.chars().all(|c| c == '_' || c.is_alphanumeric()) {
                return None;
            }
            self.field(part.as_bytes())?;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::def::{NameCtxt, Spanned};

    fn path(crate_id: u64, parts: &[&str]) -> Path {
        Arc::new(PathX {
            krate: CrateId::Id(Arc::new("same".to_string()), crate_id),
            segments: Arc::new(parts.iter().map(|s| Arc::new(s.to_string())).collect()),
        })
    }
    fn span() -> crate::messages::Span {
        crate::messages::Span {
            raw_span: Arc::new(()),
            id: 0,
            data: vec![],
            as_string: String::new(),
        }
    }
    fn function(index: u32, member: &str) -> Function {
        let module = path(11, &["m"]);
        let impl_path = module.push_segment(Arc::new(format!("impl&%{index}")));
        let trait_path = path(11, &["T"]);
        let self_type = Arc::new(TypX::Datatype(
            Dt::Path(path(11, &["S"])),
            Arc::new(vec![]),
            Arc::new(vec![]),
        ));
        let result = Spanned::new(
            span(),
            ParamX {
                name: VarIdent(Arc::new("r".to_string()), VarIdentDisambiguate::NoBodyParam),
                typ: Arc::new(TypX::Bool),
                mode: Mode::Exec,
                user_mut: false,
                unwrapped_info: None,
            },
        );
        Spanned::new(
            span(),
            FunctionX {
                name: Arc::new(FunX { path: impl_path.push_segment(Arc::new(member.to_string())) }),
                proxy: None,
                kind: FunctionKind::TraitMethodImpl {
                    method: Arc::new(FunX {
                        path: trait_path.push_segment(Arc::new(member.to_string())),
                    }),
                    impl_path,
                    trait_path,
                    trait_typ_args: Arc::new(vec![self_type]),
                    inherit_body_from: None,
                },
                visibility: Visibility { restricted_to: None },
                body_visibility: BodyVisibility::Uninterpreted,
                opaqueness: Opaqueness::Opaque,
                owning_module: Some(module),
                mode: Mode::Exec,
                typ_params: Arc::new(vec![]),
                typ_bounds: Arc::new(vec![]),
                params: Arc::new(vec![]),
                ret: result,
                ens_has_return: true,
                require: Arc::new(vec![]),
                ensure: (Arc::new(vec![]), Arc::new(vec![])),
                returns: None,
                decrease: Arc::new(vec![]),
                decrease_when: None,
                decrease_by: None,
                fndef_axioms: None,
                mask_spec: None,
                atomic_update: None,
                unwind_spec: None,
                item_kind: ItemKind::Function,
                attrs: Arc::new(FunctionAttrsX::default()),
                body: None,
                extra_dependencies: vec![],
                async_ret: None,
                hidden: Arc::new(vec![]),
            },
        )
    }
    fn krate(functions: Vec<Function>) -> Krate {
        Arc::new(KrateX {
            functions,
            reveal_groups: vec![],
            datatypes: vec![],
            traits: vec![],
            trait_impls: vec![],
            assoc_type_impls: vec![],
            modules: vec![],
            external_fns: vec![],
            external_types: vec![],
            path_as_rust_names: vec![],
            opaque_types: vec![],
            arch: Arch { word_bits: ArchWordBits::Either32Or64 },
        })
    }

    #[test]
    fn complete_set_duplicates_all_stay_legacy_even_if_later_pruned() {
        let first = function(1, "clone");
        let second = function(2, "clone");
        let other = function(3, "other");
        let source = krate(vec![first.clone(), second.clone(), other.clone()]);
        let aliases = SmtAliases::from_merged_crate(&source, false);
        let reversed = SmtAliases::from_merged_crate(
            &krate(vec![other.clone(), second, first.clone()]),
            false,
        );
        assert_eq!(aliases.names, reversed.names);
        assert!(aliases.get(&first.x.name.path).is_none());
        assert!(aliases.get(&other.x.name.path).is_some());
        // The full-set result is retained when a bucket contains only first.
        // Reclassifying that pruned set would incorrectly select a new spelling.
        assert!(
            SmtAliases::from_merged_crate(&krate(vec![first.clone()]), false)
                .get(&first.x.name.path)
                .is_some()
        );
        let ctx = NameCtxt::new(Arc::new(aliases));
        assert_eq!(ctx.fun_to_string(&first.x.name), ctx.path_to_string(&first.x.name.path));
    }

    #[test]
    fn unsupported_keys_namespace_conflicts_and_total_caps_never_partially_win() {
        let eligible = function(1, "clone");
        let mut unsupported = function(2, "generic").x.clone();
        unsupported.kind = FunctionKind::Static;
        let unsupported = Spanned::new(span(), unsupported);
        let aliases = SmtAliases::from_merged_crate(
            &krate(vec![eligible.clone(), unsupported.clone()]),
            false,
        );
        assert!(aliases.get(&eligible.x.name.path).is_some());
        assert!(aliases.get(&unsupported.x.name.path).is_none());
        let mut reserved = unsupported.x.clone();
        reserved.name = Arc::new(FunX {
            path: Arc::new(PathX {
                krate: CrateId::Internal,
                segments: Arc::new(vec![Arc::new(format!("{ALIAS_PREFIX}bad"))]),
            }),
        });
        assert!(
            SmtAliases::from_merged_crate(
                &krate(vec![eligible, Spanned::new(span(), reserved)]),
                false
            )
            .names
            .is_empty()
        );
        let many =
            (0..=CANDIDATE_LIMIT).map(|i| function(i as u32, &format!("member{i}"))).collect();
        assert!(SmtAliases::from_merged_crate(&krate(many), false).names.is_empty());
    }

    #[test]
    fn complete_fields_are_injective_bounded_and_not_affected_by_impl_number() {
        let a = function(1, "clone");
        let b = function(900, "clone");
        assert_eq!(candidate_key(&a), candidate_key(&b));
        let mut key = Key::new();
        key.field(b"ab").unwrap();
        key.field(b"c").unwrap();
        let mut other = Key::new();
        other.field(b"a").unwrap();
        other.field(b"bc").unwrap();
        assert_ne!(key.bytes, other.bytes);
        let before = key.bytes.clone();
        assert!(key.field(&[0; KEY_LIMIT]).is_none());
        assert_eq!(key.bytes, before);
        assert!(candidate_key(&function(1, &"x".repeat(KEY_LIMIT))).is_none());
        assert_ne!(alias_name(&[0, 1]), alias_name(&[0, 1, 0]));
    }

    #[test]
    fn inline_air_anywhere_disables_the_whole_invocation_and_flag_also_does() {
        let eligible = function(1, "clone");
        let mut raw = function(2, "unretained").x.clone();
        raw.kind = FunctionKind::Static;
        raw.body = Some(SpannedTyped::new(
            &span(),
            &Arc::new(TypX::Bool),
            ExprX::AirStmt(Arc::new("(assume true)".to_string())),
        ));
        let source = krate(vec![eligible.clone(), Spanned::new(span(), raw)]);
        assert!(SmtAliases::from_merged_crate(&source, false).names.is_empty());
        assert!(SmtAliases::from_merged_crate(&krate(vec![eligible]), true).names.is_empty());
    }

    #[test]
    fn rendering_preserves_original_paths_crate_registration_and_all_function_prefixes() {
        let f = function(1, "clone");
        let original_path = f.x.name.path.clone();
        let aliases = Arc::new(SmtAliases::from_merged_crate(&krate(vec![f.clone()]), false));
        let selected = aliases.get(&original_path).unwrap().to_owned();
        let plain = NameCtxt::new(Arc::new(SmtAliases::default()));
        let renamed = NameCtxt::new(aliases.clone());
        assert_eq!(renamed.path_to_string(&original_path), plain.path_to_string(&original_path));
        assert_eq!(renamed.fun_to_string(&f.x.name), selected);
        assert_eq!(f.x.name.path, original_path);
        let foreign = Arc::new(FunX { path: path(12, &["foreign"]) });
        let plain_fresh = NameCtxt::new(Arc::new(SmtAliases::default()));
        let aliased_fresh = NameCtxt::new(aliases.clone());
        plain_fresh.fun_to_string(&f.x.name);
        aliased_fresh.fun_to_string(&f.x.name);
        assert_eq!(aliased_fresh.fun_to_string(&foreign), plain_fresh.fun_to_string(&foreign));
        assert!(aliased_fresh.fun_to_string(&foreign).starts_with("same!!2!"));
        assert_eq!(renamed.prefix_fndef_type_id(&f.x.name).as_str(), format!("FNDEF%{selected}"));
        let stem = Arc::new(renamed.fun_to_string(&f.x.name));
        assert_eq!(crate::def::prefix_requires(&stem).as_str(), format!("req%{selected}"));
        assert_eq!(crate::def::prefix_ensures(&stem).as_str(), format!("ens%{selected}"));
        assert_eq!(crate::def::prefix_fuel_id(&stem).as_str(), format!("fuel%{selected}"));
        let another_bucket = NameCtxt::new(aliases);
        assert_eq!(another_bucket.fun_to_string(&f.x.name), selected);
        // Synthetic recursive names are distinct original identities and stay
        // legacy on both declaration/reference routes in this first screen.
        let recursive = crate::def::prefix_recursive_fun(&f.x.name);
        assert_eq!(renamed.fun_to_string(&recursive), renamed.path_to_string(&recursive.path));
    }
}
