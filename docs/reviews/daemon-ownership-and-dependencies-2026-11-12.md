# Daemon/shared runtime ownership and dependency boundary

Date: 2026-11-12. First bounded pre-migration slice; not integration acceptance.

## Scope and source context

Started at exactly `effb18c2836df4ce2e5e88cd46876c685b0bb318` from
`medousa/runtime-ui-fixes`, in the admitted isolated native worktree. Read both
uncommitted sibling audit reports located with `git worktree list`:
`daemon-audit-architecture-2026-11-12.md` and
`daemon-audit-performance-2026-11-12.md`. The sibling checkout and reports were
not edited or committed here.

The architecture report's runtime/launch citations (`src/tools.rs:2829-2855`,
`src/agent_runtime/runtime.rs:3-19`, `src/runtime/platform.rs:25-74,110-157`,
`src/bin/medousa_daemon.rs:493-551,1077-1094,1339-1355` at the baseline) motivated
this extraction. The performance report distinguishes lazy runtime construction
from compiled feature cost; this change does not relabel either as a measured
startup or delegation improvement.

No Forge decomposition, local/remote synchronization optimization, store
consolidation, durable-format migration, admission changes, deployment or
production-state access is included.

## Concrete ownership

- `agent_runtime::runtime::MedousaAgentRuntime` is the concrete shared service
  bundle. `tools::TuiRuntime` is now its compatibility alias, not the other way
  around. The existing Arc-shared stores, registries, worker scheduler, Forge
  authority slot and destination project-setup service are preserved.
- `runtime::agent_services` owns construction/assembly. Shared consumers import
  it directly. `tui::runtime_services` is only a compatibility facade for the
  terminal platform adapter.
- `runtime::settings` owns runtime settings and validation; terminal settings
  re-export them. Shared runtime/workshop consumers no longer import `tui`.
- `events::RuntimeEvent` owns the existing best-effort observation contract;
  `TuiEvent` is a compatibility name for that same type. Variants, payloads,
  channel capacity and delivery semantics have not been deleted or redesigned.
- `MedousaPlatformRuntime` owns the headless observation drain. Legacy headless
  agent factories retain their drain on the runtime instead. An RAII owner aborts
  only that drain on failed/cancelled construction or owner destruction, including
  when producers retain sender clones. Channel-backed clients supply their own
  sender and do not acquire an unnecessary drain.

This is **not** a claim that all process background tasks now have one shutdown
owner. The launcher remains the process composition/recovery/shutdown owner.
Its ordering is untouched: install world/document/Forge/destination authority
before dependent recovery, then serve; on shutdown signal background work, wait
for coordination, drain the session writer, flush workspace persistence and
remove the embedded-store lock. The observation drain is not a journal, outbox,
turn acknowledgement or execution cancellation mechanism.

## Build profiles and feature unification

Default features remain `full-daemon`, but that profile no longer activates
terminal dependencies. Terminal presentation is explicit:

```sh
# Actual isolated daemon target; no workspace selection or terminal feature.
cargo check --locked -p medousa --no-default-features --features full-daemon --bin medousa_daemon
# Iroh-enabled packaged daemon.
cargo build --locked --release -p medousa --no-default-features --features full-daemon,iroh-transport --bin medousa_daemon
# Supported TUI and terminal launcher.
cargo check --locked -p medousa --no-default-features --features tui --bin medousa_tui --bin medousa
cargo run --features tui --bin medousa_tui
```

The `medousa` launcher also requires `tui` because its onboarding uses terminal
presentation. A default `cargo build` no longer builds these two opt-in bins;
`cargo build --features tui` restores the previous terminal targets. This is a
build-interface change, not removal of the supported terminal functionality.

Cargo unifies features per package/build invocation, **not per selected binary**.
Building daemon and TUI together with `--features tui`, `--all-features`, or a
workspace that enables terminal features defeats the boundary. Release Bash,
PowerShell and the standalone-daemon GitHub workflow now build the daemon in a
separate invocation, then build terminal targets. The mobile development launcher
requests `tui`. Home's daemon-only sidecar invocation remains a standalone daemon
build. CI checks the isolated daemon before explicitly checking/linting the TUI;
the hermetic suite explicitly enables `tui` so existing terminal tests are not
silently dropped from coverage.

`scripts/ci/check-daemon-dependencies.py` checks default, explicit daemon and
Iroh-enabled daemon dependency graphs, has terminal positive/negative controls,
checks bin feature requirements and forbids reverse shared-to-TUI imports. It also
checks the effective Grapheme implementation/signature feature closure, not just
the presence of package names. It
uses normal/build edges, not dev-dependency or workspace-unified metadata.

## Grapheme capability accounting and remaining dependencies

The blanket `stasis-rs/grapheme-full` selector is replaced with
`stasis-rs/grapheme-host` plus an explicit, audited SDK capability list:

| Retained selection | Supported behavior / reason |
| --- | --- |
| `host` | Existing HTTP, web, network, SQL, Surreal, email and transforms modules used through the admitted host allowlist. |
| `data` | Existing CSV/data-frame execution; currently brings Polars and its formatting dependency. |
| `pdf`, `image`, `plot`, `media` | Existing optional standard-library modules remain executable, not just advertised by signatures. |
| `stage-b` | Existing native AOT compilation/execution path in `grapheme_workshop`. |
| `wasix-runtime` | Existing WASIX module hot-loading through `grapheme_runtime`; also implies stage B upstream. |
| Compiler/runtime/WASM, LSP | Source compilation, AOT/interpreter execution, the Stasis workflow adapter and supported editor language APIs remain. The LSP dependency itself still enables compiler full signatures upstream. |

