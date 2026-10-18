//! Current-invocation visitor events, never a persistent frontend artifact.
//!
//! Bucket pruning uses the same function with either its body or no body. The
//! callback arguments and their order depend only on that current immutable AST;
//! reachability effects still execute against each bucket's fresh state/context.
use crate::ast::{Expr, Fun, Function, Krate, Place, Stmt, Typ};
use crate::ast_visitor::VisitorScopeMap;
use air::scope_map::ScopeMap;
use std::collections::HashMap;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicUsize, Ordering},
};

const PER_FUNCTION_EVENTS: usize = 16_384;
const TOTAL_EVENTS: usize = 1_048_576;

pub(super) enum Event {
    Typ(Typ),
    Expr(Expr),
    Place(Place),
}

#[derive(Default)]
struct Variants {
    body: OnceLock<Option<Vec<Event>>>,
    signature: OnceLock<Option<Vec<Event>>>,
}

/// Bounded, lazily collected traversal events for one exact current crate.
///
/// No state, reached set, diagnostic, output, or source projection is retained.
/// An exhausted budget or a different crate always takes the original visitor.
pub struct FunctionTraversalCache {
    krate: Krate,
    functions: HashMap<Fun, Variants>,
    retained_events: AtomicUsize,
}

impl FunctionTraversalCache {
    /// Bind this cache to the immutable crate being bucketed in this invocation.
    pub fn new(krate: &Krate) -> Self {
        Self {
            krate: krate.clone(),
            functions: krate
                .functions
                .iter()
                .map(|f| (f.x.name.clone(), Variants::default()))
                .collect(),
            retained_events: AtomicUsize::new(0),
        }
    }

    pub(super) fn matches(&self, krate: &Krate) -> bool {
        Arc::ptr_eq(&self.krate, krate)
    }

    // Called only by prune.rs after matching this exact crate. Its function_map
    // contains the original Function, or its exact clone with body=None.
    pub(super) fn events(&self, function: &Function) -> Option<&[Event]> {
        let entry = self.functions.get(&function.x.name)?;
        let slot = if function.x.body.is_some() { &entry.body } else { &entry.signature };
        slot.get_or_init(|| {
            let events = collect(function)?;
            self.retained_events
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    old.checked_add(events.len()).filter(|next| *next <= TOTAL_EVENTS)
                })
                .ok()?;
            Some(events)
        })
        .as_deref()
    }
}

struct Collector {
    events: Vec<Event>,
    overflow: bool,
}

impl Collector {
    fn push(&mut self, event: Event) {
        if self.events.len() < PER_FUNCTION_EVENTS {
            self.events.push(event);
        } else {
            self.overflow = true;
        }
    }
}

