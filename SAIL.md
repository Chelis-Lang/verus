# Sail's Verus fork

This repository is a fork of [Verus](https://github.com/verus-lang/verus)
maintained for [Sail](https://github.com/Chelis-Lang/sail), a service that sits
between Verus and its pinned Z3 to make verification fast and stable. (The Sail
repository is private until Sail's open-source release; references below to
Sail ADRs, `VI-NNNN` issue entries and Sail paths point into it.)

"Stock" here means unmodified upstream Verus. For Verus itself (documentation,
releases and contributions) use upstream
[verus-lang/verus](https://github.com/verus-lang/verus).

## Where to report problems

- Problems in Verus itself: upstream
  [verus-lang/verus](https://github.com/verus-lang/verus/issues).
- Issues about the fork's changes (the divergences, releases or CI below) are
  welcome in this repository's issue tracker.
- Security vulnerabilities: report them privately with **Report a
  vulnerability** on this repository's Security tab, not in a public issue.

## Why it exists

The fork builds Sail's Verus runtime. Sail runs it for every verification
(default `sail verify` included; Sail ADR 0043) and ships it in Sail's
release archives; stock Verus is used only as the oracle Sail's conformance
tests compare against. The runtime also includes Sail's opt-in interactive
repair engine, which runs focused verification of one function and then an
independent full verification in a single compiler invocation.

Some upstream behaviour hurts Sail directly: output whose order varies
between runs (which defeats Sail's exact-prefix solver cache and exact
cached-versus-fresh comparison) and
structured output that is wrong or ambiguous. The fork fixes those issues
under a narrow policy (Sail ADR 0039). A fix may only:

- make stock's output deterministic by choosing one of the outputs stock can
  already produce; or
- correct structured output (`--output-json`, timing, logs) without changing
  verdicts or diagnostics; or
- rename closure types by the complete stable crate-qualified definition
  identity (Sail ADR 0049): both 64-bit halves of rustc's `DefPathHash` enter
  equality, hashing and names. The SMT output equals stock's up to a one-to-one
  renaming of closure symbols, in stock's local order. Imported trait-default
  bodies can carry the same local index from different crates; those declaration
  ties are ordered by the complete identity, independently of hash-map iteration.

Anything that would change a verdict needs its own Sail ADR. Each fix is
tracked as a `VI-NNNN` entry in Sail's `docs/verus-issues/` (private Sail
repository), with a reproduction. Fixes are not proposed upstream.

The unselected VI-0023 candidate extends this policy under proposed Sail
ADR 0054: spec formal parameters receive reserved solver names by position,
including the corresponding termination-proof parameters. Source identifiers
and diagnostic spans remain intact; proof and executable parameters are not
renamed by this rule. This changes solver input names and therefore requires
fresh matching vstd/exports and full verdict/diagnostic qualification before
selection. The combined candidate below is not qualified by its source review
or by measurements of either component alone.

One change is of another kind, under its own Sail ADR (0048): Sail ships
vstd prebuilt, compiled with its registry sources named by rustc's virtual
rustc-dev base, and the driver maps that base back to the user's own
registry directory when Sail asks (`SAIL_RUSTC_DEV_SOURCE_BASE`), so
diagnostics read exactly as stock's do for a vstd compiled on the user's
machine. Without the variable the driver is unchanged.

The disabled compiler-boundary experiment also supports a private compiler-read
observation under ADR 0048. `SAIL_COMPILER_INPUTS` names a fresh private directory
and `SAIL_COMPILER_INPUTS_BINDING` supplies a 64-digit request binding. The driver
retains source-map checksums and original byte lengths separately for GHOST
verification and EXEC compilation, then writes one `inputs.json` only after both
phases succeed. Missing, duplicate or incomplete phases refuse observation.
Extra tracked paths and tracked environment are recorded separately; reopened
dep-info hashes are not presented as evidence of earlier reads. The consumer
must still check the actual child, full proof result and complete input model.
Absent the request, no observation is made. This experiment is not a selected
Sail runtime or a qualified prebuilt-vstd release.

The separate `SAIL_NATIVE_COMPILER_INPUTS` request, with
`SAIL_NATIVE_COMPILER_INPUTS_BINDING`, observes Cargo's ordinary native Rust
path. Its genuine `after_analysis` callback captures one compiler source map,
extra tracked inputs and environment; a normal compiler return writes one
`sail-native-compiler-read-inputs` document. It never manufactures GHOST/EXEC
phases or establishes a proof. Missing, repeated or changed requests decline.
Sail must still require actual successful child exit, admit every loaded native
source against its exact official policy, and bind the actual consumed native
dependency graph. In particular, source observation alone does not establish
what mutable `--extern` or `-L` inputs the compiler loaded. This protocol is
unselected. Four controls pass on a Mac pinned-compiler API witness (actual
source B survives restoration of A in the record, unchanged A artifact bytes,
failed compilation emits no record); the witness stubs source-base configuration.
The full fork build, Linux consumer integration and complete input qualification
remain open.

## Branches and tags

- `upstream/<short sha>`: an unmodified upstream Verus commit used as a base,
  for example `upstream/426d8b0` (the current base) and `upstream/8dea4a2`.
- `sail` (default): the current upstream base, then, in order (fixes made
  after the engine sit on top of item 3 until the next upstream rebase moves
  them below the engine):
  1. one harness commit and one commit per fix, each fix with its regression
     test `source/rust_verify_test/tests/sail_vi<NNNN>_*.rs`;
  2. `feat: interactive repair engine`, the engine itself;
  3. fork documentation and CI (this file, the README notice,
     CONTRIBUTING.md and `.github/workflows/sail-fork.yml`), which change no
     Verus source.
- `pinned/<short sha>`: tags every fork commit a Sail runtime pin has ever
  named, so it stays reachable after `sail` is rebased or a fix is dropped.
  Tags are never moved or deleted.
- `archive/*`: tags retaining historical work.

The qualified full-identity runtime is built from
`cc7bbce4fdc8cf7e4256668b3ee8431343bdb6b9`; its matching Sail pin takes effect
when the pin PR merges. Sail pins that full source commit, not the branch or
this documentation successor. Documentation commits move the `sail` tip past
the pinned commit; that is expected and does not affect the runtime source.

## Divergences from upstream

`git log --reverse upstream/426d8b0..sail`, Verus source commits only:

| Commit | Subject | Sail issue | What changes for a user | Regression test |
|---|---|---|---|---|
| `15763f47` | test: pass raw verifier arguments through the rust_verify_test harness | (harness) | Nothing; tests only. `--sail-raw=<arg>` passes one argument to the verifier. | used by every `sail_vi*` test |
| `9a5b2cc8` | fix: generate enum arrow accessors in source order | VI-0001 | Generated `arrow_*` accessors, and the SMT input, are emitted in field source order instead of `HashMap` order (direct `verus` runs). | `sail_vi0001_accessor_order` |
| `404a1e30` | fix: choose automatic triggers in a deterministic search order | VI-0015 | Automatic trigger choice is the same on every run; the search and scoring are unchanged, so the trigger is one stock can choose. | `sail_vi0015_trigger_order` |
| `c9de60c6` | fix: report bucket start notes in queue order | VI-0012 | With a function or module filter, "verifying <bucket>" notes print in queue order; text and conditions unchanged. | `sail_vi0012_progress_order` |
| `bf59e1f0` | fix: list output-json func-details sorted by name | VI-0008 | `--output-json` `func-details` is sorted by name instead of hash order. | `sail_vi0008_func_details_order` |
| `8a5d3655` | fix: record bit-vector failures in the function failure set | VI-0003 | A failed bit-vector or Singular query is reported with `"success": false` in `--output-json --time` (stock reports `true`). | `sail_vi0003_bitvector_success` |
| `3566421a` | fix: omit unselected functions from function timing | VI-0009 | With `--verify-function`, functions the filter skipped no longer appear in timing output as successes. | `sail_vi0009_selected_timing` |
| `fbde5ea3` | feat: add function-path to the timing function breakdown | VI-0010 | Timing entries gain a `function-path` field that distinguishes trait implementations sharing a friendly name; existing fields unchanged. | `sail_vi0010_function_path` |
| `324c487c` | fix: give each crate its own log directory under cargo verus | VI-0004 | Under `cargo verus`, each crate logs to `<log-dir>/<crate>` (or `<crate>-bin`) instead of all crates clearing one shared directory. Direct runs unchanged. | `sail_vi0004_cargo_log_dirs` |
| `af212563` | fix: declare enum arrow accessors in source order in the verifier | VI-0001 | The same accessor order for `cargo verus` builds, whose builtin macros come from the project's dependency source rather than this fork. | `sail_vi0001_impl_accessor_order` |
| `13a1176c` | feat: interactive repair engine | (Sail ADR 0027) | Adds the engine Sail drives (`rust_verify/src/sail_engine.rs`, hooks in `rust_verify/src/driver.rs` and `rust_verify/src/verifier.rs`, and a friendly-name helper in `vir/src/ast_util.rs`). Only active when Sail requests it; ordinary runs are unchanged. | Sail's engine differential checks |
| `9e32601e` | fix: list output-json timing modules and functions by name | VI-0017 | `--output-json --time` lists modules and functions sorted by name instead of slowest first (modules) and hash order (functions), so the first entry is no longer the slowest; values and the text `--time` report unchanged. This corrects structured output; it is not an order stock produces. In Sail's pin since `73dea021`. | `sail_vi0017_timing_order` |
| `bc9cda09` | fix: print bucket diagnostics in bucket order | VI-0007 | Errors from different modules print in bucket order instead of completion order; stock was observed printing the same stderr (1 of 40 runs of `debug_expand.rs`, 8 of 20 of the VI-0007 reproduction). A finished module's errors wait until every lower-numbered module finishes, so they can appear later than in stock; the final output is the same. In Sail's pin since `73dea021`. | `sail_vi0007_error_order` |
| `8d03ab54` | fix: name closure types by definition path, not crate-wide index | VI-0019 | Introduces stable definition-path closure names instead of crate-wide `DefIndex`, so inserting an item no longer renames closures in other modules. The initial truncated identity is superseded by `cc7bbce4` below. Declarations keep stock's order; the SMT input equals stock's up to renaming and line wrapping. Included in the cc7 pin update. | `sail_vi0019_closure_ids` |
| `3f7f6c51` | fix: order imported closure index ties deterministically | VI-0019 | Imported trait-default bodies can contain distinct closures with the same crate-local `DefIndex`. Break those declaration-order ties by the stable closure name, rather than hash-map iteration. The two-crate regression observes both exported closure types and checks their order over sixteen fresh processes. Included in the cc7 pin update. | `sail_vi0019_closure_ids` |
| `b4450da9` | feat: name prebuilt registry sources by the user's real Cargo home | (Sail ADR 0048) | Only with `SAIL_RUSTC_DEV_SOURCE_BASE=<virtual base>=<registry directory>`, which Sail sets: every compilation sets rustc's `real_rustc_dev_source_base_dir` to the registry directory, so names under the virtual rustc-dev base (`/rustc-dev/<rustc commit>`) read as the user's own files; exported VIR records such files by the base and imported VIR maps them back. Without the variable nothing changes. | `sail_source_base` library test; Sail's `scripts/shipped-deps-check.py` against stock |
| `5793253c` | fix: serialize proof-note arrays in lexical order | VI-0021 | The two proof-note JSON arrays are sorted; internal set membership, verdicts and rendered diagnostics are unchanged. Included in the qualified cc7 runtime. | `output_json` multi-note tests assert serialized order and original set membership |
| `cc7bbce4` | fix: retain full definition-path identity for closures | VI-0019 | Equality, hashing and SMT names retain both stable crate ID and local definition hash; the declaration index only orders output. Fresh matching runtime, vstd and exports are required. | two VIR identity tests and the 16-run `sail_vi0019_closure_ids` two-crate regression |
| `425ec23b`, `a82c0b82` | feat: observe retained compiler source inputs; fix: seal only after both phases complete | (Sail ADR 0048, disabled experiment) | Optional private source-map observation with separate GHOST/EXEC reads, latched phase failure and request binding; no default verdict, diagnostic or query change. Does not itself authorize reuse. | Source-map checksum and duplicate/missing/failed-phase library tests; native compiler/API qualification pending |
| `fd26e888`, `aead21d2` | perf: stabilize spec formal solver names by position; fix: align spec termination proof parameters | VI-0023, proposed Sail ADR 0054 | Candidate positional names preserve imported caller axioms across a formal rename. Unselected; component measurements do not qualify the combined runtime. | Three spec-name VIR tests and five `sail_vi0023_spec_parameter_names` cases |
| `de1f0e1a` | test: isolate spec parameter regression scratch directories | VI-0023 | Gives each regression case its own harness scratch name, including when exact cases run in separate concurrent processes. Assertions and runtime behavior are unchanged. | All five cases with four test threads and four separate-process workers |

### Qualified full closure identity

The qualified `cc7bbce4` source retains both 64-bit halves of rustc
1.98.1's `DefPathHash`: the stable crate ID and the crate-local definition
hash. Equality, hashing, internal datatype names and closure-impl names use
the complete pair; the crate-local declaration index only controls ordering,
with ties broken by the complete pair. The local hash is seeded by the stable
crate ID, but that does not extend rustc's within-crate collision check to
arbitrary local halves from different crates. The complete pair preserves the
compiler's intended crate-graph identity instead of relying on that truncation.
No actual cross-crate hash collision or wrong verdict has been observed.
The source API is rustc's pinned
[`DefPathHash`](https://github.com/rust-lang/rust/blob/1.98.1/compiler/rustc_span/src/def_id.rs),
using `stable_crate_id().as_u64()` and `local_hash().as_u64()` separately.

The two added unit controls force equal local halves with different crate
halves and check independent map keys, exact hash inputs and both name families.
The existing source-identical two-crate trait-default regression now checks
both identity halves in exported VIR and simplified datatype names. Native
qualification passes both unit controls, all 16 fresh two-crate clients, the
complete fork suite (4,709 Same, 58 ignored; all 4,651 nonignored plain tests
pass), all four FAST and STANDARD arms, and default DEEP (159 Same / 12,726
functions per arm, zero Different/Unstable/infrastructure errors; two optional
Singular projects skipped per arm). Three runs of every current FAST input
(147 inputs, 441 executions) yield 577 query buckets with no varying buckets
or problems under the unchanged determinism oracle. Earlier `8a373f81`
evidence is not substituted for these cc7 runs.

Matching native normal Linux and Apple-silicon runtimes, vstd and dependency
exports were built from cc7. The extra identity field changes exported VIR's
serialized layout; no compatibility with earlier exported VIR is claimed.
The original normal runtime manifests and per-file hashes remain authoritative.

Three rotated native Linux rounds with identical Sail code retain all 54
samples: helper insertion falls from 34.682 s to 25.107 s (27.6%, 1.381x)
against the previous `73dea021` runtime. Body edits rise from 25.130 s to
25.437 s (1.2%); cold medians rise about 0.9%. Both recipe matrices pass all
vstd and Vest per-function verdicts and diagnostic comparisons. Final comment
samples deliberately force frontend work; they are not unchanged receipt
reruns. This is a narrow helper-edit gain, not a full edit-catalogue or repair
2x claim. Cross-runtime SMT identifier differences are retained, not normalized.

Linux staged-bundle smoke passes. Mac build/install/stage/new/edit/repair and
negative-proof components pass, but that delivery run's enclosing global-cache
inventory guard remains FAIL: 57 added crate archives, writer unknown, with
all original installed bytes unchanged. No quiet-cache retry is substituted.
Sail's private `docs/validation/closure-full-identity.md` retains exact source,
runtime and raw evidence hashes, including earlier failed setup attempts.

Dropped fixes (no longer on `sail`; the commits stay reachable from tag
`pinned/4881cd0f`):

- **VI-0005** (forwarded Verus arguments do not re-verify a fresh crate):
  `441d6cd9` and `b35e9226` bound forwarded arguments in dep-info, which made
  every Sail repair rebuild, because the repair passes the selected function
  as a forwarded argument.

Not fixed: **VI-0011** (Cargo `no-vstd` metadata does not reach the launcher's
macro-mode selection). The fix is not small: the launcher would have to
decode Cargo-Verus's per-package driver arguments.

## Releases

From `13a1176c` on, each pinned fork commit that Sail ships has prebuilt
engine runtimes published here as GitHub prereleases. Earlier pinned commits
(`4881cd0f`, `89709da7`, `d2b69aea`) have none.

| Release tag | Platform | Sail pin |
|---|---|---|
| `engine-runtime-13a1176c-linux` | Linux x86-64 | `engine/verus/artifact-linux.json` (until `73dea021`) |
| `engine-runtime-13a1176c-macos` | macOS arm64 (Apple silicon) | `engine/verus/artifact-macos.json` (until `73dea021`) |
| `engine-runtime-73dea021-linux` | Linux x86-64 | `engine/verus/artifact-linux.json` (until `cc7bbce4`) |
| `engine-runtime-73dea021-macos` | macOS arm64 (Apple silicon) | `engine/verus/artifact-macos.json` (until `cc7bbce4`) |
| `engine-runtime-cc7bbce4-linux` | Linux x86-64 | `engine/verus/artifact-linux.json` (cc7 pin update) |
| `engine-runtime-cc7bbce4-macos` | macOS arm64 (Apple silicon) | `engine/verus/artifact-macos.json` (cc7 pin update) |

The cc7 normal archive digests are
`717680b4cb126f48397b02f32dcbf54e0f2a320fa0c16c86a97237a8c3eee8b1`
(Linux, 14 files including the original manifest) and
`65523848b990f49221035a087e685fe9666e65d34cb5b9d1b76f25f690f86d9b`
(macOS, 15 files including the original manifest).

Each release has the archive `engine-runtime.tar.gz` and its checksum file
`engine-runtime.tar.gz.sha256`. The archive holds `sail-build.json` (the build
manifest: provenance and per-file digests) and
`verus/source/target-verus/release/` (`verus`, `cargo-verus`, `rust_verify`,
`z3`, `vstd`, builtin libraries). The runtimes are built and packed by Sail's
tooling on native hosts (`cargo xtask build-verus-engine`,
`ci/engine-artifact.py pack`) and published by a maintainer; this repository's
CI does not build them (see [CI](#ci)).

Sail's pin file names the release (`repo`, `release_tag`, `asset`) and records
the archive SHA-256, the fork and upstream commits, the source lock digest, the
Rust toolchain and the Z3 digest. Sail downloads anonymously and accepts the
runtime only if the archive digest and every file in the manifest match. To
check a download by hand:

```sh
sha256sum -c engine-runtime.tar.gz.sha256   # or: shasum -a 256 -c
```

Assets of a release a pin has named are never replaced or deleted.

## CI

`.github/workflows/sail-fork.yml` runs on every push to `sail`, every pull
request into it, and on demand, on Linux x86-64 only. It installs the toolchain
from `rust-toolchain.toml`, gets Z3 with `source/tools/get-z3.sh` and checks
its SHA-256 against the Z3 Sail pins, builds vargo and then Verus
(`vargo build --release`, which verifies vstd), runs the `rust_verify`, `vir`
and `air` library tests, the fork's regression tests
(`rust_verify_test/tests/sail_vi*.rs`) and upstream's `basic` test, and
rebuilds vstd the way the release does (`cargo-verus build --release
--manifest-path vstd/Cargo.toml`).

It shows that the fork builds, that vstd verifies, and that the fork's fixes
still pass their tests. It only compiles the engine: it does not exercise the
engine's behaviour, which Sail's own engine qualification covers. It does not
run upstream's full `rust_verify_test` suite; Sail's local qualification does
(the qualified cc7 source: 4,709 Same, 58 ignored and all 4,651 nonignored
plain tests passing). This CI uses no
macOS runners; Sail's release tooling builds and qualifies the macOS runtime
natively. Actions are pinned by commit SHA (the repository requires it) and
the workflow token is read-only.

Upstream's workflows under `.github/workflows/` are kept unchanged so rebases
stay clean, but each is disabled in this repository, so none of them
(including the scheduled and release-publishing ones) runs here.

## Build and test locally

On Linux or macOS, with `rustup` installed, from the repository root:

```sh
cd source
./tools/get-z3.sh
# Linux x86-64: check Z3 against the one Sail pins
echo "e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765  z3" | sha256sum -c
export VERUS_Z3_PATH="$PWD/z3"
source ../tools/activate          # builds vargo and puts it on PATH
vargo build --release             # builds Verus and verifies vstd
vargo test --release -p rust_verify_test \
  $(for f in rust_verify_test/tests/sail_vi*.rs; do echo --test "$(basename "$f" .rs)"; done)
```

Upstream's [BUILD.md](BUILD.md) and its
[CONTRIBUTING.md](https://github.com/verus-lang/verus/blob/426d8b01e7ffb910a36c15e1f870ff985bb61eef/CONTRIBUTING.md)
(at the pinned upstream commit) cover the rest.

## Upgrading to a new Verus

1. Push the new upstream commit as `upstream/<short sha>`.
2. On a review branch, rebase: `git rebase --onto upstream/<new> upstream/<old>`.
   Drop fixes the new release already contains; resolve conflicts per commit.
   First set `user.name` and `user.email` explicitly in the clone (`git
   config`) to the maintainer's identity: a rebase or cherry-pick rewrites
   every committer, and with no identity configured Git falls back to
   `user@hostname`. Check author and committer of every rewritten commit
   before pushing.
3. Run every `sail_vi*` regression test and the `rust_verify_test` suite, and
   record the results in Sail.
4. After review, move `sail` to the branch. In Sail, update the upstream and
   fork pins together, rebuild, qualify and publish new runtimes, re-pin them,
   and tag the pinned commit `pinned/<short sha>`.

## Licence

Verus and this fork are under the MIT licence in [LICENSE](LICENSE), which is
unchanged from upstream.

## Unselected SCC node-index prototype

The source-only SCC prototype starts from accepted fork
`e5365c05d0a9f8c8cb655edc035ce9a2f6dde482`. Each node records the numeric SCC id
at the same Tarjan pop that inserts its existing `mapping` entry. The two
internal edge walks in SCC sorting and reachability read that id directly,
instead of hashing the already-indexed edge target again. Both original maps,
their constructors and insertion order, external-key lookups, graph traversal,
SCC membership order and representative order remain unchanged.

Three targeted VIR controls **passed on macOS** with Rust 1.98.1: all 6,144
three-node graph/declaration/edge-order combinations against the original map
sort and reachability walks; explicit parallel-edge/cycle/implicit-node order
witnesses; and actual key-hash counters showing which internal edge lookups
disappear while external lookups remain. The original map-based ordering and
reachability code is retained as a test oracle. The genuine fresh test executable
was emitted for this worktree's VIR package under two jobs, offline and with
debug information and incremental compilation disabled. The three tests passed
in 0.21 seconds; the complete bounded source-unit recipe took 47.40 seconds.
Its private raw result has SHA-256
`f0be79af7a36d83b5dcf3badb47c8b5aa2d2b203cf7e632bfd66bee95515b27a`.
This is source-unit qualification, not a built Verus runtime or native gain.

A baseline-only native Vest CPU observation found `Path::hash` at 7.80% self
cost and a VIR SipHasher write at 6.95%. It does not establish how much either
SCC walk contributes: the observation failed its required autospec attribution
gate, and Rust ancestor coverage still needs retained-data reanalysis. The
autospec type-preservation prototype remains separate and unselected. This SCC
source has no runtime build, native gain measurement, literal-SMT/determinism,
whole-corpus conformance or platform qualification. Ancestor evidence and a
material measured gain must precede any backend selection.
