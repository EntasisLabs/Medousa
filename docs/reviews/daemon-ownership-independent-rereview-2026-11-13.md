# Independent correction re-review — ownership and daemon dependencies

## Verdict: CHANGES REQUESTED for the exact candidate

Reviewed implementation source: **f44c405c3e91bbe160ed148b34265beffff25a1b**, direct
parent **49984680858d50da8ec424709b943c62a1bfe589**; full baseline
**effb18c2836df4ce2e5e88cd46876c685b0bb318**. No approval, integration, push,
publish, deployment or production changes. This handback changes review evidence
only; sender retains final review, correction orchestration and integration.

Packaging corrections are supported. The three actual-constructor regressions
compile and **pass 3/3 in a precisely qualified environment**. However, the
committed runner fails in this host's default environment. Two new harness
portability defects need correction and independent requalification before
accepting the exact candidate. These are test/verification defects, not evidence
of a new production lifecycle or authority regression.

## Findings

### P2 — runner overflows its default test-thread stack

Locations: `scripts/ci/test-platform-lifetime.sh:7-17`,
`src/runtime/platform/lifetime_tests.rs:186-247`.

`CARGO_BUILD_JOBS=2 MEDOUSA_TEST_HERMETIC=1 bash scripts/ci/test-platform-lifetime.sh`
compiles successfully, executes exactly one retained-agent test, then aborts with
`has overflowed its stack` / `fatal runtime error: stack overflow`. Cargo exits
101. The script stops before its next two selectors. Each other constructor
selector also aborts separately on the default stack (signal 6). A canonical
temporary directory alone does not fix the stack overflow.

With **RUST_MIN_STACK=33554432**, unchanged source gets past stack exhaustion.
Please make the isolated runner's stack requirement explicit and bounded, or
change the test harness to avoid oversized inline futures. Do not weaken the
assertions or silently remove these tests from CI. This is a reproduced macOS
host defect; no Linux/Windows runtime result is inferred.

### P2 — fixture temporary path violates the vault's no-follow contract on macOS

Location: `src/runtime/platform/lifetime_tests.rs:83-86`, failing call at line 165.

With the larger stack alone, **each of the three exact tests fails** at its real
calendar call with `PortFailure("open vault root /var/folders/.../vault")`.
The fixture installs the raw `tempfile::tempdir()` path as the vault override;
macOS has `/var -> private/var`. `StoreRoot::open_or_create_nofollow` intentionally
rejects symlink components. This is **not a missing-directory bug**: that API
creates directories. The earlier provisional missing-mkdir diagnosis is
superseded by source inspection and the controlled environment probes.

Using **TMPDIR=/private/tmp** plus the larger stack makes **all three tests pass**
unchanged, one exact nonzero selector per Cargo/test process. Fix the fixture to
use a canonical trusted temp root (while retaining the TempDir owner), or supply
an equivalently portable canonical temporary directory in the runner. Preserve
production no-follow confinement. Creation alone is not the needed correction.

The retained-agent test fails on its pre-final-drop calendar call in the
stack-only environment, so that run does not qualify final-drop behavior. The
failure/cancellation tests reach and pass receiver closure before their calendar
error. Only the combined canonical-path/larger-stack run qualifies all assertions.

## Closure of prior required corrections and source review

- **Docker** now explicitly builds standalone `medousa_daemon` and `medousa_cli`
  with `full-daemon`, preserves them in `/build/packaged` before compiling the
  `tui` launcher, and copies only staged artifacts. `CARGO_TARGET_DIR` overrides
  the repository-relative cache path. All three image binary interfaces remain.
- **Release workflow, Bash and PowerShell** preserve the standalone daemon
  before any terminal/adapter build can overwrite Cargo output, and avoid a late
  daemon recopy. Bash's prebuilt-daemon path is also preserved early. The fake
  Cargo adversary deliberately overwrites the output during the terminal build;
  the committed staging commands still package the standalone marker. This is
  executable packaging-command qualification, **not a native binary or Docker
  build**. PowerShell ordering was inspected statically; no `pwsh` is installed.
