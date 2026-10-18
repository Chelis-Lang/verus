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

fn unary_resolved(typ: Typ) -> Expr {
    let arg = Arc::new(SpannedTyped {
        span: span(100),
        typ: typ.clone(),
        x: ExprX::Var(VarIdent(Arc::new("arg".into()), VarIdentDisambiguate::NoBodyParam)),
    });
    Arc::new(SpannedTyped {
        span: span(101),
        typ: Arc::new(TypX::Bool),
        x: ExprX::UnaryOpr(UnaryOpr::HasResolved(typ), arg),
    })
}

fn bool_function(
    name: &str,
    kind: FunctionKind,
    module: Path,
    arg_typ: Typ,
    body: Option<Expr>,
) -> Function {
    let public = Visibility { restricted_to: None };
    let params = Arc::new(vec![Spanned::new(
        span(102),
        ParamX {
            name: VarIdent(Arc::new("arg".into()), VarIdentDisambiguate::NoBodyParam),
            typ: arg_typ,
            mode: Mode::Spec,
            user_mut: false,
            unwrapped_info: None,
        },
    )]);
    let typ_params = if matches!(&kind, FunctionKind::TraitMethodDecl { .. }) {
        Arc::new(vec![Arc::new("Self".into())])
    } else {
        Arc::new(vec![])
    };
    Spanned::new(
        span(103),
        FunctionX {
            name: Arc::new(FunX { path: path(name) }),
            proxy: None,
            kind,
            visibility: public.clone(),
            body_visibility: BodyVisibility::Visibility(public.clone()),
            opaqueness: Opaqueness::Revealed { visibility: public },
            owning_module: Some(module),
            mode: Mode::Spec,
            typ_params,
            typ_bounds: Arc::new(vec![]),
            params,
            ret: Spanned::new(
                span(104),
                ParamX {
                    name: VarIdent(Arc::new("ret".into()), VarIdentDisambiguate::NoBodyParam),
                    typ: Arc::new(TypX::Bool),
                    mode: Mode::Spec,
                    user_mut: false,
                    unwrapped_info: None,
                },
            ),
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
            body,
            extra_dependencies: vec![],
            async_ret: None,
            hidden: Arc::new(vec![]),
        },
    )
}

fn resolved_method_fixture(reverse_impl_order: bool) -> Krate {
    let mut input = fixture();
    let method = Arc::new(FunX { path: path("method") });
    let trait_path = path("Trait");
    let box_typ = Arc::new(TypX::Decorate(TypDecoration::Box, None, Arc::new(TypX::Bool)));
    let tracked_typ = Arc::new(TypX::Decorate(TypDecoration::Tracked, None, Arc::new(TypX::Bool)));
    let decl = bool_function(
        "method",
        FunctionKind::TraitMethodDecl { trait_path: trait_path.clone(), has_default: false },
        path("other"),
        Arc::new(TypX::TypParam(Arc::new("Self".into()))),
        None,
    );
    let call_arg = Arc::new(SpannedTyped {
        span: span(105),
        typ: box_typ.clone(),
        x: ExprX::Var(VarIdent(Arc::new("arg".into()), VarIdentDisambiguate::NoBodyParam)),
    });
    let root = bool_function(
        "root",
        FunctionKind::Static,
        path("m"),
        box_typ.clone(),
        Some(Arc::new(SpannedTyped {
            span: span(106),
            typ: Arc::new(TypX::Bool),
            x: ExprX::Call {
                target: CallTarget::Fun(
                    CallTargetKind::Dynamic,
                    method.clone(),
                    Arc::new(vec![box_typ.clone()]),
                    Arc::new(vec![]),
                    CallTargetAttrs {
                        autospec: AutospecUsage::Final,
                        const_var: false,
                        assume_external_allowed: false,
                    },
                ),
                args: Arc::new(vec![call_arg]),
                post_args: None,
                body: None,
            },
        })),
    );
    let implementation = |name: &str, typ: Typ| {
        bool_function(
            name,
            FunctionKind::TraitMethodImpl {
                method: method.clone(),
                impl_path: path(name),
                trait_path: trait_path.clone(),
                trait_typ_args: Arc::new(vec![typ.clone()]),
                inherit_body_from: None,
            },
            path("other"),
            typ.clone(),
            Some(unary_resolved(typ)),
        )
    };
    let mut implementations = vec![
        implementation("box_impl", box_typ.clone()),
        implementation("tracked_impl", tracked_typ.clone()),
    ];
    if reverse_impl_order {
        implementations.reverse();
    }
    let input = Arc::make_mut(&mut input);
    let mut root_x = root.x.clone();
    root_x.typ_bounds = Arc::new(vec![Arc::new(GenericBoundX::Trait(
        TraitId::Path(trait_path.clone()),
        Arc::new(vec![box_typ.clone()]),
    ))]);
    input.functions = vec![decl, Spanned::new(root.span.clone(), root_x)];
    input.functions.extend(implementations);
    input.datatypes.clear();
    input.assoc_type_impls.clear();
    let mut trait_x = input.traits[0].x.clone();
    trait_x.assoc_typs = Arc::new(vec![]);
    trait_x.methods = Arc::new(vec![method]);
    input.traits[0] = Spanned::new(span(109), trait_x);
    let mut trait_impls = vec![];
    for (name, typ) in [("box_impl", box_typ), ("tracked_impl", tracked_typ)] {
        let mut imp = input.trait_impls[0].x.clone();
        imp.impl_path = path(name);
        imp.typ_bounds = Arc::new(vec![]);
        imp.trait_typ_args = Arc::new(vec![typ]);
        imp.owning_module = Some(path("other"));
        trait_impls.push(Spanned::new(span(110), imp));
    }
    input.trait_impls = trait_impls;
    // No initial root function contains HasResolved. Both implementation bodies
    // are reached through method_map's Bool overapproximation and the actual
    // reached_types/reached_functions worklists. Reversing the ordered impl Vec
    // forces opposite body-discovery order, independently of random hash seeds.
    Arc::new(input.clone())
}

