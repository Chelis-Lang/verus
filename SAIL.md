# Sail's Verus fork

This repository is a fork of [Verus](https://github.com/verus-lang/verus)
maintained for [Sail](https://github.com/Chelis-Lang/sail), a service that sits
between Verus and its pinned Z3 to make verification fast and stable. (The Sail
repository is private until Sail's open-source release.)

For Verus itself (documentation, releases, issues and contributions) use
upstream [verus-lang/verus](https://github.com/verus-lang/verus).

## Why it exists

The fork builds one thing: the runtime of Sail's opt-in interactive repair
engine, which runs focused verification of one function and then an
independent full verification in a single compiler invocation. Default
`sail verify` does not use this fork; it runs the stock pinned Verus release.

Some upstream behaviour hurts the engine directly: output whose order varies
between runs (which defeats exact cached-versus-fresh comparison) and
structured output that is wrong or ambiguous. The fork fixes those issues
under a narrow policy (Sail ADR 0039). A fix may only:

- make stock's output deterministic by choosing one of the outputs stock can
  already produce; or
- correct structured output (`--output-json`, timing, logs) without changing
  verdicts or diagnostics.

Anything that would change a verdict needs its own Sail ADR. Each fix is
tracked as a `VI-NNNN` entry in Sail's `docs/verus-issues/`, with a
reproduction. Fixes are not proposed upstream.

## Branches and tags

- `upstream/<short sha>`: an unmodified upstream Verus commit used as a base,
  for example `upstream/426d8b0` (the current base) and `upstream/8dea4a2`.
- `sail` (default): the current upstream base, then, in order:
  1. one harness commit and one commit per fix, each fix with its regression
     test `source/rust_verify_test/tests/sail_vi<NNNN>_*.rs`;
  2. `feat: interactive repair engine`, the engine itself;
  3. fork documentation (this file, the README notice and CONTRIBUTING.md),
     which changes no source.
- `pinned/<short sha>`: tags every fork commit a Sail runtime pin has ever
  named, so it stays reachable after `sail` is rebased or a fix is dropped.
  Tags are never moved or deleted.
- `archive/*`: retained historical work.

Sail pins one full commit (currently `13a1176c`, tagged `pinned/13a1176c`),
not the branch. Documentation commits move the `sail` tip past the pinned
commit; that is expected and does not affect the pin.

## Divergences from upstream

`git log upstream/426d8b0..sail` (oldest first; source commits only):

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
| `13a1176c` | feat: interactive repair engine | (Sail ADR 0027) | Adds the engine Sail drives (`rust_verify/src/sail_engine.rs`, hooks in `driver.rs` and `verifier.rs`). Only active when Sail requests it; ordinary runs are unchanged. | Sail's engine differential checks |

Dropped fixes (no longer on `sail`; their commits stay in Sail's records):

- **VI-0005** (forwarded Verus arguments do not re-verify a fresh crate):
  binding forwarded arguments in dep-info made every Sail repair rebuild,
  because the repair passes the selected function as a forwarded argument.
- **VI-0007** (errors from different modules print in run-dependent order):
  a bucket-order fix was dropped because no deterministic order is provably
  one that stock produces, which the policy requires.

Not fixed: **VI-0011** (Cargo `no-vstd` metadata does not reach the launcher's
macro-mode selection). The fix is not small: the launcher would have to
decode Cargo-Verus's per-package driver arguments.

## Releases

Each pinned fork commit has prebuilt engine runtimes published as GitHub
prereleases here:

| Release tag | Platform | Sail pin |
|---|---|---|
| `engine-runtime-13a1176c-linux` | Linux x86-64 | `engine/verus/artifact-linux.json` |
| `engine-runtime-13a1176c-macos` | macOS arm64 (Apple silicon) | `engine/verus/artifact-macos.json` |

Each release has the archive `engine-runtime.tar.gz` and its checksum file
`engine-runtime.tar.gz.sha256`. The archive holds `sail-build.json` (the build
manifest: provenance and per-file digests) and
`verus/source/target-verus/release/` (`verus`, `cargo-verus`, `rust_verify`,
`z3`, `vstd`, builtin libraries). The runtimes are built and packed by Sail's
tooling on native hosts (`cargo xtask build-verus-engine`,
`ci/engine-artifact.py pack`) and published by a maintainer; this repository's
CI does not build them.

Sail's pin file names the release (`repo`, `release_tag`, `asset`) and records
the archive SHA-256, the fork and upstream commits, the source lock digest, the
Rust toolchain and the Z3 digest. Sail downloads anonymously and accepts the
runtime only if the archive digest and every file in the manifest match. To
check a download by hand:

```sh
sha256sum -c engine-runtime.tar.gz.sha256   # or: shasum -a 256 -c
```

Assets of a release a pin has named are never replaced or deleted.

## GitHub Actions

Upstream's workflows under `.github/workflows/` are kept unchanged so rebases
stay clean, but GitHub Actions is disabled for this repository, so none of them
(including the scheduled and release-publishing ones) run here.

## Upgrading to a new Verus

1. Push the new upstream commit as `upstream/<short sha>`.
2. On a review branch, rebase: `git rebase --onto upstream/<new> upstream/<old>`.
   Drop fixes the new release already contains; resolve conflicts per commit.
3. Run every `sail_vi*` regression test and the `rust_verify_test` suite, and
   record the results in Sail.
4. After review, move `sail` to the branch. In Sail, update the upstream and
   fork pins together, rebuild, qualify and publish new runtimes, re-pin them,
   and tag the pinned commit `pinned/<short sha>`.

## Licence

Verus and this fork are under the MIT licence in [LICENSE](LICENSE), which is
unchanged from upstream.