- **Caller guidance** states default daemon / explicit `tui` launchers; stale
  mobile install/run guidance opts in. Binary gates and compatibility names are
  consistent. No remaining missing-`tui` launcher invocation found in the scoped
  callers reviewed.
- **Actual construction** is used, not an empty/synthetic future. Test hooks
  capture the real agent and registered sender after handler registration;
  failure and pause occur after real registration. Hooks, thread-local action
  and their call site are `cfg(test)`; production constructor authority is not
  expanded. Public construction remains the same ownership path.
- **Cancellation** actually drops the polled constructor. `tokio::pin!(future)`
  moves the owned future into its inner scope; leaving that scope destroys the
  owner, not merely a borrowed `Pin<&mut _>`. The combined-environment passing
  run proves the observation receiver closes while the captured real agent and
  sender remain live. Nonfinal platform drop keeps it open; final drop closes it.
- **Retained agent and producer behavior** use the agent's actual
  `CognitionCalendarListTool` handler, invoking real `calendar.list` with a
  fixture vault. Public tool output is compared before/after final platform drop;
  live producers survive receiver shutdown. This tests retained-agent/producer
  semantics, not remote authorization or HTTP admission, which it does not bypass
  in production. Request-boundary and tool-contract suites pass freshly too.
- **Fixture and isolation**: environment lock, scoped data directory, temporary
  product/vault state and installation identity, loopback port-zero MCP endpoint,
  and restoration of mutated environment variables constrain the tests. Vault
  override is thread-local on the current-thread runtime. Process-global handler
  and store registries are isolated by **separate processes**, not just
  `--test-threads=1`. Ignored attributes keep these out of the general lib suite;
  CI invokes the isolated runner before the ordinary hermetic suite. Exact
  selectors and a one-pass summary check fail closed on zero matches. These
  safeguards are sound; the two environment defaults above are incomplete.
- Revisited original settings/assembly extraction, compatibility re-exports,
  ownership endpoints and affected consumers against the full baseline and
  correction delta. No new production authority regression found. No unrelated
  refactoring or implementation correction was made during this review.

## Fresh qualification of corrected sources (not old binary reuse)

Toolchain: **rustc 1.100.0 (0ef1bc386 2026-11-09)**, commit
`0ef1bc386059596dada8fb65b50425aab2ea637a`, LLVM 23.1.5;
**cargo 1.100.0 (3acc79c0c 2026-11-05)**; stable aarch64-apple-darwin,
Darwin 27.0.0 arm64. Existing Medousa cache from `.cargo/config.toml` was inspected
and reused: `../.cache/cargo-target`. No foreign build artifacts were used or
removed. No cache/worktree deletion.

All Rust work serialized, **CARGO_BUILD_JOBS=2**. The monitor sampled free bytes
every two seconds and would interrupt the whole process group below **8 GiB**,
leaving margin above the **6 GiB hard floor**. No stop was necessary. Lowest
recorded during monitored Rust work was **16.8329 GiB**; final free space remained
over 16 GiB. This is incremental compilation with existing dependencies, not a
clean-room rebuild.

Fresh commands and outcomes:

1. `bash scripts/ci/test-platform-lifetime.sh` under the default test environment:
   **compile PASS, runner FAIL 101** as above. It compiled the corrected lib test
   executable in about 3 minutes. All three default-stack selectors were then
   executed independently; each aborted. Nonzero matches are in saved logs.
2. `RUST_MIN_STACK=33554432 bash scripts/ci/test-platform-lifetime.sh`:
   first test fails on vault path; remaining exact selectors executed separately
   have the same failure. **0/3 passing**, not a green runner.
3. `RUST_MIN_STACK=33554432 TMPDIR=/private/tmp bash scripts/ci/test-platform-lifetime.sh`:
   **PASS, 1/1 + 1/1 + 1/1**, separate processes, unchanged corrected source.
   Each summary has zero failed and zero ignored tests. This qualifies real
   constructor semantics **under those two explicit environment prerequisites**.
