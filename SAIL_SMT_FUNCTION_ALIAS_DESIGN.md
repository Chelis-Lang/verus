# VI-0030: function-only SMT spelling prototype03

Status: source implemented on accepted e536, uncompiled, unqualified and
unselected. Every new unit and compiler fixture is unrun. Earlier source
packets01/02 failed independent correctness review and were never built.
Proposed Sail ADR 0056 records this additional name-generation policy.

## Preserved boundary

All Rust DefIds/DefPathHashes, VIR Paths/Funs/ImplPaths/ClosureIds, equality/hash,
sorting, diagnostics and imports/exports remain exactly the accepted design.
No renamed identity is used to join native metadata to producer VIR. There is
no new export header, parent registry, external-item refresh, closure key, family
scan, global/TLS state, mutable PathX hash cache or dependency. The rejected
identity implementation and independent filename helper are not included.

The only spelling substitution is NameCtxt::fun_to_string. It renders the
legacy path first, preserving every crate-registration and encounter-order side
effect, then looks up an optional exact-function alias. path_to_string remains
unchanged. The map is not serialized and never changes a lookup key. A producer
and consumer may independently choose different solver spellings: their proof
invocations remain internally consistent and their ABI stays original.

## Complete-set selection

The builder reads the complete merged pre-prune VIR already retained as
ErasureHints.vir_crate. This preserves private/imported collision peers lost by
later crate/module/function pruning. FunctionKind::TraitMethodImpl and
ForeignTraitMethodImpl retain typed trait_path, trait_typ_args, impl_path and
method before external demotion can turn a function into Static.

Support only direct actual members of a numeric legacy impl, with a normal
module owner, nominal Datatype(Path) Self with zero arguments, and no other
trait arguments. Inherited/synthesized paths, inherent functions, nonordinary
paths and unsupported headers retain legacy spelling. Self and trait are typed
VIR values, never inferred from friendly names or a first parameter.

A key contains the explicit domain/version, full typed module crate identity,
Self path, trait path, item-kind and declared trait-member name. Every component
is length-prefixed. No body, span, source offset, impl number, generic-parameter
spelling, predicate inventory, member inventory or encounter order enters it.
Keys are bounded at 512 bytes; oversized keys are never truncated. Group ALL
candidate declarations by full key. Every duplicate group remains legacy,
including impossible-predicate and const-specialized ground duplicates. No
coherence uniqueness, marker-flag policy or producer/native parity is assumed.

Selected names use %sail%fn%<full-key-hex>. in a reserved namespace. Full-key
encoding is injective; ordinary crate function spellings begin with a crate
prefix, and accepted internal/generated namespaces do not use this reserved
prefix. A conflicting current internal declaration or repeated original function
path silently disables the whole map. The selected partial renaming extends by
identity for all unselected solver functions, keeping different original symbols
distinct. An alias is a spelling, not a new Path.

Staging is bounded at 16,384 candidates / 8 MiB encoded keys. Exceeding either bound
silently selects the entirely legacy map, never a partial first winner or error.
Selected hex names may occupy twice the encoded-key bytes; grouping also holds
key copies and map overhead. No useful performance follows from the bound.

Raw inline AIR can mention original solver names. The allow-inline-air flag or
any AirStmt anywhere in all current/imported function headers/bodies disables
aliases for the entire invocation. The existing function AST visitor examines
all expression fields. Raw bytes are never rewritten or given new diagnostics.

## Immutable lifetime and renderer routes

One Arc<SmtAliases> belongs to GlobalCtx. Its explicit constructor argument is
preserved through simplify_krate's GlobalCtx rebuild and from_self_with_log
worker copies. Every Ctx creates its NameCtxt with that same Arc; bucket pruning
cannot reclassify duplicates. There is one complete-set build, no foreign-family
scan and no map update during AIR generation.

| Route | Consistency |
|---|---|
| sst_to_air::fun_to_air_ident | every ordinary function declaration/call uses fun_to_string |
| req/ens/async-ens/openinv/no-unwind predicates | their stems derive from that exact function renderer |
| fuel IDs/naturals/reveal | same renderer; reverse lookup maps solver Ident to original Fun |
| FnDef/static IDs and prelude FnDef qid/skolemid | NameCtxt helpers call fun_to_string |
| Ctx::new fun_ident_map | constructed with the alias-bearing NameCtxt, values remain original Fun |
| function internal quantifier labels | derive from selected stems; qid_map values remain original Fun/span |
| diagnostics, AIR function comments, timings and log paths | retain original identities/path_to_string |

Distinct synthesized recursive/default/autospec/FnDefImplPath identities that
have no recorded eligible exact function keep legacy spelling on both definition
and reference routes. Their linking axioms still refer to the selected original
function through the central renderer. No inferred prefix substitution or new
synthetic identity is introduced.

The raw impl qids in traits::trait_impl_to_air and assoc_types_to_air, user
quantifier friendly names and nondefault-fuel friendly labels remain legacy.
They do not call an aliased function by a wrong name, but can retain literal
edit churn. Inherent leaf_inv/nonmal_leaf_inv and accepted closure identities
also remain outside the first screen. No closure gain is claimed.

## Source controls and open gates

Five pure unit controls cover: all duplicate labels legacy independent of order
and later pruning; unsupported/namespace/cap whole-map fallback; complete-field
injectivity and per-key refusal without truncation; inline AIR/flag whole-map
fallback; original paths, multiple contexts, crate registration and shared
req/ens/fuel/FnDef stem spelling. They also retain synthetic recursive names as
original identities. All units are unrun.

Eight actual compiler-fixture sources cover: eligible Clone spelling across a
padded empty-impl edit while original timing paths still shift; repeated exact
complete unchanged-program inputs; inherent/generic legacy scope;
trivial_bounds impossible duplicates; const specialization duplicates; actual
producer/export/native-rlib/consumer cfg-marker, private/restricted ghost and
ghost-only/empty blocks; inline AIR; recursive/inherited-default consistency;
and complete failed-proof diagnostics. All are uncompiled and unrun, including
their Rust/Verus syntax and feature assumptions. A failed compiler control remains
a failed gate; no assertion is weakened to produce acceptance.

The transcript adapter retains complete literal query bytes/echo terminators,
empty sessions, duplicate bodies and every file. Extracted symbol observations
do not replace, normalize or filter the retained input. Different-program
frontend paths intentionally remain original and can change. Complete edit-loop
inputs remain available; this screen does not falsely assert every legacy qid,
inherent name or closure becomes stable.

The actual Vest timing witness includes 21 Clone paths and 2 inherent spec
paths among 23 changes, omitting spec declarations. Of 30 changed literal
sessions, offline classification finds only five Clone-only cases; 25 also
contain deep_view/other symbols or raw impl labels. The map is not Clone-name
filtered: supported concrete spec/trait methods can also qualify. Classify all
actual supported headers and every residual changed session before claiming
improvement. A linear
complete-set scan, key grouping/storage, one extra lookup after existing function
rendering and longer full-hex SMT names all have real costs. Only matched actual
accepted/candidate measurements can establish useful gain.

Independent source review precedes the first compiler gate. Then require a
matching runtime/vstd build, full fork source suite, complete per-function verdict
and diagnostic conformance, determinism and final-currentness guards. Preserve
all actual measurement samples and exact solver work. Logical bijection alone
cannot guarantee Z3's resource-limited verdict; no runtime or default pin is
selected from this source. Sail's exact-byte key policy remains unchanged.
