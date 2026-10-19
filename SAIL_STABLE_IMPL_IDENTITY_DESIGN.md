# VI-0030: concrete trait identity source prototype

Status: prototype02 source only on accepted `e5365c05`, uncompiled and unselected.
No compiler, fixture, unit, runtime or gain result is claimed. Proposed Sail
ADR 0056 records this additional renaming policy. The accepted-runtime Vest
witness establishes the cause: 23 ASN.1 method paths and 30 literal input
sessions changed after an unrelated empty impl insertion. Twenty-one changed
paths are Clone methods; the two remaining proof methods still require header
classification. Untouched CBOR inputs changed on a Clone contract symbol.

Prototype01's member-set/family design failed independent static review:
private and restricted ghost members disappear from native metadata, so producer
and consumer need not reconstruct the same associated set or peer inventory.
Its sealed source packet remains retained; it was never compiled or selected.
Prototype02 removes this dependence rather than treating an import mismatch as
an error or silently choosing different naming policies on the two sides.

## Implemented subset and uniqueness

Only positive, non-marker trait implementations with a zero-argument nominal
Self ADT and a trait reference whose only argument is Self qualify. Inherent
impls, negative/reservation impls, marker traits, generic Self instantiations,
traits with other arguments, nested or ambiguous named paths and over-limit
keys retain their legacy identities. No general Rust type encoder is added.

A ground trait reference names one coherent positive implementation. Verus's
custom `rustc_hir_analysis::check_crate` explicitly runs `coherent_trait`,
inherent validity and inherent overlap checks before registry activation;
`verifier.rs` returns on any compiler error before enabling new names. Conflicting
positive implementations therefore never enter the new namespace. Marker traits
are excluded because rustc explicitly permits their overlapping implementations;
reservation implementations are likewise excluded. Ordinary successful native
rlib/metadata compilation performs coherence before its metadata can be consumed.
The foreign DefId uses the same typed header rule after the consumer HIR check.

Impl predicates and unused impl lifetimes are not identity fields or eligibility
switches. Satisfied ground predicates cannot select a second implementation of
the same ground trait reference. Safety, constness and defaultness likewise do
not distinguish that implementation's identity and are neither encoded nor used
as legacy/stable switches. No associated item or peer inventory is read. A private
spec/proof method or const disappearing, a ghost-only inherent block becoming
empty, or an earlier impl disappearing cannot change the key of a retained
concrete trait implementation. No raw impl DefPathHash is used as an import join.

The key `i2` contains full stable crate ID, typed parent module path, typed Self
ADT path and typed trait path, each length-prefixed, then rendered injectively
as `impl&%s2<full hex>`. Named paths use rustc DefKeys and full crate identity,
not friendly strings or Debug output. Named nonzero disambiguators remain
unsupported. No body, span, source position, DefIndex, CrateNum, member set or
encounter order enters a key. The 512-byte bound refuses without truncation or
partial hashing. Numeric legacy names cannot alias the reserved namespace.

This is not a proof of arbitrary producer/native cfg-program equivalence.
Cfg-driven changes to the semantic header, named hygiene, type arguments or
parent module can cross this subset's boundary even while an old human VIR path
looks equal. That producer/native boundary remains an explicit source-review
obligation before any build or selection; known ghost-member erasure alone does
not change those fields. No invented error/fallback protocol or exported mapping
is added. Baseline-valid input must not acquire new diagnostics.

## Compiler context, early paths and closures

One synchronized DefId-to-result cache belongs to `VerusItems`. It holds neither
TyCtxt nor process-global state and caches no hash on public mutable PathX.
Compiler queries occur outside the lock. There are no family scans, including
foreign `trait_impls_in_crate` scans. DefId is only a compiler-local cache handle.