4. `cargo check --locked --offline -p medousa --no-default-features --features full-daemon --bin medousa_daemon`:
   **PASS**, 1m08s.
5. `cargo check --locked --offline -p medousa --no-default-features --features tui --bin medousa_tui --bin medousa`:
   **PASS**, 57.75s.
6. Fresh corrected-source executable directly, each original selector with
   `MEDOUSA_TEST_HERMETIC=1` and `--test-threads=1`: **47/47 pass**.
   Counts: drain 4, identity 2, settings 5, tool contract 4, spine parity 7,
   host catalog 4, Grapheme runtime 11, recovery 1, request boundary 9.
   Exact selectors, command vectors and summaries are in
   `current-test-receipts.json`; no zero-test selector is counted.

Fresh executable SHA-256:
`afe8909be1ebbaea71278740ff40ab48eb105b0bb8ffc946461c51b9f866747c`.
`source-fingerprints.json` independently records **1082** source/build-file hashes,
rechecked unchanged after all qualification. These fresh results supersede old
47-test evidence for this corrected source only; historical logs are not relabeled.

Noncompiling checks independently rerun: **9/9 Python packaging tests**, daemon
feature/dependency guard, runtime-context ownership guard, strict docs, Rust
formatting for platform/event delivery (including lifetime submodule), Bash
syntax, workflow YAML syntax, and `git diff --check effb18c f44c405` all pass.
Correction verifier passes **40 current source fingerprints, 37 immutable old
candidate files, 29 old-source fingerprints against 4998468**. Its printed
uncompiled/unrun disclaimer describes the archived pre-review qualification,
not the fresh evidence in this report.

## Docker toolchain and dependency boundary

`docker manifest inspect rust:1.100-bookworm` returned **0**, an OCI image index;
raw manifest retained. This establishes tag availability, **not an image build**.
`cargo metadata --locked --offline --format-version 1` inspected actual locked
package manifests: maximum declared Rust minimum is **1.93** (Cranelift/Wasmtime
and related packages). Thus 1.100 clears declared constraints; 1.85 supports
edition 2024 but does **not** meet this locked dependency graph. No false
edition-2024 assertion is retained. Declared minima are not a Linux ABI or native
build certification. Existing warning limits: redundant homepage, future
incompatibility in proc-macro-error2, and macOS test linker large unwind section.

Dependency guard independently reports **803** unique package names for default
and explicit daemon, **881** for release daemon+iroh; terminal positive control
is present. Shared Crossterm remains via comfy-table. Grapheme implementations,
signatures, AOT and WASIX closure are retained. **Effective Grapheme slimming is
not done; no terminal-free transitive graph or measured speedup is claimed.**

## Remaining limits and next action

Return the two harness corrections to the sender, then independently rerun the
committed runner and relevant focused checks. Do not accept or integrate this
exact candidate on the strength of environment-qualified diagnostic passes alone.
No production lifecycle failure was found in the passing qualified tests.

Not run: Docker build/image execution, native PowerShell release, release linking,
Linux/Windows execution, full workspace suite/Clippy, live daemon/GUI/mobile,
AOT/WASIX roundtrips or full release qualification. No claim that simulations
validate native packaging. Forge/sync/repository split and broader migration
remain outside scope. All implementation source remains exactly f44c405.

## Preserved evidence

Raw commands, stdout/stderr, return codes, toolchain, source hashes, executable
hash, disk monitor receipts, Docker manifest and dependency Rust minima:
`docs/reviews/daemon-ownership-evidence/independent-f44c405/evidence.tar.gz`.
Archive SHA-256:
`57915460bbebb20444581c9ab482a0043d611bd1a59d7511bb6213041f42b37d`.
The archive has an internal per-file SHA-256 manifest. Run the noncompiling audit:

```sh
python3 docs/reviews/daemon-ownership-evidence/independent-f44c405/verify.py
```

The auditor verifies integrity and expected pass/fail observations; its success
is **not** acceptance of the candidate or an additional Rust execution.