fn builtin_resolve_air(types: Vec<vir::resolve_axioms::ResolvableType>) -> Vec<String> {
    use air::context::SmtSolver;
    use std::collections::HashMap;
    use vir::context::{Ctx, GlobalCtx, WarningCtx};
    let mut empty = fixture();
    let data = Arc::make_mut(&mut empty);
    data.functions.clear();
    data.datatypes.clear();
    data.traits.clear();
    data.trait_impls.clear();
    data.assoc_type_impls.clear();
    // Builtin resolve axioms consult only resolved_typs; use a genuine empty
    // context to isolate the actual production AIR sink without fake commands.
    let global = GlobalCtx::new(
        &empty,
        CrateId::Internal,
        span(107),
        1.0,
        Arc::new(std::sync::Mutex::new(None)),
        Arc::new(std::sync::Mutex::new(None)),
        Arc::new(WarningCtx { fun_warn_configs: HashMap::new() }),
        SmtSolver::Z3,
        true,
        false,
        false,
        false,
        false,
    )
    .unwrap();
    let ctx = Ctx::new(
        &empty,
        global,
        empty.modules[0].clone(),
        vec![],
        vec![],
        Default::default(),
        vir::prune::UsedBuiltins {
            uses_array: false,
            uses_bytestr: false,
            uses_pointee_metadata: false,
            uses_ieee_float: false,
        },
        vec![],
        types,
        false,
    )
    .unwrap();
    let printer = air::printer::Printer::new(
        Arc::new(vir::messages::VirMessageInterface {}),
        false,
        SmtSolver::Z3,
    );
    vir::resolve_axioms::resolve_axioms(&ctx)
        .iter()
        .map(|command| {
            let air::ast::CommandX::Global(decl) = &**command else {
                panic!("unexpected resolve command")
            };
            air::printer::NodeWriter::new()
                .node_to_string_indent(&String::new(), &printer.decl_to_node(decl))
        })
        .collect()
}

