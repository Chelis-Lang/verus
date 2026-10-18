//! The old mapper remains the literal reference. No rustc, solver or allocator hook.

use super::*;
use crate::ast::*;
use crate::ast_visitor::Walk;
use crate::def::Spanned;
use crate::messages::Span;
use std::cell::RefCell;
use std::collections::HashSet;

// Capture every ordinary serde field, variant, scalar and ordered element.
// This avoids a new test dependency and does not use VIR's lossy Debug printer.
// Opaque raw_span is deliberately not serializable; its Arc is checked below.
mod tokens {
    use serde::ser::{self, Serialize};
    use std::fmt;

    #[derive(Debug)]
    pub struct Error(String);
    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for Error {}
    impl ser::Error for Error {
        fn custom<T: fmt::Display>(message: T) -> Self {
            Self(message.to_string())
        }
    }
    #[derive(Default)]
    struct Capture(Vec<String>);
    impl Capture {
        fn event(&mut self, kind: &str, value: impl fmt::Debug) {
            self.0.push(format!("{kind}:{value:?}"));
        }
    }
    macro_rules! scalar {
        ($($method:ident($ty:ty)),* $(,)?) => {$(
            fn $method(self, value: $ty) -> Result<(), Error> {
                self.event(stringify!($method), value); Ok(())
            }
        )*};
    }
    impl<'a> ser::Serializer for &'a mut Capture {
        type Ok = ();
        type Error = Error;
        type SerializeSeq = Self;
        type SerializeTuple = Self;
        type SerializeTupleStruct = Self;
        type SerializeTupleVariant = Self;
        type SerializeMap = Self;
        type SerializeStruct = Self;
        type SerializeStructVariant = Self;
        scalar!(
            serialize_bool(bool),
            serialize_i8(i8),
            serialize_i16(i16),
            serialize_i32(i32),
            serialize_i64(i64),
            serialize_i128(i128),
            serialize_u8(u8),
            serialize_u16(u16),
            serialize_u32(u32),
            serialize_u64(u64),
            serialize_u128(u128),
            serialize_f32(f32),
            serialize_f64(f64),
            serialize_char(char),
            serialize_str(&str),
            serialize_bytes(&[u8])
        );
        fn serialize_none(self) -> Result<(), Error> {
            self.event("none", ());
            Ok(())
        }
        fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
            self.event("some", ());
            value.serialize(self)
        }
        fn serialize_unit(self) -> Result<(), Error> {
            self.event("unit", ());
            Ok(())
        }
        fn serialize_unit_struct(self, name: &'static str) -> Result<(), Error> {
            self.event("unit-struct", name);
            Ok(())
        }
        fn serialize_unit_variant(
            self,
            name: &'static str,
            index: u32,
            variant: &'static str,
        ) -> Result<(), Error> {
            self.event("unit-variant", (name, index, variant));
            Ok(())
        }
        fn serialize_newtype_struct<T: Serialize + ?Sized>(
            self,
            name: &'static str,
            value: &T,
        ) -> Result<(), Error> {
            self.event("newtype", name);
            value.serialize(self)
        }
        fn serialize_newtype_variant<T: Serialize + ?Sized>(
            self,
            name: &'static str,
            index: u32,
            variant: &'static str,
            value: &T,
        ) -> Result<(), Error> {
            self.event("newtype-variant", (name, index, variant));
            value.serialize(self)
        }
        fn serialize_seq(self, len: Option<usize>) -> Result<Self, Error> {
            self.event("seq", len);
            Ok(self)
        }
        fn serialize_tuple(self, len: usize) -> Result<Self, Error> {
            self.event("tuple", len);
            Ok(self)
        }
        fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self, Error> {
            self.event("tuple-struct", (name, len));
            Ok(self)
        }
        fn serialize_tuple_variant(
            self,
            name: &'static str,
            index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, Error> {
            self.event("tuple-variant", (name, index, variant, len));
            Ok(self)
        }
        fn serialize_map(self, len: Option<usize>) -> Result<Self, Error> {
            self.event("map", len);
            Ok(self)
        }
        fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self, Error> {
            self.event("struct", (name, len));
            Ok(self)
        }
        fn serialize_struct_variant(
            self,
            name: &'static str,
            index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, Error> {
            self.event("struct-variant", (name, index, variant, len));
            Ok(self)
        }
        fn is_human_readable(&self) -> bool {
            false
        }
    }
    macro_rules! sequence {
        ($trait:ident, $method:ident) => {
            impl ser::$trait for &mut Capture {
                type Ok = ();
                type Error = Error;
                fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
                    value.serialize(&mut **self)
                }
                fn end(self) -> Result<(), Error> {
                    self.event("end", ());
                    Ok(())
                }
            }
        };
    }
    sequence!(SerializeSeq, serialize_element);
    sequence!(SerializeTuple, serialize_element);
    sequence!(SerializeTupleStruct, serialize_field);
    sequence!(SerializeTupleVariant, serialize_field);
    impl ser::SerializeMap for &mut Capture {
        type Ok = ();
        type Error = Error;
        fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
            self.event("key", ());
            value.serialize(&mut **self)
        }
        fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
            self.event("value", ());
            value.serialize(&mut **self)
        }
        fn end(self) -> Result<(), Error> {
            self.event("end", ());
            Ok(())
        }
    }
    macro_rules! structure {
        ($trait:ident) => {
            impl ser::$trait for &mut Capture {
                type Ok = ();
                type Error = Error;
                fn serialize_field<T: Serialize + ?Sized>(
                    &mut self,
                    key: &'static str,
                    value: &T,
                ) -> Result<(), Error> {
                    self.event("field", key);
                    value.serialize(&mut **self)
                }
                fn end(self) -> Result<(), Error> {
                    self.event("end", ());
                    Ok(())
                }
            }
        };
    }
    structure!(SerializeStruct);
    structure!(SerializeStructVariant);
    pub fn capture(value: &impl Serialize) -> Vec<String> {
        let mut capture = Capture::default();
        value.serialize(&mut capture).unwrap();
        capture.0
    }
}

