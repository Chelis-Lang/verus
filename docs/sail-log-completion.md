# Paired log completion experiment

Status: isolated source experiment on accepted baseline `73dea021`; not built,
qualified, selected, pinned or published. There is no selected Sail consumer or runtime change.
The logging-only change is independently ported from combined candidate `8a373f81`;
no closure, shipped-vstd or proof-note change is imported into this baseline.
The old markerless actual-work attempt remains failed. Its ordinary timing
samples describe the original runtime, not this instrumentation.

## Observation and scope

A completed real Vest run produced 725 transcript paths, including 94 zero-byte
files. Its 631 nonempty transcripts matched 631 solver-session records. Verus
creates the files before executing the bucket; it starts a solver lazily. A
zero-byte file alone cannot distinguish a normal no-process context from lost
or interrupted capture. The actual-work observer correctly refuses it today.
The retained AIR/transcript pair and the reduced observer fixture are preserved
with the external validation evidence. The reduced fixture is not a fresh
minimal Verus execution.

This experiment supplies an explicit producer-owned completion witness. It
changes log bytes only when both initial AIR and SMT transcript logs were
requested. It does not add a flag, alter a solver option, transmit a command,
flush buffered commands, start a process or wait for a pending response.
Default logging-disabled execution and independently requested single logs
retain their previous log format. A lone writer does not receive a footer and
cannot qualify a pair.

## Exact version 1 encoding

After successfully completing a context, append exactly one of these ASCII
lines to each writer, then flush that writer:

```
;; SAIL-COMPLETE 1 air zero-process
;; SAIL-COMPLETE 1 transcript zero-process
```

or:

```
;; SAIL-COMPLETE 1 air process-used
;; SAIL-COMPLETE 1 transcript process-used
```

Each line includes one LF. The transcript footer is written first. A zero
context has no transcript bytes before its footer. A used context has launched
a process; every outstanding `CommandsHandle` must have returned its response
before the footer is written. This is actual producer state, not an inference
from a missing AIR query. Evaluation/version queries can also launch a process.

Completion consumes `Context`, preventing later commands or a second footer.
There is no Drop completion. A missing pair, early error, cancellation or panic
cannot accidentally acquire a completion witness through destruction. Writer
write and flush errors are returned through `io_vir_err`; a partially written
pair is incomplete evidence. Existing process destruction still closes its
request sender and reaps its child at the same owner boundary as before.

`Emitter::complete_log` touches only its log writer, not `pipe_buffer`.
`SmtProcess::complete_transcript_log` checks the existing response-channel
ownership state before log I/O. Neither finalizer calls a solver/send/wait API.
Previously buffered trailing options are still discarded at context destruction.

## Ownership and early-exit inventory

All production contexts that receive these two log writers originate in
`Verifier::new_air_context_with_prelude`. Its only callers are the two owners:

- `verify_bucket`: creates the shared context; context-generation `?`, operation
  generation `?`, and recommends retry `?` return without completion. The
  operation-loop `break` is its normal end. Time and rlimit totals are read
  before consuming the context immediately before `Ok(VerifyBucketOut)`.
- `new_air_context_with_bucket_context`: returns a spinoff to the query-loop
  owner. Nonlinear, bit-vector, profile rerun and `spinoff-all` use that owner.
  Its one `continue` (skipped recommends) precedes context creation. Completion
  occurs at the iteration's original destruction boundary, after command,
  metrics, rlimit restoration and profile handling. Construction/profile-path
  `?` returns remain unmarked. Recommends and expansion retries use the same
  iteration boundary. The closure-local `return format!(...)` only formats a
  profile description; it does not leave the context owner.

The standalone AIR CLI also installs a transcript writer, but has no initial
AIR log option and does not call paired completion. Its single-log format and
normal execution remain unchanged. This experiment makes no completion claim
for its transcripts.

`run_commands_queries` can return early for an unselected function. That still
completes its created context; no process is started just to produce a marker.
Singular uses its existing separate path; only any AIR context actually owned
by these two boundaries can acquire the pair. An invalid/timeout verdict may
finish its logs normally, but completion never means verification success.
The observer must separately require its successful complete Current result.

## Tests and remaining gates

Direct AIR test sources cover no-process pairs, no footer I/O with disabled or
single logging, no Drop completion, both writers' write/flush failures, pending
response refusal before I/O, and unchanged pending solver bytes. Verus regression
test source combines a spec-only bucket, ordinary proof and bit-vector proof,
with and without `spinoff-all`; it requires both mode witnesses, unique paired
EOF markers and unchanged single-log format. These tests have not been compiled
or run for this source experiment. No runtime acceptance is claimed.

Before qualification, run these tests against a real source-bound build and
retain raw solver stdin/session input hashes plus verdict/diagnostic/exit
comparisons for logging disabled/enabled, selected-out, nonlinear, bit-vector,
recommends, expansion, profiling and error paths. Missing/failed capture remains
a refusal. Both historical comparison arms need separately bound instrumented
builds; this source patch does not qualify an old binary or retrofit old logs.

## Source-only grammar helper and consumer draft

`source/air/examples/sail_no_query.rs` reuses genuine SISE and AIR parsing,
constructs no Context and starts no solver. It accepts at most 8 MiB of UTF8 AIR
and 256 levels including its wrapper, checks EOF and balanced push/pop scopes,
and rejects every query command. Successful stdout is exactly
`SAIL-AIR-NO-QUERY 1` followed by LF. It validates the genuine command grammar,
not semantic typing or proof validity. Direct test sources cover recognized
commands, malformed declarations/expressions, query forms, scope errors,
invalid UTF8 and size/depth bounds; they have not been run.

A separate explicit opt-in Sail observer draft can bind this helper's actual
source and executable, bound each process/output, and retain zero-context log
hashes. Neither that draft nor this helper is built, qualified or selected.
The default observer must continue to refuse all old incomplete evidence.


A future consumer must require a uniquely placed EOF marker in each paired
file, matching version and mode, with no duplicate marker or trailing bytes.
Used mode must retain all existing complete-batch, response, AIR/SMT query,
input-hash and session-multiplicity checks. Zero mode additionally requires an
empty transcript payload, a complete structural AIR parse with an explicit
recognized command grammar excluding all query forms, and no empty-input
session. The existing line-prefix `air_queries` extractor is not such a parser.
No zero-mode admission is implemented here. Exact grammar admission and its
negative controls require separate review before that change.

The paired footer is not cryptographic authentication. Current run identity,
complete raw capture, immutable source/runtime identities and per-member hashes
remain necessary. Coordinated post-capture rewriting is not solved by comments.