#[test]
fn has_resolved_method_discovery_order_preserves_exact_air() {
    use vir::resolve_axioms::ResolvableType;
    let forward = resolved_method_fixture(false);
    let reverse = resolved_method_fixture(true);
    let expected = vec![
        ResolvableType::Decoration(TypDecoration::Box),
        ResolvableType::Decoration(TypDecoration::Tracked),
    ];
    let mut previous = None;
    for input in [&forward, &reverse] {
        let shared = ModulePruneIndexes::for_buckets(input, true).unwrap();
        for indexes in [None, Some(shared.as_ref())] {
            let (_, info) = prune(input, path("m"), indexes);
            let actual = info.resolved_typs.unwrap();
            assert!(actual == expected); // nonempty, exact sequence, no sorting in the test
            let air = builtin_resolve_air(actual);
            assert_eq!(air.len(), 2);
            assert!(air[0].contains("prelude_resolved_box_decoration"));
            assert!(air[1].contains("prelude_resolved_tracked_decoration"));
            if let Some(previous) = &previous {
                assert_eq!(&air, previous);
            }
            previous = Some(air);
        }
    }
}

#[test]
fn resolved_datatypes_follow_current_declarations_after_complete_closure() {
    use vir::resolve_axioms::{ResolvableType, ResolvedTypeCollection};
    let mut input = fixture();
    let extra =
        Arc::new(TypX::Datatype(Dt::Path(path("Extra")), Arc::new(vec![]), Arc::new(vec![])));
    let root = Arc::new(TypX::Datatype(Dt::Path(path("D")), Arc::new(vec![]), Arc::new(vec![])));
    let mut datatype = input.datatypes[0].x.clone();
    datatype.variants = Arc::new(vec![Variant {
        name: Arc::new("D".into()),
        ctor_style: CtorPrintStyle::Braces,
        fields: Arc::new(vec![Arc::new(air::ast::BinderX {
            name: Arc::new("field".into()),
            a: (extra.clone(), Mode::Exec, Visibility { restricted_to: None }),
        })]),
    }]);
    Arc::make_mut(&mut input).datatypes[0] = Spanned::new(span(108), datatype);
    let expected = vec![
        ResolvableType::Datatype(Dt::Path(path("D"))),
        ResolvableType::Datatype(Dt::Path(path("Extra"))),
    ];
    for first in [root.clone(), extra.clone()] {
        let mut collected = ResolvedTypeCollection::new(&path("m"), &input);
        collected.visit_type(&first);
        collected.visit_type(&root);
        collected.visit_type(&root); // existing set dedup is retained
        assert!(collected.finish() == expected); // Extra is also reached through D's field
    }
}

fn datatype_with_field(datatype: &Datatype, typ: Typ, id: u64) -> Datatype {
    let mut value = datatype.x.clone();
    value.variants = Arc::new(vec![Variant {
        name: Arc::new("value".into()),
        ctor_style: CtorPrintStyle::Braces,
        fields: Arc::new(vec![Arc::new(air::ast::BinderX {
            name: Arc::new("field".into()),
            a: (typ, Mode::Exec, Visibility { restricted_to: None }),
        })]),
    }]);
    Spanned::new(span(id), value)
}

fn datatype_typ(name: &str) -> Typ {
    Arc::new(TypX::Datatype(Dt::Path(path(name)), Arc::new(vec![]), Arc::new(vec![])))
}

#[test]
fn shared_resolved_lookup_keeps_last_duplicate_value_and_declaration_ordinal() {
    use vir::resolve_axioms::ResolvableType;
    let mut input = fixture();
    let d = input.datatypes[0].clone();
    let box_typ = Arc::new(TypX::Decorate(TypDecoration::Box, None, Arc::new(TypX::Bool)));
    let tracked_typ = Arc::new(TypX::Decorate(TypDecoration::Tracked, None, Arc::new(TypX::Bool)));
    let data = Arc::make_mut(&mut input);
    data.datatypes[0] = datatype_with_field(&d, box_typ, 120);
    data.datatypes.push(datatype_with_field(&d, tracked_typ, 121));
    data.functions = ["D", "Extra"]
        .iter()
        .map(|name| {
            let typ = datatype_typ(name);
            bool_function(
                name,
                FunctionKind::Static,
                path("m"),
                typ.clone(),
                Some(unary_resolved(typ)),
            )
        })
        .collect();
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    assert_equal(prune(&input, path("m"), None), prune(&input, path("m"), Some(&shared)));
    let (output, info) = prune(&input, path("m"), Some(&shared));
    assert_eq!(output.datatypes.iter().map(|d| d.span.id).collect::<Vec<_>>(), vec![120, 2, 121]);
    // The final D supplies both its fields (Tracked, never Box) and ordinal,
    // placing Extra before D even though D's first declaration was earlier.
    assert!(
        info.resolved_typs.unwrap()
            == vec![
                ResolvableType::Decoration(TypDecoration::Tracked),
                ResolvableType::Datatype(Dt::Path(path("Extra"))),
                ResolvableType::Datatype(Dt::Path(path("D"))),
            ]
    );
}