fn path(name: &str) -> Path {
    Arc::new(PathX {
        krate: CrateId::Id(Arc::new("current".into()), 23),
        segments: Arc::new(name.split("::").map(|s| Arc::new(s.into())).collect()),
    })
}
fn fun(name: &str) -> Fun {
    Arc::new(FunX { path: path(name) })
}
fn var(name: &str) -> VarIdent {
    VarIdent(Arc::new(name.into()), VarIdentDisambiguate::RustcId(71))
}
fn span(id: u64) -> Span {
    Span { raw_span: Arc::new(id), id, data: vec![23, id * 17], as_string: format!("current:{id}") }
}
fn expr(id: u64, typ: &Typ, x: ExprX) -> Expr {
    SpannedTyped::new(&span(id), typ, x)
}
fn constant(id: u64, typ: &Typ) -> Expr {
    expr(id, typ, ExprX::ConstVar(fun("const_exec"), AutospecUsage::IfMarked))
}

fn types() -> Vec<Typ> {
    let b = Arc::new(TypX::Bool);
    let args = Arc::new(vec![b.clone(), Arc::new(TypX::TypParam(Arc::new("T".into())))]);
    let imps =
        Arc::new(vec![ImplPath::TraitImplPath(path("impl")), ImplPath::FnDefImplPath(fun("exec"))]);
    vec![
        b.clone(),
        Arc::new(TypX::Int(IntRange::Int)),
        Arc::new(TypX::Real),
        Arc::new(TypX::Float(64)),
        args[1].clone(),
        Arc::new(TypX::TypeId),
        Arc::new(TypX::ConstInt(17.into())),
        Arc::new(TypX::ConstBool(true)),
        Arc::new(TypX::Air(Arc::new(air::ast::TypX::Named(Arc::new("opaque-air".into()))))),
        Arc::new(TypX::SpecFn(args.clone(), b.clone())),
        Arc::new(TypX::AnonymousClosure(
            args.clone(),
            b.clone(),
            ClosureKind::FnOnce,
            ClosureId { name: 31, stable_crate_id: 23, order: 7 },
        )),
        Arc::new(TypX::FnDef(fun("exec"), args.clone(), Some(fun("spec")))),
        Arc::new(TypX::Datatype(Dt::Path(path("D")), args.clone(), imps.clone())),
        Arc::new(TypX::Dyn(path("Trait"), args.clone(), imps)),
        Arc::new(TypX::Opaque { def_path: path("opaque"), args: args.clone() }),
        Arc::new(TypX::Primitive(Primitive::Array, args.clone())),
        Arc::new(TypX::Decorate(
            TypDecoration::Box,
            Some(TypDecorationArg { allocator_typ: args[1].clone() }),
            b.clone(),
        )),
        Arc::new(TypX::Boxed(b.clone())),
        Arc::new(TypX::Projection {
            trait_typ_args: args,
            trait_path: path("Trait"),
            name: Arc::new("Assoc".into()),
        }),
        Arc::new(TypX::PointeeMetadata(b.clone())),
        Arc::new(TypX::MutRef(b)),
    ]
}

