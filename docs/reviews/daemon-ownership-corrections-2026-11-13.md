# Daemon ownership/dependency review corrections — 2026-11-13

## Status and scope

Local correction pass on candidate `49984680858d50da8ec424709b943c62a1bfe589`,
original baseline `effb18c2836df4ce2e5e88cd46876c685b0bb318`. Independent review
accepted the review deliverable, **not** the implementation candidate. Sender
review and native integration remain outstanding. No approval, integration,
push, publish, deployment or production changes were performed.

This report accompanies the correction commit returned in the handback. Exact
source/config/document changes from the old candidate are archived as
`daemon-ownership-evidence/correction-2026-11-13/corrections.patch.gz` (losslessly
compressed Git patch); generated
evidence is separate. Final source hashes, noncompiling check receipts, archive
hashes and exact unrun Rust selectors are in that directory. The original
47-test archive remains associated only with candidate `4998468`; see
`daemon-ownership-evidence/ARCHIVE.md`.

## 1. Packaging invocation contract

- Docker builds `medousa_daemon` + `medousa_cli` with explicit `full-daemon` and no
  defaults, copies both into `/build/packaged`, then builds the `medousa` launcher
  in a separate `tui` invocation. All three shipped binary names, entrypoint,
  exposed port, volume and runtime image are preserved.
- `CARGO_TARGET_DIR=/build/target` makes build outputs agree with staging paths.
  The repository otherwise sets `../.cache/cargo-target` in `.cargo/config.toml`.
- Docker's builder moves from Rust 1.85 to 1.100, the line used for prior local
  qualification. **Rust 1.85 does support edition 2024**; the actual compatibility
  problem is the locked dependency set, including `time 0.3.47` whose declared
  minimum is Rust 1.88. The Rust 1.85 pin and Docker target-directory mismatch
  both predate this candidate (present at `effb18c`). They are narrowly corrected
  to make the corrected invocation/output contract coherent, not presented as
  regressions introduced by the extraction. The missing `tui` selection for
  `medousa` became a failure with the new binary feature gate.
- The release workflow and Bash/PowerShell source-build helpers now preserve the
  standalone daemon immediately after its build, **before** launcher/adapter
  builds can overwrite the Cargo output. Later staging does not recopy an
  overwritten daemon. Bash also stages prebuilt daemon inputs immediately.
  Existing native-binary validation is retained; PowerShell CRLF is preserved.

`scripts/ci/test-daemon-packaging.py` is a noncompiling regression suite wired into
CI. It executes the actual Docker cargo-RUN shell bodies and release workflow
build/stage bodies (Linux, Windows suffix, Darwin target strings) with a fake
Cargo that enforces binary feature gates and deliberately overwrites daemon
output during terminal builds. It executes the actual Bash root-build/staging
section with fixture path/native-validator bindings, including the prebuilt
branch. Negative controls reject the old missing-tui invocation, reject a unified
daemon+tui invocation, and demonstrate that late daemon recopy is contaminated.
PowerShell preservation is checked **statically only**, not executed. These are
invocation/staging regressions, **not Docker builds, real release compilation,
native binary validation, Windows execution or cross-platform qualification**.

## 2. Source compatibility and caller audit

- `apps/medousa-home/MOBILE-DEV.md` install/run commands now select `--features tui`.
- Root README and mobile guidance explain daemon-default builds, terminal opt-in,
  explicit build/install/run examples, separate packaging invocations, artifact
  preservation and the configured Cargo output directory.
- Audited tracked source-build callers across scripts, Docker, workflows and
  documentation, including multiline Bash/PowerShell commands. The mobile wrapper
  already selects `tui`; the CI terminal check already does too. Release workflow
  and helper callers select it separately and now preserve the daemon first.
  `CONTRIBUTING.md`, cookbook daemon instructions and Home sidecar preparation
  select `medousa_daemon` and remain compatible. Schema generators, examples,
  adapters, browser and local-inference builds do not select the gated terminal
  launcher. Dynamic helper arrays were inspected, not assumed safe from a grep.
  The saved caller inventory records the audit; this is not an exhaustive claim
  about external or unpublished build consumers.

## 3. Actual platform lifetime regression sources

`src/runtime/platform/lifetime_tests.rs` adds three tests against the actual
`build_daemon_platform` constructor, real in-memory Stasis composition, real agent
assembly, scheduler attachment and actual registered recurring/durable handlers:

