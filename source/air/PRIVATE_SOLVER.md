# In-memory solver-child configuration

Status: an unselected source component for Sail issue435. It does not close the
issue or install a compiler-control transport. Normal command entrypoints still
use the ordinary constructors, with no private configuration.

`Verifier::new_with_private_solver` retains an owned `PrivateSolverLaunch`
separately from `ArgsX`. Every `Verifier::from_self` worker copies the same
settings. Actual AIR context construction carries them to its lazy solver
launch, and `SmtProcess::launch_with_private_configuration` uses the resulting
owned `std::process::Command` at the existing spawn/read/write/wait boundary.

The semantic contract is narrow:

- With no private configuration, the existing SolverInfo environment/PATH
  resolver is called once. Z3 and Cvc5 receive their unchanged argument lists.
- An explicit executable retains native OS bytes and bypasses that resolver.
  Ordered overrides/removals apply only to the new solver-child Command.
  Empty values and absent values remain distinct. No process-global setter,
  compiler Command mutation, public argument flag or new selector is involved.
- Solver selection, SMT encoding, resource policy, diagnostics, writer/reader
  behavior, transcript bytes, original child ownership and error handling stay
  on their existing paths. Configuration neither authenticates a process nor
  proves executable identity or observation completeness.

Three ordinary unit regressions inspect actual Command construction without
starting a child or sampling the caller environment: normal resolver invocation
and both solver argument lists, private resolver bypass plus ordered environment
operations/unchanged compiler Command, and Unix non-Unicode path/value retention.
At frozen `8b127ceb`, the complete ordinary native release library gates pass:
`cargo test --release -p air --lib` runs 147 passing tests, including all three
new Command-construction tests; `cargo test --release -p rust_verify --lib`
runs 18 passing tests. Neither scope ignores or filters tests. Actual Verifier
construction, worker copies and AIR producer code compile. AIR tests take
6.57 seconds (38.817 seconds including build/toolchain work); Verifier tests
take 0.01 seconds (57.922 seconds including build/toolchain work). The full
qualifier takes 122.222 seconds on native x86-64 Linux with the pinned 1.99.1
toolchain and Z3. Final tracked source, image, Z3, rustc and Cargo guards pass.

This is library/build evidence for the unselected component. Complete runtime
and vstd builds, full fork regressions, corpus verdict/diagnostic/query-byte
parity, macOS and determinism remain separate. No compiler consumer/provider
experiment or actual private configuration installation is exercised.

Raw evidence: `experiments/2026-10-27/acceptance-private-solver-8b127ceb-libraries-native.tgz`,
29,739,961 bytes, SHA256
`fbdd544008f8cef638ea4acff057c13aba66cf37160bf9a5e0e4b0a34bde744f`;
authenticated byte-identical PUT/GET.

The missing join remains substantial. Current Sail sets solver-route variables
before Cargo, changes its wrapper environment and augments physical driver
settings. A real installer must receive private configuration for the actual
original Cargo-owned per-unit launch, consume it before compiler plugins and
keep caller-original environment/arguments. Final-child stripping cannot repair
earlier build-script observations. A matching file/offer, UID or PID text is not
that join. No entrypoint installs from one, and this component selects no provider,
new runtime or release. Complete runtime/fork qualification, conformance, query-byte equality, macOS
and cost evidence remain required before selection.