fn function(name: &str, id: u64, typs: &[Typ]) -> Function {
    let typ = typs.last().unwrap();
    let param = |name: &str, id: u64, typ: &Typ| {
        Spanned::new(
            span(id),
            ParamX {
                name: var(name),
                typ: typ.clone(),
                mode: Mode::Spec,
                user_mut: true,
                unwrapped_info: Some((Mode::Proof, var("unwrapped"))),
            },
        )
    };
    Spanned::new(
        span(id),
        FunctionX {
            name: fun(name),
            proxy: Some(Spanned { span: span(id + 2), x: path("proxy") }),
            kind: FunctionKind::TraitMethodImpl {
                method: fun("Trait::exec"),
                impl_path: path("impl"),
                trait_path: path("Trait"),
                trait_typ_args: Arc::new(typs.to_vec()),
                inherit_body_from: Some(fun("default")),
            },
            visibility: Visibility { restricted_to: Some(path("module")) },
            body_visibility: BodyVisibility::Uninterpreted,
            opaqueness: Opaqueness::Opaque,
            owning_module: Some(path("module")),
            mode: Mode::Spec,
            typ_params: Arc::new(vec![Arc::new("T".into())]),
            typ_bounds: Arc::new(vec![
                Arc::new(GenericBoundX::Trait(
                    TraitId::Path(path("Trait")),
                    Arc::new(typs.to_vec()),
                )),
                Arc::new(GenericBoundX::TypEquality(
                    path("Trait"),
                    Arc::new(typs.to_vec()),
                    Arc::new("Assoc".into()),
                    typ.clone(),
                )),
                Arc::new(GenericBoundX::ConstTyp(typ.clone(), typs[0].clone())),
            ]),
            params: Arc::new(
                typs.iter()
                    .enumerate()
                    .map(|(i, t)| param(&format!("p{i}"), id + 10 + i as u64, t))
                    .collect(),
            ),
            ret: param("ret", id + 1, typ),
            ens_has_return: true,
            require: Arc::new(vec![]),
            ensure: (Arc::new(vec![]), Arc::new(vec![])),
            returns: None,
            decrease: Arc::new(vec![]),
            decrease_when: None,
            decrease_by: Some(fun("termination")),
            fndef_axioms: None,
            mask_spec: None,
            atomic_update: None,
            unwind_spec: None,
            item_kind: ItemKind::Function,
            attrs: Arc::new(FunctionAttrsX::default()),
            body: None,
            extra_dependencies: vec![fun("extra")],
            async_ret: Some(param("future", id + 3, typ)),
            hidden: Arc::new(vec![fun("hidden")]),
        },
    )
}

fn call(id: u64, typ: &Typ, kind: CallTargetKind, name: &str) -> Expr {
    expr(
        id,
        typ,
        ExprX::Call {
            target: CallTarget::Fun(
                kind,
                fun(name),
                Arc::new(vec![typ.clone()]),
                Arc::new(vec![ImplPath::TraitImplPath(path("call-impl"))]),
                CallTargetAttrs {
                    autospec: AutospecUsage::IfMarked,
                    const_var: true,
                    assume_external_allowed: name == "missing",
                },
            ),
            args: Arc::new(vec![]),
            post_args: None,
            body: None,
        },
    )
}