1. `retained_agent_survives_final_platform_drop_without_observation_failure`
   retains the actual `agent_handle()` Arc, checks pointer identity, drops a
   nonfinal and then the final actual platform Arc, waits for receiver closure
   while agent/tool producers remain live, and invokes the actual registered
   `cognition_calendar_query` / `calendar.list` tool before and after final drop.
   The calendar tool emits the real `ToolInvoked` observation; its successful
   result must remain unchanged after receiver closure.
2. `late_actual_constructor_failure_closes_observations_after_handler_registration`
   reaches the real post-handler checkpoint, injects an error, retains the actual
   registered agent Arc/sender, then requires eventual receiver closure and a
   successful observation-producing calendar operation.
3. `polled_actual_constructor_cancellation_closes_observations_after_handler_registration`
   polls that same actual constructor until the checkpoint signals real handler
   registration, pauses it there, drops the polled future and asserts the same
   closure/operation properties. The pending future exists **inside the real
   late constructor hook**; it is not an unpolled synthetic stand-in for constructor
   qualification.

The controlled checkpoint/sender capture is entirely `cfg(test)`, task-local,
with no production authority change, no test config field added to public
construction configs, and no global production failpoint. It is after actual
turn-worker registration and before reconciliation. Fixture roots are temporary,
the existing test environment lock is held, MCP is directed at an invalid local
port, provider configuration does not request inference, and remote execution is
refused by a fixture transport.

These constructors touch process-global stores/registrations. The three tests
are explicitly ignored by the general parallel lib suite and are **required in
CI by `bash scripts/ci/test-platform-lifetime.sh`**, one exact selector per fresh
process using `--ignored --exact --test-threads=1`. The runner rejects zero-test
matches. A noncompiling fake-Cargo check verifies its three invocations and
zero-match refusal. That runner check is **not execution of the Rust tests**.

Drain Drop requests **asynchronous** Tokio task abortion. Receiver closure needs
runtime progress; it is not synchronous with Drop. It does **not** stop durable
workers or all background tasks. The production ownership mechanism is unchanged;
comments now state this boundary explicitly. The tests do not establish complete
platform shutdown, durable-worker cleanup, persistent-backend behavior or absence
of other Arc cycles.

## Fresh checks actually run

- **9 Python regression tests passed** (including static PowerShell ordering and
  fake-Cargo runner checks); subprocesses perform no Rust compilation.
- Offline real Cargo **dependency graph** guard passed: default and explicit
  daemon each 803 unique package names, daemon+Iroh 881, terminal positive control,
  and retained Grapheme implementation/signature/AOT/WASIX capability closure.
- Targeted `rustfmt --edition 2024 --check` passed, parsing the new child test
  module too. This is syntax/format validation, **not Rust typechecking**.
- Bash syntax checks passed for the isolated runner, release helper and mobile
  wrapper; Ruby YAML parsing passed for CI/release workflows; strict docs check
  passed; Python AST and PowerShell CRLF checks passed; final diff whitespace
  checks passed. Exact receipts/logs are saved separately.
- Archive hashes were checked against Git objects for `4998468`; original
  fingerprints were verified against that candidate, **not relabelled current**.
  Final corrected source fingerprints and source diff were captured separately.

The initial free space was approximately 6.38 GiB against the 6 GiB hard floor.
Space later recovered externally (approximately 24 GiB observed); this pass did
not delete caches/worktrees or claim that recovery as its work. No Rust compile,
link or new Rust test execution was attempted, respecting the explicit
no-expensive-compilation instruction. **The three new Rust tests are uncompiled
and unrun; fresh Rust type/test qualification remains a blocker to accepting
this correction candidate.** Exact selectors/commands are saved in
`unrun-rust-tests.json`. Prior executables cannot qualify these changed sources.

No Docker image build/run, full release packaging run, PowerShell execution
(`pwsh` unavailable), current-source daemon/TUI check, Clippy/full workspace tests,
native/GUI, WASIX/AOT runtime qualification, Iroh compilation, benchmarks or
production certification is claimed.

## Dependency/accounting boundaries retained

No manifest or lockfile capability reduction was added in this pass. Independent
review's extraction equivalence and **955 -> 917 package-version pairs** remain
old-candidate graph evidence; the manifest/lock/graph guard are unchanged, and
fresh current-source graph checks preserve that boundary. There is **no effective
Grapheme slimming claim**. Crossterm 0.29 remains via Grapheme data -> Polars fmt ->
comfy-table tty; direct terminal Crossterm 0.28/Ratatui are gated. Supported data,
media, editor, AOT and WASIX capabilities remain. Upstream Grapheme work, repository
split, Forge/sync changes and unrelated cleanup remain excluded.

**Handback: local committed corrections and bounded evidence only; sender retains
independent review and native integration responsibility.**
