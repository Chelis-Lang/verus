# VI-0030: function-only SMT spelling prototype03

Status: production source `0a4a06cb` is independently source-reviewed, built with
matching native Linux runtime/vstd, and passes the focused controls plus a useful
27-call matched edit screen. It remains unqualified for selection. This delivery
branch changes regression fixtures and documentation only; its standalone test
adaptation is not yet executed. Earlier source packets01/02 failed independent
correctness review and were never built. Proposed Sail ADR 0056 records this
additional name-generation policy.

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

## Controls, measured screen and open gates

Five pure VIR controls pass: all duplicate labels legacy independent of order
and later pruning; unsupported/namespace/cap whole-map fallback; complete-field
injectivity and per-key refusal without truncation; inline AIR/flag whole-map
fallback; original paths, multiple contexts, crate registration and shared
req/ens/fuel/FnDef stem spelling. Synthetic recursive names retain original
identities.

Eight repaired actual compiler fixtures pass independently against the accepted
`e5365c05` runtime and the production candidate: eligible Copy/Clone spelling
across a padded empty-impl edit while original timing paths still shift; repeated
complete unchanged-program inputs; inherent/generic legacy scope; actual
producer/export/native-rlib/consumer cfg-marker, private/restricted ghost and
ghost-only/empty blocks; literal inline AIR; recursive/inherited-default
consistency; and complete failed-proof diagnostics. The two programs containing
impossible-predicate and const-specialization declarations are strictly compared
original frontend refusals. The accepted compiler rejects them too; they do not
provide positive two-header runtime coverage. All-duplicate fallback remains
mandatory in the pure VIR control. The earlier five failed fixture results and
the wrapper failure before tests remain preserved rather than substituted with
passes.

Normal fork-suite execution requires no fixture environment. Each case owns a
`tempfile::TempDir`; `SAIL_VI0030_BASELINE_SCREEN` is false when absent. In the
three negative cases, a second actual compiler run uses identical source and
`-V allow-inline-air`, disabling aliases for that invocation. Complete stderr
must match byte for byte. This is a same-runtime legacy control, not independent
accepted-runtime conformance. Its delivery adaptation is source-ready and still
awaits default-mode native execution.

The separate independent baseline mode is retained: provide
`SAIL_VI0030_FIXTURE_ROOT` and `SAIL_VI0030_BASELINE_DIAGNOSTICS`, run the same
fixture executable beside the genuine accepted runtime with
`SAIL_VI0030_BASELINE_SCREEN=1`, then beside the candidate without that variable.
Both runs use the same explicit fixture root, so complete stderr comparison
needs no path or diagnostic normalization. Raw source, arguments, stdout,
stderr, VIR, AIR and transcripts remain in the caller-owned root. Missing
baseline diagnostics refuse this mode; they never silently become a legacy
control.

The transcript adapter retains complete literal query bytes/echo terminators,
empty sessions, duplicate bodies and every file. Extracted symbol observations
do not replace, normalize or filter the retained input. Different-program
frontend paths intentionally remain original and can change.

## Actual native Vest screen

The accepted-runtime cause witness changes 21 Clone paths and two inherent spec
paths among 23 timed method changes; this omits spec declarations. Its 30 changed
literal sessions include five Clone-only cases and 25 with deep_view/other
symbols or raw impl labels. The map is not Clone-name filtered: supported concrete
spec/trait methods can qualify too. No claim that all 30 sessions are fixed is
made.

The candidate normal artifact is
`10eff8fba0c3c8d4c517742ddf7db16284972e1d67b2236a1d18f31309b701a7`, built
from `0a4a06cb6431d5a36e1ea4c8eb9c8da938866a24`; the genuine accepted artifact
is `689004c074aca8883e225a2b89efebc137f3e563f24df2abd398516112b13928`.
Sail source is `83128ad0d5327f00bfea822d608ebd99dd9d8112`; Vest is
`11af01ddaef76cceded03517cd48764adb9d35b1`. On native EC2 c7i.4xlarge, 16 vCPU,
us-east-1, the original three-actor harness retains all 27 calls in three rotated
rounds. Its only recipe change inserts the exact unrelated empty `impl TagFmt { }`
in `vest_lib/src/asn1/mod.rs`; timing and per-call checks remain unchanged.

| Metric | Accepted | Candidate |
|---|---:|---:|
| Edited wall samples, seconds | 45.089, 45.108, 44.968 | 34.266, 33.955, 33.885 |
| Edited wall median, seconds | 45.089 | 33.955 |
| Edited solver processes started, every round | 30 | 24 |
| Edited summed-thread SMT median, seconds | 46.650 | 30.423 |
| Cold wall median, seconds | 89.992 | 89.595 |
| A-return-plus-comment wall median, seconds | 31.105 | 30.959 |

The edited median saving is 24.693%; every candidate edited sample is below
every accepted edited sample. The original at-least-5% plus full-separation
criterion passes. A-return restores A and adds a comment; it is not unchanged B.
This is useful edit evidence, not a first-verification gain or default acceptance.
All 27 calls and final source/runtime/body/corpus guards pass. Complete raw
capture SHA-256 is
`9f102bb20ac4fbbd000bbd7392c32423aa42031efdd092d2dc26450a8aa1c96a`
(8,296 members, 69,400,537 raw bytes); every member/type/mode/hash was verified.
The combined focused/timing summary SHA-256 is
`03288ab9f9c72739444db044f54d35f6c761efebb7ac11164dafee21d20aa858`.

Full target-session literal input-hash multisets preserve duplicate and empty
sessions: A-return versus B changes 30 of 266 sessions in every accepted round
and 24 of 266 in every candidate round. Independently rooted benchmark copies
also have identical 266-entry A-return hash multisets across all three rounds
in both arms. This passive observation does not replace controlled raw
StableCrateId/physical-root qualification or full named-stream attribution.

Remaining gates are: the standalone delivery tests; the complete fork source
suite; a fresh alias-specific six-call/four-comparison complete per-function
verdict and diagnostic matrix; full named SMT/body census and residual-stream
classification; controlled raw crate-ID/root variation; determinism; and full
corpus qualification. The old SCC matrix's cross-runtime byte-equality assertion
is inapplicable to intentional aliases and has not been silently weakened or
reported as a pass. A linear complete-set scan, key grouping/storage, the extra
renderer lookup, full AST raw-AIR scan and longer full-hex SMT names all have real
costs; the screen does not establish their whole-corpus cost.

Logical bijection alone cannot guarantee Z3's resource-limited verdict. No
runtime/default pin is selected from this screen, and Sail's exact-byte key
policy remains unchanged.