fn fixture() -> Krate {
    let mut typs = types();
    let root = Arc::new(TypX::Decorate(
        TypDecoration::Box,
        Some(TypDecorationArg { allocator_typ: typs[18].clone() }),
        Arc::new(TypX::SpecFn(Arc::new(typs.clone()), typs[12].clone())),
    ));
    typs.push(root.clone());
    let mut functions = vec![];
    for (i, (name, autospec)) in [
        ("exec", Some("spec")),
        ("spec", None),
        ("const_exec", Some("const_spec")),
        ("const_spec", None),
        ("Trait::exec", Some("Trait::spec")),
        ("Trait::spec", None),
        ("ImplA::exec", Some("ImplA::spec")),
        ("ImplA::spec", None),
        ("ImplB::exec", None),
        ("ImplB::spec", None),
        ("ImplC::exec", None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut f = function(name, 1000 + i as u64 * 100, &typs);
        Arc::make_mut(&mut Arc::make_mut(&mut f).x.attrs).autospec = autospec.map(fun);
        functions.push(f);
    }
    let dynamic = |resolved| CallTargetKind::DynamicResolved {
        resolved: fun(resolved),
        typs: Arc::new(vec![root.clone()]),
        impl_paths: Arc::new(vec![]),
        is_trait_default: false,
    };
    let mut static_call = call(100, &root, CallTargetKind::Static, "exec");
    if let ExprX::Call { args, post_args, body, .. } = &mut Arc::make_mut(&mut static_call).x {
        *args = Arc::new(vec![constant(101, &root)]);
        *post_args = Some(constant(102, &root));
        *body = Some(constant(103, &root));
    }
    let pattern = SpannedTyped::new(
        &span(180),
        &root,
        PatternX::Binding {
            binding: PatternBinding {
                name: var("local"),
                by_ref: ByRef::MutRef,
                typ: root.clone(),
                user_mut: Some(true),
                copy: false,
            },
            sub_pat: SpannedTyped::new(&span(181), &root, PatternX::Expr(constant(182, &root))),
        },
    );
    let place = SpannedTyped::new(
        &span(170),
        &root,
        PlaceX::WithExpr(
            constant(171, &root),
            SpannedTyped::new(
                &span(172),
                &root,
                PlaceX::Index(
                    SpannedTyped::new(&span(173), &root, PlaceX::Local(var("p0"))),
                    constant(174, &root),
                    ArrayKind::Slice,
                    BoundsCheck::Error,
                ),
            ),
        ),
    );
    let closure = expr(
        190,
        &root,
        ExprX::NonSpecClosure {
            params: Arc::new(vec![Arc::new(VarBinderX { name: var("bound"), a: root.clone() })]),
            proof_fn_modes: Some((Arc::new(vec![Mode::Proof]), Mode::Spec)),
            body: expr(191, &root, ExprX::Var(var("bound"))),
            requires: Arc::new(vec![constant(192, &root)]),
            ensures: Arc::new(vec![expr(193, &root, ExprX::Var(var("closure_ret")))]),
            ret: Arc::new(VarBinderX { name: var("closure_ret"), a: root.clone() }),
            external_spec: Some((var("closure_object"), constant(194, &root))),
        },
    );
    let mut expressions = vec![
        static_call,
        call(104, &root, dynamic("ImplA::exec"), "Trait::exec"),
        call(105, &root, dynamic("ImplB::exec"), "Trait::exec"),
        call(106, &root, dynamic("ImplC::exec"), "Trait::exec"),
        call(107, &root, CallTargetKind::Static, "missing"),
        constant(108, &root),
        expr(
            109,
            &root,
            ExprX::Call {
                target: CallTarget::FnSpec(constant(110, &root)),
                args: Arc::new(vec![constant(111, &root)]),
                post_args: None,
                body: None,
            },
        ),
        closure,
        expr(195, &root, ExprX::Var(var("bound"))),
    ];
    expressions.push(expr(
        196,
        &root,
        ExprX::Assign {
            place: place.clone(),
            rhs: constant(197, &root),
            op: None,
            typ: root.clone(),
            resolve: true,
        },
    ));
    let mut final_call = call(198, &root, CallTargetKind::Static, "exec");
    if let ExprX::Call { target: CallTarget::Fun(_, _, _, _, attrs), .. } =
        &mut Arc::make_mut(&mut final_call).x
    {
        attrs.autospec = AutospecUsage::Final;
    }
    let mut default_target = dynamic("ImplB::exec");
    if let CallTargetKind::DynamicResolved { is_trait_default, .. } = &mut default_target {
        *is_trait_default = true;
    }
    expressions.extend([
        final_call,
        expr(199, &root, ExprX::ConstVar(fun("const_exec"), AutospecUsage::Final)),
        call(200, &root, default_target, "Trait::exec"),
        call(201, &root, CallTargetKind::Static, "spec"),
    ]);
    let mut statements: Vec<Stmt> = expressions
        .into_iter()
        .enumerate()
        .map(|(i, e)| Spanned::new(span(400 + i as u64), StmtX::Expr(e)))
        .collect();
    statements.push(Spanned::new(
        span(420),
        StmtX::Decl {
            pattern,
            mode: Some((Mode::Spec, DeclProph::Prophetic)),
            init: Some(place),
            els: Some(constant(183, &root)),
            assert_irrefutable: true,
        },
    ));
    let mut caller = function("caller", 9000, &typs);
    let f = &mut Arc::make_mut(&mut caller).x;
    f.body = Some(expr(
        300,
        &root,
        ExprX::Block(Arc::new(statements), Some(expr(184, &root, ExprX::Var(var("local"))))),
    ));
    f.require = Arc::new(vec![constant(90, &root)]);
    f.ensure = (
        Arc::new(vec![expr(91, &root, ExprX::Var(var("ret")))]),
        Arc::new(vec![constant(92, &root)]),
    );
    f.returns = Some(constant(93, &root));
    f.decrease = Arc::new(vec![constant(94, &root)]);
    f.decrease_when = Some(constant(95, &root));
    f.fndef_axioms = Some(Arc::new(vec![constant(96, &root)]));
    f.mask_spec = Some(MaskSpec::InvariantOpens(span(97), Arc::new(vec![constant(98, &root)])));
    f.atomic_update = Some(constant(99, &root));
    f.unwind_spec = Some(UnwindSpec::NoUnwindWhen(constant(89, &root)));
    functions.push(caller);
    Arc::new(KrateX {
        functions,
        reveal_groups: vec![],
        datatypes: vec![],
        opaque_types: vec![],
        traits: vec![],
        trait_impls: vec![],
        assoc_type_impls: vec![],
        modules: vec![Spanned::new(span(9999), ModuleX { path: path("module"), reveals: None })],
        external_fns: vec![fun("external")],
        external_types: vec![path("ExternalType")],
        path_as_rust_names: vec![],
        arch: Arch { word_bits: ArchWordBits::Exactly(64) },
    })
}

fn legacy<F>(function: &Function, map: &mut VisitorScopeMap, expr: &F) -> Result<Function, VirErr>
where
    F: Fn(&mut VisitorScopeMap, &Expr) -> Result<Expr, VirErr>,
{
    crate::ast_visitor::map_function_visitor_env(
        function,
        map,
        &mut (),
        &|_, map, e| expr(map, e),
        &|_, _, s| Ok(vec![s.clone()]),
        &|_, t| Ok(t.clone()),
        &|_, _, p| Ok(p.clone()),
    )
}

#[derive(Default)]
struct Identities {
    types: HashSet<*const TypX>,
    type_order: Vec<*const TypX>,
    spans: Vec<(u64, crate::messages::RawSpan)>,
}
impl AstVisitor<Walk, VirErr, crate::ast_visitor::NoScoper> for Identities {
    fn visit_typ(&mut self, t: &Typ) -> Result<(), VirErr> {
        self.types.insert(Arc::as_ptr(t));
        self.type_order.push(Arc::as_ptr(t));
        self.visit_typ_rec(t)
    }
    fn visit_expr(&mut self, e: &Expr) -> Result<(), VirErr> {
        self.spans.push((e.span.id, e.span.raw_span.clone()));
        self.visit_expr_rec(e)
    }
    fn visit_stmt(&mut self, s: &Stmt) -> Result<(), VirErr> {
        self.spans.push((s.span.id, s.span.raw_span.clone()));
        self.visit_stmt_rec(s)
    }
    fn visit_place(&mut self, p: &Place) -> Result<(), VirErr> {
        self.spans.push((p.span.id, p.span.raw_span.clone()));
        self.visit_place_rec(p)
    }
    fn visit_pattern(&mut self, p: &Pattern) -> Result<(), VirErr> {
        self.spans.push((p.span.id, p.span.raw_span.clone()));
        self.visit_pattern_rec(p)
    }
    fn visit_param(&mut self, p: &Param) -> Result<(), VirErr> {
        self.spans.push((p.span.id, p.span.raw_span.clone()));
        self.visit_typ(&p.x.typ)
    }
}
fn identities(f: &Function) -> Identities {
    let mut result = Identities::default();
    result.spans.push((f.span.id, f.span.raw_span.clone()));
    if let Some(proxy) = &f.x.proxy {
        result.spans.push((proxy.span.id, proxy.span.raw_span.clone()));
    }
    if let Some(MaskSpec::InvariantOpens(span, _) | MaskSpec::InvariantOpensExcept(span, _)) =
        &f.x.mask_spec
    {
        result.spans.push((span.id, span.raw_span.clone()));
    }
    result.visit_function(f).unwrap();
    result
}

#[test]
fn complete_ast_and_real_autospec_rewrites_match_legacy() {
    let input = fixture();
    let functions: HashMap<_, _> =
        input.functions.iter().map(|f| (f.x.name.clone(), f.clone())).collect();
    let mut old = (*input).clone();
    old.functions = input
        .functions
        .iter()
        .map(|f| legacy(f, &mut ScopeMap::new(), &|_, e| simplify_one_expr(&functions, e)).unwrap())
        .collect();
    let actual = resolve_autospec(&input).unwrap();
    assert_eq!(tokens::capture(&old), tokens::capture(&actual));
    let original = identities(input.functions.last().unwrap());
    let previous = identities(old.functions.last().unwrap());
    let current = identities(actual.functions.last().unwrap());
    assert!(
        previous.types.difference(&original.types).count() > 20,
        "old mapper must really reconstruct nested types"
    );
    assert_eq!(
        current.types.difference(&original.types).count(),
        0,
        "new mapper must retain every original type allocation"
    );
    assert_eq!(
        previous.type_order.len(),
        current.type_order.len(),
        "both real rewrites retain the same number of type occurrences"
    );
    // Real autospec legitimately removes redundant target-type branches when
    // replacing DynamicResolved or AssumeExternal call targets. Check exact
    // positional Arc preservation where expression rewriting is identity.
    let source_function = input.functions.last().unwrap();
    let identity = |_: &mut VisitorScopeMap, expr: &Expr| Ok(expr.clone());
    let old_identity = legacy(source_function, &mut ScopeMap::new(), &identity).unwrap();
    let new_identity =
        map_function_preserving_types(source_function, &mut ScopeMap::new(), &identity).unwrap();
    assert_eq!(tokens::capture(&old_identity), tokens::capture(&new_identity));
    assert_eq!(identities(&old_identity).type_order.len(), original.type_order.len());
    assert_eq!(
        identities(&new_identity).type_order,
        original.type_order,
        "every type occurrence retains its original Arc under identity expression rewriting"
    );
    for (before, (old, new)) in
        input.functions.iter().zip(old.functions.iter().zip(&actual.functions))
    {
        let before = identities(before);
        for output in [identities(old), identities(new)] {
            assert_eq!(output.spans.len(), before.spans.len());
            for ((before_id, before), (after_id, after)) in before.spans.iter().zip(&output.spans) {
                assert_eq!(before_id, after_id);
                assert!(Arc::ptr_eq(before, after));
            }
        }
    }
    assert!(Arc::ptr_eq(&input.modules[0].span.raw_span, &actual.modules[0].span.raw_span));
    let ExprX::Block(stmts, _) = &actual.functions.last().unwrap().x.body.as_ref().unwrap().x
    else {
        panic!("fixture block");
    };
    let call_at = |i: usize| {
        let StmtX::Expr(e) = &stmts[i].x else {
            panic!("fixture expression");
        };
        e
    };
    assert!(
        matches!(&call_at(0).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Static, target, _, _, _), .. } if *target == fun("spec"))
    );
    assert!(
        matches!(&call_at(1).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Static, target, _, _, _), .. } if *target == fun("ImplA::spec"))
    );
    assert!(
        matches!(&call_at(2).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::DynamicResolved { resolved, .. }, target, _, _, _), .. } if *target == fun("Trait::spec") && *resolved == fun("ImplB::spec"))
    );
    assert!(
        matches!(&call_at(3).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Dynamic, target, _, _, _), .. } if *target == fun("Trait::spec"))
    );
    assert!(matches!(&call_at(4).x, ExprX::Call { target: CallTarget::AssumeExternal, .. }));
    assert!(
        matches!(&call_at(10).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Static, target, _, _, attrs), .. } if *target == fun("exec") && attrs.autospec == AutospecUsage::Final)
    );
    assert!(
        matches!(&call_at(11).x, ExprX::ConstVar(target, AutospecUsage::Final) if *target == fun("const_exec"))
    );
    assert!(
        matches!(&call_at(12).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Dynamic, target, _, _, _), .. } if *target == fun("Trait::spec"))
    );
    assert!(
        matches!(&call_at(13).x, ExprX::Call { target: CallTarget::Fun(CallTargetKind::Static, target, _, _, _), .. } if *target == fun("spec"))
    );

    assert!(
        matches!(&call_at(5).x, ExprX::ConstVar(target, AutospecUsage::Final) if *target == fun("const_spec"))
    );
    assert!(
        matches!(&call_at(6).x, ExprX::Call { target: CallTarget::FnSpec(e), .. } if matches!(&e.x, ExprX::ConstVar(target, AutospecUsage::Final) if *target == fun("const_spec")))
    );
}

