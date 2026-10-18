//! Source-level controls for invocation-local pruning indexes.
//! These exercise real typed VIR, without invoking rustc or a solver.

use std::sync::Arc;
use vir::ast::*;
use vir::def::Spanned;
use vir::messages::Span;
use vir::prune::{ModulePruneIndexes, PruneInfo};

fn path(name: &str) -> Path {
    Arc::new(PathX { krate: CrateId::Internal, segments: Arc::new(vec![Arc::new(name.into())]) })
}

fn span(id: u64) -> Span {
    Span { raw_span: Arc::new(id), id, data: vec![17, id], as_string: format!("current:{id}") }
}

fn fixture() -> Krate {
    let module = path("m");
    let other = path("other");
    let trait_path = path("Trait");
    let self_typ =
        Arc::new(TypX::Datatype(Dt::Path(path("D")), Arc::new(vec![]), Arc::new(vec![])));
    let args = Arc::new(vec![self_typ.clone(), self_typ.clone()]);
    let assoc: Ident = Arc::new("Assoc".into());
    let bool_typ = Arc::new(TypX::Bool);
    let bounds = Arc::new(vec![
        Arc::new(GenericBoundX::Trait(TraitId::Path(trait_path.clone()), args.clone())),
        Arc::new(GenericBoundX::TypEquality(
            trait_path.clone(),
            args.clone(),
            assoc.clone(),
            bool_typ.clone(),
        )),
        Arc::new(GenericBoundX::ConstTyp(bool_typ.clone(), bool_typ.clone())),
    ]);
    let public = Visibility { restricted_to: None };
    let datatype = |name: &str, module: Path, bounds: GenericBounds, id| {
        Spanned::new(
            span(id),
            DatatypeX {
                name: Dt::Path(path(name)),
                proxy: None,
                owning_module: Some(module),
                visibility: public.clone(),
                transparency: DatatypeTransparency::WhenVisible(public.clone()),
                typ_params: Arc::new(vec![]),
                typ_bounds: bounds,
                variants: Arc::new(vec![]),
                mode: Mode::Exec,
                ext_equal: false,
                user_defined_invariant_fn: None,
                sized_constraint: None,
                destructor: false,
            },
        )
    };
    let traits = vec![Spanned::new(
        span(3),
        TraitX {
            name: trait_path.clone(),
            proxy: None,
            visibility: public.clone(),
            typ_params: Arc::new(vec![]),
            typ_bounds: Arc::new(vec![]),
            assoc_typs: Arc::new(vec![assoc.clone()]),
            assoc_typs_bounds: Arc::new(vec![]),
            methods: Arc::new(vec![]),
            is_unsafe: false,
            dyn_compatible: None,
            external_trait_extension: None,
        },
    )];
    let trait_impl = |name: &str, id| {
        Spanned::new(
            span(id),
            TraitImplX {
                impl_path: path(name),
                typ_params: Arc::new(vec![]),
                typ_bounds: bounds.clone(),
                trait_path: trait_path.clone(),
                trait_typ_args: args.clone(),
                trait_typ_arg_impls: Spanned::new(span(id + 20), Arc::new(vec![])),
                owning_module: Some(module.clone()),
                auto_imported: false,
                external_trait_blanket: false,
            },
        )
    };
    let assoc_impl = |name: &str, id| {
        Spanned::new(
            span(id),
            AssocTypeImplX {
                name: assoc.clone(),
                impl_path: path(name),
                typ_params: Arc::new(vec![]),
                typ_bounds: Arc::new(vec![]),
                trait_path: trait_path.clone(),
                trait_typ_args: args.clone(),
                typ: bool_typ.clone(),
                impl_paths: Arc::new(vec![]),
            },
        )
    };
    let first_assoc = assoc_impl("impl1", 6);
    Arc::new(KrateX {
        functions: vec![],
        reveal_groups: vec![],
        datatypes: vec![
            datatype("D", module.clone(), bounds.clone(), 1),
            datatype("Extra", other.clone(), Arc::new(vec![]), 2),
        ],
        traits,
        trait_impls: vec![trait_impl("impl1", 4), trait_impl("impl2", 5)],
        // Identical entries and the group member order must survive.
        assoc_type_impls: vec![first_assoc.clone(), assoc_impl("impl2", 7), first_assoc],
        modules: vec![
            Spanned::new(span(8), ModuleX { path: module, reveals: None }),
            Spanned::new(span(9), ModuleX { path: other, reveals: None }),
        ],
        external_fns: vec![],
        external_types: vec![],
        path_as_rust_names: vec![],
        arch: Arch { word_bits: ArchWordBits::Exactly(64) },
        opaque_types: vec![],
    })
}