fn collect(function: &Function) -> Option<Vec<Event>> {
    let mut collector = Collector { events: Vec::new(), overflow: false };
    let fe = |c: &mut Collector, _: &mut VisitorScopeMap, e: &Expr| {
        c.push(Event::Expr(e.clone()));
        Ok(e.clone())
    };
    let fs = |_: &mut Collector, _: &mut VisitorScopeMap, s: &Stmt| Ok(vec![s.clone()]);
    let ft = |c: &mut Collector, t: &Typ| {
        c.push(Event::Typ(t.clone()));
        Ok(t.clone())
    };
    let fp = |c: &mut Collector, _: &mut VisitorScopeMap, p: &Place| {
        c.push(Event::Place(p.clone()));
        Ok(p.clone())
    };
    crate::ast_visitor::map_function_visitor_env(
        function,
        &mut ScopeMap::new(),
        &mut collector,
        &fe,
        &fs,
        &ft,
        &fp,
    )
    .ok()?;
    if collector.overflow { None } else { Some(collector.events) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::def::Spanned;
    use crate::messages::Span;

    fn span(id: u64) -> Span {
        Span { raw_span: Arc::new(id), id, data: vec![id, 91], as_string: format!("current:{id}") }
    }

    fn path(name: &str) -> Path {
        Arc::new(PathX {
            krate: CrateId::Id(Arc::new("test".to_owned()), 7),
            segments: Arc::new(vec![Arc::new(name.to_owned())]),
        })
    }

    fn fixture() -> Krate {
        let typ = Arc::new(TypX::Bool);
        let leaf = |id, value| {
            Arc::new(SpannedTyped {
                span: span(id),
                typ: typ.clone(),
                x: ExprX::Const(Constant::Bool(value)),
            })
        };
        let param = Spanned::new(
            span(2),
            ParamX {
                name: VarIdent(Arc::new("x".to_owned()), VarIdentDisambiguate::VirParam),
                typ: typ.clone(),
                mode: Mode::Proof,
                user_mut: false,
                unwrapped_info: None,
            },
        );
        let place = Arc::new(SpannedTyped {
            span: span(3),
            typ: typ.clone(),
            x: PlaceX::Local(param.x.name.clone()),
        });
        let read = Arc::new(SpannedTyped {
            span: span(4),
            typ: typ.clone(),
            x: ExprX::ReadPlace(
                place,
                UnfinalizedReadKind { preliminary_kind: ReadKind::Spec, id: 4 },
            ),
        });
        let box_typ = Arc::new(TypX::Decorate(TypDecoration::Box, None, typ.clone()));
        let box_param = Spanned::new(
            span(15),
            ParamX {
                name: VarIdent(Arc::new("boxed".to_owned()), VarIdentDisambiguate::VirParam),
                typ: box_typ.clone(),
                mode: Mode::Proof,
                user_mut: false,
                unwrapped_info: None,
            },
        );
        let boxed = Arc::new(SpannedTyped {
            span: span(13),
            typ: box_typ.clone(),
            x: ExprX::Var(box_param.x.name.clone()),
        });
        let helper = Arc::new(FunX { path: path("helper") });
        let call = Arc::new(SpannedTyped {
            span: span(6),
            typ: typ.clone(),
            x: ExprX::Call {
                target: CallTarget::Fun(
                    CallTargetKind::Static,
                    helper,
                    Arc::new(vec![]),
                    Arc::new(vec![]),
                    CallTargetAttrs {
                        autospec: AutospecUsage::Final,
                        const_var: false,
                        assume_external_allowed: false,
                    },
                ),
                args: Arc::new(vec![leaf(12, true), boxed.clone()]),
                post_args: None,
                body: None,
            },
        });
        let resolved = Arc::new(SpannedTyped {
            span: span(7),
            typ: typ.clone(),
            x: ExprX::UnaryOpr(UnaryOpr::HasResolved(box_typ), boxed),
        });
        let body = Arc::new(SpannedTyped {
            span: span(5),
            typ: typ.clone(),
            x: ExprX::If(read, call, Some(resolved)),
        });
        let functions = ["a", "b", "helper"]
            .into_iter()
            .map(|name| {
                Spanned::new(
                    span(8),
                    FunctionX {
                        name: Arc::new(FunX { path: path(name) }),
                        proxy: None,
                        kind: FunctionKind::Static,
                        visibility: Visibility { restricted_to: None },
                        body_visibility: BodyVisibility::Visibility(Visibility {
                            restricted_to: None,
                        }),
                        opaqueness: Opaqueness::Revealed {
                            visibility: Visibility { restricted_to: None },
                        },
                        owning_module: Some(path(name)),
                        mode: Mode::Proof,
                        typ_params: Arc::new(vec![]),
                        typ_bounds: Arc::new(vec![]),
                        params: Arc::new(vec![param.clone(), box_param.clone()]),
                        ret: param.clone(),
                        ens_has_return: true,
                        require: Arc::new(vec![leaf(9, true)]),
                        ensure: (Arc::new(vec![leaf(10, true)]), Arc::new(vec![])),
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
                        body: Some(if name == "helper" { leaf(14, true) } else { body.clone() }),
                        extra_dependencies: vec![],
                        async_ret: None,
                        hidden: Arc::new(vec![]),
                    },
                )
            })
            .collect();
        Arc::new(KrateX {
            functions,
            reveal_groups: vec![],
            datatypes: vec![],
            traits: vec![],
            trait_impls: vec![],
            assoc_type_impls: vec![],
            modules: ["a", "b", "helper"]
                .into_iter()
                .map(|name| Spanned::new(span(11), ModuleX { path: path(name), reveals: None }))
                .collect(),
            external_fns: vec![],
            external_types: vec![],
            path_as_rust_names: vec![],
            arch: Arch { word_bits: ArchWordBits::Exactly(64) },
            opaque_types: vec![],
        })
    }

    fn fingerprints(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                Event::Typ(t) => format!("type:{t:?}"),
                Event::Expr(e) => format!("expr:{e:?}"),
                Event::Place(p) => format!("place:{p:?}"),
            })
            .collect()
    }

    #[test]
    fn current_full_and_bodyless_events_keep_order_values_and_spans() {
        let krate = fixture();
        let cache = FunctionTraversalCache::new(&krate);
        let full = &krate.functions[0];
        let mut bodyless = full.x.clone();
        bodyless.body = None;
        let bodyless = Spanned::new(full.span.clone(), bodyless);
        let full_events = cache.events(full).unwrap();
        let signature_events = cache.events(&bodyless).unwrap();
        assert_eq!(fingerprints(full_events), fingerprints(&collect(full).unwrap()));
        assert_eq!(fingerprints(signature_events), fingerprints(&collect(&bodyless).unwrap()));
        assert!(full_events.len() > signature_events.len());
        assert!(full_events.iter().any(|e| matches!(e, Event::Place(_))));
        for event in full_events {
            if let Event::Expr(e) = event {
                assert_eq!(e.span.data, vec![e.span.id, 91]);
            }
        }
        assert_eq!(
            cache.retained_events.load(Ordering::Relaxed),
            full_events.len() + signature_events.len()
        );
    }

    #[test]
    fn original_pruning_and_cached_pruning_match_in_both_modules() {
        let krate = fixture();
        let cache = Arc::new(FunctionTraversalCache::new(&krate));
        for name in ["a", "b", "a"] {
            let args = (krate.functions[0].x.name.path.krate.clone(), Some(path(name)));
            let (old, old_info) = super::super::prune_krate_for_module_or_krate(
                &krate,
                &args.0,
                None,
                args.1.clone(),
                None,
                true,
                true,
            );
            let (new, new_info) = super::super::prune_krate_with_traversals(
                &krate, &args.0, None, args.1, None, true, true, &cache,
            );
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
            assert_eq!(
                format!("{:?}", old_info.used_builtins),
                format!("{:?}", new_info.used_builtins)
            );
            assert_eq!(old_info.mono_abstract_datatypes, new_info.mono_abstract_datatypes);
            assert_eq!(old_info.spec_fn_types, new_info.spec_fn_types);
            assert_eq!(old_info.fndef_types, new_info.fndef_types);
            assert!(old_info.resolved_typs == new_info.resolved_typs);
            assert_eq!(old_info.dyn_traits, new_info.dyn_traits);
            assert!(matches!(
                new_info.resolved_typs.as_deref(),
                Some([crate::resolve_axioms::ResolvableType::Decoration(TypDecoration::Box)])
            ));
            // The helper belongs to neither selected module: only the real Call
            // event can retain it, with its proof body correctly stripped.
            let helper = new.functions.iter().find(|f| f.x.name.path == path("helper")).unwrap();
            assert!(helper.x.body.is_none());
            assert_eq!(new.functions.len(), 2);
        }
    }

    #[test]
    fn separate_crate_and_exhausted_budget_take_original_path() {
        let krate = fixture();
        let other = Arc::new((*krate).clone());
        let cache = Arc::new(FunctionTraversalCache::new(&krate));
        assert!(!cache.matches(&other));
        cache.retained_events.store(TOTAL_EVENTS, Ordering::Relaxed);
        assert!(cache.events(&krate.functions[0]).is_none());
        assert!(cache.events(&krate.functions[0]).is_none());
        assert_eq!(cache.retained_events.load(Ordering::Relaxed), TOTAL_EVENTS);
        let mut collector = Collector { events: vec![], overflow: false };
        for _ in 0..=PER_FUNCTION_EVENTS {
            collector.push(Event::Typ(Arc::new(TypX::Bool)));
        }
        assert!(collector.overflow);
        assert_eq!(collector.events.len(), PER_FUNCTION_EVENTS);
    }

    #[test]
    fn parallel_buckets_collect_each_variant_once() {
        let krate = fixture();
        let cache = Arc::new(FunctionTraversalCache::new(&krate));
        let mut workers = vec![];
        for _ in 0..8 {
            let cache = cache.clone();
            let function = krate.functions[0].clone();
            workers.push(std::thread::spawn(move || {
                let events = cache.events(&function).unwrap();
                (events.as_ptr() as usize, events.len())
            }));
        }
        let first = workers.remove(0).join().unwrap();
        for worker in workers {
            assert_eq!(worker.join().unwrap(), first);
        }
        assert_eq!(cache.retained_events.load(Ordering::Relaxed), first.1);
    }
}