#[test]
fn expression_postorder_scope_and_error_prefix_match_legacy() {
    let input = fixture();
    let f = input.functions.last().unwrap();
    let functions: HashMap<_, _> =
        input.functions.iter().map(|f| (f.x.name.clone(), f.clone())).collect();
    for fail in [None, Some(102)] {
        let mut traces = vec![];
        let mut results = vec![];
        for old in [true, false] {
            let trace = RefCell::new(vec![]);
            let mut map = ScopeMap::new();
            let callback = |map: &mut VisitorScopeMap, e: &Expr| {
                let bindings = ["p0", "local", "bound", "ret", "closure_ret"].map(|name| {
                    map.get(&var(name))
                        .map(|entry| (entry.user_mut, entry.init, entry.is_outer_param_or_ret))
                });
                trace.borrow_mut().push((e.span.id, map.num_scopes(), bindings));
                if Some(e.span.id) == fail {
                    return Err(crate::messages::error(&e.span, "intentional callback failure"));
                }
                simplify_one_expr(&functions, e)
            };
            let result = if old {
                legacy(f, &mut map, &callback)
            } else {
                map_function_preserving_types(f, &mut map, &callback)
            };
            results.push(match result {
                Ok(value) => (true, tokens::capture(&value)),
                Err(error) => (false, tokens::capture(&error)),
            });
            traces.push(trace.into_inner());
            if fail.is_none() {
                assert_eq!(map.num_scopes(), 0);
            }
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(traces[0], traces[1]);
        if fail.is_some() {
            assert!(!results[0].0);
            assert_eq!(traces[0].last().unwrap().0, 102);
        } else {
            let ids: Vec<_> = traces[0].iter().map(|e| e.0).collect();
            let positions: Vec<_> =
                [101, 102, 103, 100].map(|id| ids.iter().position(|n| *n == id).unwrap()).to_vec();
            assert!(positions.windows(2).all(|p| p[0] < p[1]));
            let at = |id| traces[0].iter().find(|event| event.0 == id).unwrap();
            assert!(at(191).2[2].is_some());
            assert!(at(195).2[2].is_none());
            assert!(at(193).2[4].is_some());
            assert!(at(91).2[3].is_some());
            assert!(at(184).2[1].is_some());
            assert!(at(182).2[1].is_none());
        }
    }
}

#[test]
fn every_type_variant_keeps_the_entire_original_graph() {
    let input = types();
    assert_eq!(input.len(), 21);
    for typ in input {
        assert!(Arc::ptr_eq(&preserve_type(&typ), &typ));
        let mut visits = 0;
        let old = crate::ast_visitor::map_typ_visitor_env(&typ, &mut visits, &|visits, t| {
            *visits += 1;
            Ok(t.clone())
        })
        .unwrap();
        assert!(visits > 0);
        assert_eq!(tokens::capture(&typ), tokens::capture(&old));
        // These variants have nested type allocations under the old mapper.
        if matches!(
            &*typ,
            TypX::SpecFn(_, _)
                | TypX::AnonymousClosure(_, _, _, _)
                | TypX::FnDef(_, _, _)
                | TypX::Datatype(_, _, _)
                | TypX::Dyn(_, _, _)
                | TypX::Opaque { .. }
                | TypX::Primitive(_, _)
                | TypX::Decorate(_, _, _)
                | TypX::Boxed(_)
                | TypX::Projection { .. }
                | TypX::PointeeMetadata(_)
                | TypX::MutRef(_)
        ) {
            assert!(!Arc::ptr_eq(&typ, &old));
        }
    }
}