There is **no measured or asserted current effective Grapheme dependency
reduction** from replacing the blanket selector. The selected upstream bundles
back supported capabilities. Removing them merely because the default allowlist
does not use every module would silently make previously supported configurations
stop working. A future smaller capability profile needs explicit product-level
selection/reporting and module/compile/runtime parity qualification. No such
removal is hidden in this slice.

**Important remaining terminal-related crate:** daemon-only builds retain
`crossterm 0.29.0` through
`Grapheme data -> grapheme-stdlib -> Polars fmt -> comfy-table tty`.
`grapheme-stdlib 0.7.1` requests Polars `csv,fmt`; Polars activates
`comfy-table/tty`. Cargo features are additive, so the application cannot turn
that upstream feature off. The guard permits only the immediate
`comfy-table -> crossterm 0.29.0` edge, not a new direct dependency or the TUI's
`crossterm 0.28.1`. Fully eliminating this remaining crate without dropping data
support requires an upstream packaging change or a reviewed dependency patch;
none is silently vendored here. See `daemon-ownership-evidence/retained-crossterm.txt`.

`image` remains shared by supported daemon image/vision/Grapheme behavior; it is
not a TUI-only dependency. Stasis dashboard stays because existing daemon routes
use it. Multiple logical store adapters continue sharing the original runtime
composition; no second runtime, scheduler or database authority is introduced.

## Evidence and limitations

`daemon-ownership-evidence/` records baseline and candidate normal/build trees,
feature trees (losslessly archived as `.txt.gz`), sorted package-version inventories,
selected and effective feature inventories,
exact graph hashes, the retained inverse dependency path, check logs and bounded
focused-test receipts. Baseline trees were captured before editing the manifest
with the same standalone selection:

```sh
cargo tree --locked --offline -p medousa --no-default-features --features full-daemon -e normal,build --prefix none
cargo tree --locked --offline -p medousa --no-default-features --features full-daemon -e features
```

Host graph: **955 -> 917 distinct package-version pairs** (**835 -> 803 package
names**), **38 removed, zero added**. The removed set includes every Ratatui crate,
`ratatui-image`, direct Crossterm 0.28.1 and terminal-only transitive dependencies.
These are resolved dependency counts, **not compilation-unit counts, linked
binary-size reductions, speedups or runtime-memory measurements**.

Qualification uses installed Rust/Cargo 1.100 and the checked-in current lockfile,
`--locked --offline`, two build jobs and no incremental test compilation. No
older-toolchain mismatch is presumed. Check durations are cached local command
observations only; they are not controlled before/after build-cost benchmarks.

## Completed local qualification

- Standalone daemon check: passed, explicit `full-daemon`, no default features.
- TUI and terminal launcher check: passed, explicit `tui`, no default features.
- Daemon library/integration-test typecheck (`--lib --tests`): passed.
- Dependency/feature guard: passed for default daemon, explicit daemon and
  daemon+Iroh; terminal positive/negative controls and Grapheme capability closure
  passed. The Iroh graph has 881 distinct package names on this host.
- **47 actually executed focused tests passed:** four observation-drain lifecycle,
  two runtime/event compatibility identity, five shared settings, four unchanged
  tool-contract parity, seven turn-spine byte-parity, four Grapheme host-catalog,
  eleven Grapheme admission/cancellation/receipt-ordering, one durable recovery,
  and nine HTTP request-boundary tests. See `test-summary.json` and the logs.
- Bash syntax checks and Ruby YAML parsing passed. Strict documentation validation
  passed. `git diff --check` passed. Changed Rust sources pass targeted rustfmt,
  excluding inherited unsorted declarations in `full_daemon.rs`; no whole-workspace
  formatting pass is claimed or unrelated baseline sorting committed.
- The existing `rg`-based runtime-context script cannot be qualified in this
  environment: `rg` is unavailable and that script misleadingly exits zero on a
  missing command. Its exit was **not** counted as a pass. An equivalent Python
  regex scan across the same Rust roots passed, recorded separately.

The first focused test compilation/execution took **306.68 seconds**, from
12,114,182,144 free bytes down to a sampled minimum 6,700,494,848 bytes. The next two
cached groups passed. The subsequent Cargo invocation was stopped at the **6 GiB
free-space safety floor** (6,396,157,952 bytes sampled); its exit -15 is recorded,
not called a test failure or a test pass. No broad cache/worktree cleanup was
performed. The remaining tests ran directly from the existing qualified test
executable without invoking a compiler; its SHA-256 is saved in
`existing-receipts.json`. One portable-FSM root selector matched zero tests
(the implementation is a crate re-export); that probe is not counted.

Build artifacts occupy approximately 8.2 GiB in the configured Cargo target cache
after this work. This is a local footprint observation, not attribution of all
cache contents to this slice and not a before/after savings measurement.

### Explicit untested gaps / limits

No release binary link/size measurement, controlled clean-build comparison,
startup/RSS/latency benchmark, live daemon/GUI/network integration, native WASIX
hot-load/AOT roundtrip, persistent/remote-store restart matrix, Windows/Linux
local execution, complete workspace Clippy/test suite, or TUI unit-test execution
was performed. Iroh-enabled dependencies were graph-checked, not newly compiled.
Further expensive compilation was stopped at the disk safety floor. Existing
manifest homepage and upstream future-incompatibility warnings were not hidden.

All supported heavy Grapheme capabilities remain; this slice makes their
selection explicit but does **not** claim an effective Grapheme footprint win.
Complete removal of the remaining shared Crossterm edge is blocked on upstream
Polars/Grapheme feature packaging (or an explicit supported-capability decision),
not on a supposed old Rust dependency mismatch.

Independent sender review/fix and the native integration decision remain
separate. This candidate does not accept or close the undertaking. Nothing was
pushed, published, deployed or applied to production state.