fn prune(input: &Krate, module: Path, indexes: Option<&ModulePruneIndexes>) -> (Krate, PruneInfo) {
    vir::prune::prune_krate_for_module_or_krate_with_indexes(
        input,
        &CrateId::Internal,
        None,
        Some(module),
        None,
        true,
        true,
        indexes,
    )
}

fn assert_equal(left: (Krate, PruneInfo), right: (Krate, PruneInfo)) {
    // Complete ordinary AST serialization, including spans and source IDs;
    // no projection, field removal, set conversion or normalization.
    assert_eq!(serde_json::to_vec(&left.0).unwrap(), serde_json::to_vec(&right.0).unwrap());
    let (a, b) = (left.1, right.1);
    assert!(a.mono_abstract_datatypes == b.mono_abstract_datatypes);
    assert_eq!(a.spec_fn_types, b.spec_fn_types);
    assert_eq!(a.fndef_types, b.fndef_types);
    assert!(a.resolved_typs == b.resolved_typs);
    assert_eq!(a.dyn_traits, b.dyn_traits);
    assert_eq!(a.used_builtins.uses_array, b.used_builtins.uses_array);
    assert_eq!(a.used_builtins.uses_bytestr, b.used_builtins.uses_bytestr);
    assert_eq!(a.used_builtins.uses_pointee_metadata, b.used_builtins.uses_pointee_metadata);
    assert_eq!(a.used_builtins.uses_ieee_float, b.used_builtins.uses_ieee_float);
}

#[test]
fn shared_indexes_preserve_complete_output_and_ordered_duplicates() {
    let input = fixture();
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    for module in [path("m"), path("other")] {
        assert_equal(prune(&input, module.clone(), None), prune(&input, module, Some(&shared)));
    }
    let (output, _) = prune(&input, path("m"), Some(&shared));
    assert_eq!(
        output.assoc_type_impls.iter().map(|a| a.span.id).collect::<Vec<_>>(),
        vec![6, 7, 6]
    );
    assert_eq!(output.trait_impls.iter().map(|i| i.span.id).collect::<Vec<_>>(), vec![4, 5]);
    assert_eq!(output.datatypes.len(), 1);
    assert!(Arc::ptr_eq(&output.datatypes[0].span.raw_span, &input.datatypes[0].span.raw_span));
    // Sharing is ordinary thread-safe read-only ownership, without unsafe Send.
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ModulePruneIndexes>();
}

#[test]
fn different_owner_refuses_old_indexes_and_keeps_new_reachability() {
    let input = fixture();
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    let mut changed = input.clone();
    let mut imp = changed.trait_impls[0].x.clone();
    imp.typ_bounds = Arc::new(vec![Arc::new(GenericBoundX::ConstTyp(
        Arc::new(TypX::Datatype(Dt::Path(path("Extra")), Arc::new(vec![]), Arc::new(vec![]))),
        Arc::new(TypX::Bool),
    ))]);
    Arc::make_mut(&mut changed).trait_impls[0] = Spanned::new(span(40), imp);
    assert!(!Arc::ptr_eq(&input, &changed));
    assert_equal(prune(&changed, path("m"), None), prune(&changed, path("m"), Some(&shared)));
    // A stale index would retain the old impl and fail to reach Extra.
    assert_eq!(prune(&changed, path("m"), Some(&shared)).0.datatypes.len(), 2);
    assert_eq!(prune(&input, path("m"), Some(&shared)).0.datatypes.len(), 1);
}

#[test]
fn duplicate_impl_refusal_and_crate_mode_last_write_are_preserved() {
    let mut input = fixture();
    let duplicate = input.trait_impls[0].clone();
    Arc::make_mut(&mut input).trait_impls.push(duplicate);
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    for prepared in [None, Some(shared.as_ref())] {
        assert!(std::panic::catch_unwind(|| prune(&input, path("m"), prepared)).is_err());
    }
    let current = input.clone();
    let crate_prune = |prepared| {
        vir::prune::prune_krate_for_module_or_krate_with_indexes(
            &input,
            &CrateId::Internal,
            Some(&current),
            None,
            None,
            false,
            false,
            prepared,
        )
    };
    assert_equal(crate_prune(None), crate_prune(Some(&shared)));
}

#[test]
fn zero_buckets_do_not_construct_or_retain_indexes() {
    let input = fixture();
    let before = Arc::strong_count(&input);
    assert!(ModulePruneIndexes::for_buckets(&input, false).is_none());
    assert_eq!(Arc::strong_count(&input), before);
}