#[test]
fn shared_resolved_lookup_keeps_module_transparency_and_refuses_changed_owner() {
    use vir::resolve_axioms::ResolvableType;
    let mut input = fixture();
    let d = datatype_with_field(&input.datatypes[0], datatype_typ("Extra"), 122);
    let mut d_x = d.x.clone();
    d_x.transparency =
        DatatypeTransparency::WhenVisible(Visibility { restricted_to: Some(path("m")) });
    let data = Arc::make_mut(&mut input);
    data.datatypes[0] = Spanned::new(d.span.clone(), d_x);
    data.functions = ["m", "other"]
        .iter()
        .map(|module| {
            let typ = datatype_typ("D");
            bool_function(
                module,
                FunctionKind::Static,
                path(module),
                typ.clone(),
                Some(unary_resolved(typ)),
            )
        })
        .collect();
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    for module in ["m", "other"] {
        assert_equal(prune(&input, path(module), None), prune(&input, path(module), Some(&shared)));
    }
    assert!(
        prune(&input, path("m"), Some(&shared)).1.resolved_typs.unwrap()
            == vec![
                ResolvableType::Datatype(Dt::Path(path("D"))),
                ResolvableType::Datatype(Dt::Path(path("Extra"))),
            ]
    );
    assert!(prune(&input, path("other"), Some(&shared)).1.resolved_typs.unwrap().is_empty());
    let mut changed = input.clone();
    let mut changed_d = changed.datatypes[0].x.clone();
    changed_d.transparency = DatatypeTransparency::WhenVisible(Visibility { restricted_to: None });
    Arc::make_mut(&mut changed).datatypes[0] = Spanned::new(span(123), changed_d);
    assert!(!Arc::ptr_eq(&input, &changed));
    assert_equal(
        prune(&changed, path("other"), None),
        prune(&changed, path("other"), Some(&shared)),
    );
    assert_eq!(prune(&changed, path("other"), Some(&shared)).1.resolved_typs.unwrap().len(), 2);
}

#[test]
fn changed_owner_method_lookup_reaches_replacement_and_foreign_implementation() {
    use vir::resolve_axioms::ResolvableType;
    let input = resolved_method_fixture(false);
    let shared = ModulePruneIndexes::for_buckets(&input, true).unwrap();
    let mut changed = input.clone();
    let mut replacement = changed.functions[2].x.clone();
    replacement.name = Arc::new(FunX { path: path("replacement_impl") });
    let mut foreign = changed.functions[3].x.clone();
    let FunctionKind::TraitMethodImpl { method, impl_path, trait_path, trait_typ_args, .. } =
        &foreign.kind
    else {
        panic!("fixture method implementation expected")
    };
    foreign.kind = FunctionKind::ForeignTraitMethodImpl {
        method: method.clone(),
        impl_path: impl_path.clone(),
        trait_path: trait_path.clone(),
        trait_typ_args: trait_typ_args.clone(),
    };
    let data = Arc::make_mut(&mut changed);
    data.functions[2] = Spanned::new(span(124), replacement);
    data.functions[3] = Spanned::new(span(125), foreign);
    assert!(!Arc::ptr_eq(&input, &changed));
    assert_equal(prune(&changed, path("m"), None), prune(&changed, path("m"), Some(&shared)));
    let (output, info) = prune(&changed, path("m"), Some(&shared));
    assert!(output.functions.iter().any(|f| f.x.name.path == path("replacement_impl")));
    assert!(!output.functions.iter().any(|f| f.x.name.path == path("box_impl")));
    assert!(
        info.resolved_typs.unwrap()
            == vec![
                ResolvableType::Decoration(TypDecoration::Box),
                ResolvableType::Decoration(TypDecoration::Tracked),
            ]
    );
}