The existing early external-item scan runs with semantic queries disabled.
After successful custom HIR/coherence checking, external paths in both the item
vector and owner map are refreshed without repeating classification/diagnostics.
Containing module paths are reconstructed from each item's parent, including
Mod items and root fallback. Debug invariants check vector/map parity. Every
normal path conversion, including autospec's diagnostic-rename bypass, uses the
same registry. Existing special path redirects and legacy suffix rules remain.
No `iter_local_def_id` call forces ordinary analysis before Verus's HIR check.

A closure under a supported trait impl uses the same whole parent key plus its
existing relative DefKey chain. The closure record is bounded at 1,024 bytes;
unsupported/over-limit suffixes remain legacy. Local anonymous disambiguators
remain, so editing/reordering closures inside one function is outside the promise.
ClosureId retains its optional full shared key in equality, hash, solver names
and exported VIR. Stable identities retain the full crate half and do not retain
the old impl-number-derived local hash; legacy cases retain both original halves.
Existing declaration order remains the old index with full identity breaking
imported equal-index ties. All Copy uses were replaced by borrowing/Clone.

Matching compiler, vstd and all exports must be rebuilt together. A fixed
`VERUS-SAIL-IMPL-ID-02` format header is written before bincode serialization
and checked before decoding. Missing, truncated, old or mismatched headers
refuse with an explicit rebuild message. This policy version must change with
identity/wire changes; it is not a claim that arbitrary different runtimes are
interchangeable. Source/runtime matching remains compulsory. Stock and mixed
exports are unsupported. The changed ClosureId wire format is incompatible
with older serialized VIR. Complete actual
imported closure-type coverage remains required; a serde unit alone is not that
qualification. No imported native inventory or new wire map is introduced here.

## Paired filename repair and costs

Full hex function names can exceed a 255-byte log component. The independent
VI-0029 paired filename helper starts from `470258b0`; prototype02 additionally
preserves the genuine trailing `._NN[_expand]` spinoff grammar outside its full
original-stem digest, so Sail can retain obligation IDs. AIR and transcript still
share a bounded stem. Counter spelling is hash-bound; a shortened counter uses
bounded decimal with at least two digits. New helper and Sail attribution
controls remain unrun. No SCC code or dependency was added.

Full keys increase SMT bytes, CPU and memory; per-impl queries and stored keys
also have costs. Removing family scans does not establish useful performance.
Inherent identities remain edit-sensitive. The original full 23-path/30-session
witness is retained, and a future matched run must classify every affected path
and session rather than declaring all churn fixed from source alone.

## Unrun controls and remaining gates

Compiler/VIR units cover full-field injectivity and bounds, whole closure-key
serialization, hash/equality/name/namespace and imported ordering. Export units
construct a genuine complete old-schema payload that decodes with bincode but
fails the new header, reject missing/truncated/mismatched headers, and round-trip
actual CrateWithMetadata through the matching new format. Source RVT
controls use actual separately compiled producer rlibs plus exported VIR for
private/restricted ghost methods and consts, ghost-only/duplicate inherent blocks,
retained trait method calls, unused impl lifetimes with a ground predicate, and
an over-limit header that stays legacy on both sides. Inherent/generic/trait-arg
legacy controls, a padded empty-impl edit before a trait method/closure and
Clone, complete literal input maps, failed-proof complete diagnostics, conflicting
impl coherence errors before activation, and containing-module/item-map parity
are included. The early external trait impl must refresh to a stable identity.
Every new assertion is uncompiled and unrun.

Literal-input comparison retains empty sessions, duplicate bodies, query bytes
and echo terminators; nothing is normalized or removed. The reduced standalone
corpus fixture also remains unrun. The original prototype01 checks are preserved
in its sealed packet; changing an unrun inherent fixture to assert intentional
legacy scope records the corrected design, not an acceptance result.

Independent source review must resolve producer/native header eligibility and
coherence uniqueness before the first compiler gate. Then require a normal
matching runtime/vstd build, full fork source suite, complete per-function verdict
and diagnostic conformance, determinism and final-currentness guards. Actual
accepted/candidate edit-loop measurement must retain all samples and exact solver
work, including cold overhead and failures. No gain, runtime or pin is selected.
