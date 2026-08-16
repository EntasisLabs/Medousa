# Performance budgets and evidence protocol

> **Status:** Draft baseline contract
> **Program:** [Medousa hardening](../README.md)
> **Primary findings:** PERF-001–PERF-007, MEM-001, MEM-002, STORE-001,
> STORE-002, ASYNC-001, FRONT-001, ARCH-001, DEP-001
> **Required decisions:** ADR-015, ADR-016, ADR-020 (planned)

This document defines how Medousa will measure and gate performance. It avoids
inventing impressive percentages without a stable workload. Current measured
facts are recorded as baselines; target budgets become binding when the owning
workstream accepts its dataset, harness, and reference environment.

Performance closure requires both:

1. removal or bounding of the algorithmic/I/O defect identified in the audit;
   and
2. reproducible evidence that the resulting system meets an accepted budget.

A faster lossy implementation fails. Correctness, durability, cancellation,
and security matrices run alongside performance probes.

## Budget classes

| Class | Meaning | Enforcement |
| --- | --- | --- |
| Safety bound | Hard resource/liveness invariant such as maximum queued bytes or cancellation time | Must pass every run; no statistical waiver |
| Regression budget | Current accepted value that may not worsen beyond noise allowance | Required PR check on stable probes |
| Target budget | Desired end-state selected from product/hardware evidence | Required to close the owning finding |
| Diagnostic metric | Recorded to explain changes but not initially gated | Retained in CI time series; promoted when stable |

Until an H01–H12 plan accepts a target, the default rule is **no unexplained
regression**. Structural work should lower a ratchet; it must never reset the
baseline upward merely to make CI green.

## Measurement environments

### Reference tiers

| Tier | Purpose | Requirements |
| --- | --- | --- |
| `micro-ci` | Stable engine/store microbenchmarks | Pinned runner image/CPU class, local SSD, release build, network disabled unless measured |
| `desktop-macos` | Primary packaged app and WebKit behavior | Named Apple hardware/OS/WebKit versions, clean user profile, release package |
| `desktop-windows` | WebView2, NTFS/reparse, package behavior | Named hardware/Windows/WebView2 versions, release package |
| `desktop-linux` | WebKitGTK/filesystem behavior | Named distro/kernel/WebKitGTK, release package |
| `scale-nightly` | Large vault/repository and saturation probes | Dedicated retained runner with fixed dataset disk image |

The first implementation of H12 records exact machine identifiers and creates a
calibration probe. Results from changing hosted runners may detect catastrophic
regressions but cannot support tight latency thresholds.

### Run hygiene

- Use release/production builds and the same feature set being compared.
- Record revision, dirty state, compiler/runtime/dependency versions, OS,
  filesystem, CPU, memory, storage, power mode, and thermal state where exposed.
- Warm/cold behavior are separate workloads; never warm a “cold start” result.
- Disable unrelated background work and network variability, or record it as a
  deliberate dimension.
- Retain raw samples, not only averages.
- Measure at least 20 steady samples for short benchmarks after warm-up; use
  longer fixed-duration windows for throughput/saturation tests.
- Report median, p95, p99 where sample size supports it, median absolute
  deviation, min/max, and confidence/bootstrap interval selected by H12.
- Compare distributions and effect size. A single lucky run does not lower a
  ratchet.

## Current measured baseline

The 2026-08-12 audit established these reproducible build artifacts:

| Metric | Current observation | Baseline use |
| --- | ---: | --- |
| Root route initial static JavaScript | 7,102,090 minified bytes across 56 files | Binding regression ceiling until H09 adopts a lower ratchet |
| Root route initial JS per-file gzip sum | 2,120,493 bytes | Diagnostic; transfer behavior differs by packaging/server |
| Root route initial static CSS | 1,448,096 minified bytes across 11 files | Binding regression ceiling until H09 lowers it |
| Root route initial CSS per-file gzip sum | 189,858 bytes | Diagnostic |
| Complete generated client JavaScript | 11,761,808 minified bytes across 164 files | Regression ceiling |
| Largest initial application/page chunk | 2,199,774 bytes | Regression ceiling |
| Global generated CSS | 953,407 minified bytes | Regression ceiling |
| Frontend runtime SCCs | 7; largest contains 74 modules | Architectural budget: zero new SCCs and zero growth |
| Main daemon normal dependency closure | 932 unique name/version pairs | Regression ceiling; features reported separately |
| Duplicate-version crate names | 93 | Regression ceiling with reviewed exceptions |

The audit build took roughly 28 seconds, but its hardware/process conditions
were not captured sufficiently for a binding build-time budget. It remains
context only. No trustworthy runtime latency or allocation baseline exists yet;
H12 must create it before speedup claims are accepted.

## Global hard safety bounds

The owning ADRs choose exact byte/time values. The following properties are
non-negotiable:

| Resource | Required bound |
| --- | --- |
| Stream queues | Bounded in messages and bytes per turn plus process-wide cap |
| Pending browser calls | Bounded per surface and globally; every entry has deadline |
| Replay memory | Bounded ring/segments; durable journal or explicit expiry owns older replay |
| Completed task/worker registries | Bounded records and bytes with active/recovery exemptions |
| Feed/workspace snapshots | Bounded pending generations; superseded state does not retain serialized copies |
| Coder checkpoint observations | Bounded bytes read/hashed per ordinary boundary; exceptional full validation explicit |
| Snapshot/HTML/event payloads | Schema, depth, and byte limits before allocation/IPC |
| Cancellation | Finite accepted deadline under full queues, stalled sinks, and failed writers |
| Shutdown/flush | Finite deadline and explicit incomplete-durability result |

An implementation with “practically bounded by provider output” or “eventually
times out upstream” does not satisfy the safety budget.

## Benchmark suite

### P01 — Turn streaming spine

**Findings:** PERF-001, MEM-002, DUR-001, TYPE-001

| Dataset | Dimensions |
| --- | --- |
| Synthetic prose stream | 10,000 fragments at 1, 8, 32, and 256 bytes |
| Realistic model stream | Recorded synthetic fixture preserving sizes/timing without user content |
| Transcript pressure | 0, 100, and 1,000 existing messages |
| Subscribers | 0, 1, and several live/reconnecting clients |
| Sink state | Fast, slow disk, blocked UI, injected writer failure, cancellation |

Record:

- time to first accepted/published delta;
- producer-to-journal and producer-to-client p50/p95/p99 latency;
- allocations/allocated bytes and copies by layer;
- journal write/flush/sync syscalls and bytes;
- queue message/byte high-water marks;
- Tokio worker blocking time and task count;
- replay memory and reconnect latency; and
- cancellation/terminal drain latency.

Target properties:

- work and syscalls scale with emitted **batches**, not provider fragments;
- memory remains below the accepted bound with an indefinitely stalled sink;
- terminal sequence is monotonic and writer failure is visible; and
- end-to-end total work is O(total bytes + batches), not O(fragment count ×
  accumulated response/transcript).

### P02 — Home streaming render

**Findings:** PERF-005, PERF-006

Harnesses:

- `cd apps/medousa-home && npm run bench:p02 -- --full` runs the deterministic
  happy-dom allocation/work baseline; add `--baseline` only to reproduce the
  retired whole-answer replacement path.
- While `npm run dev` is active, open
  `/p02-browser-harness?bytes=100000&fragment=256` for the dev-only real-browser
  frame, task-delay, Long Task, heap, DOM, and hydration counters. The route is
  unavailable in ordinary production builds.
- To build the isolated packaged-app probe on macOS, run
  `PUBLIC_P02_HARNESS=1 npx tauri build --config src-tauri/tauri.p02.conf.json --bundles app`.
  The dedicated config omits the daemon sidecar, opens the harness route directly,
  prints one `MEDOUSA_P02_RESULT=<json>` line, and exits. Select the workload at
  runtime with `MEDOUSA_P02_BYTES` and `MEDOUSA_P02_FRAGMENT_BYTES`. Neither the
  route nor its two native commands is enabled in an ordinary production build.

Run in the packaged app and a browser harness with 1k, 10k, and 100k generated
characters containing prose, links, tables, fenced code, Mermaid, and Liquid.
Compare fragment rates and the accepted engine batch cadence.

Record:

- main-thread time, long tasks, missed frames, input latency;
- Markdown parse, sanitize, highlighting, Mermaid, and hydration duration;
- DOM nodes created/replaced and mount teardown count;
- JS heap/allocations and GC pauses; and
- terminal full-render and steady completed-block cache cost.

Target properties:

- completed blocks are parsed/hydrated once per source change;
- streaming work is proportional to changed tail plus bounded scheduling work;
- no whole-answer DOM replacement per provider fragment; and
- a user can scroll/type/select during the 100k stream without sustained long
  tasks beyond the accepted UI budget.

#### 2026-08-15 packaged WebKit evidence

Candidate: H03.5 on `codex/h03-turn-stream-v2`. Environment: release `.app`,
macOS 26.5.2 (25F84), Mac16,10, Apple M4, 16 GiB RAM. These are single-run
closure observations, not a statistically accepted H12 regression ratchet.
WKWebView did not expose `performance.memory`, so packaged heap remains
unavailable.

The representative fixture contains prose, links, inline code, one table, one
fenced Rust block, one Mermaid block, and one Liquid block. Remaining bytes are
ordinary linked/code prose. Fragment size was 256 bytes.

| Source | Elapsed | Update p95 / p99 / max | Frame gap p95 / max | Task delay p95 / max | Stable blocks | DOM nodes | Long tasks / whole replacements / teardowns |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 B | 67 ms | 1 / 1 / 7 ms | 20 / 21 ms | 10 / 19 ms | 12 | 136 | 0 / 0 / 0 |
| 10,000 B | 664 ms | 3 / 4 / 7 ms | 19 / 21 ms | 5 / 19 ms | 92 | 457 | 0 / 0 / 0 |
| 100,000 B | 6,517 ms | 4 / 4 / 7 ms | 18 / 21 ms | 5 / 19 ms | 896 | 3,671 | 0 / 0 / 0 |

The 100k packaged run therefore preserves interactive frame/task latency while
the answer grows, hydrates every completed block once, and performs no
whole-answer replacement or mount teardown.

Fixture density is a material dimension. An earlier diagnostic repeated the
entire rich prelude every roughly 270 bytes, producing about 375 Mermaid and
Liquid blocks in 100k. Packaged WebKit took 52,383 ms with 249 ms p95 frame gaps,
269 ms maximum task delay, and 38,917 DOM nodes even though individual update
work stayed at or below 9 ms. That is an embed-density/DOM pressure workload,
not the accepted prose-stream fixture; retain it as evidence that future H09/H12
work needs an explicit rich-embed admission or virtualization budget.

### P03 — Feed and workspace persistence

**Findings:** STORE-001, STORE-002

**H04 implementation state:** the hot paths now append delta records, coalesce
typed workspace mutations before serialization, and enforce bounded admission.
Focused tests cover ordering, recovery, and thresholds. A retained benchmark
artifact across the full matrix below is still pending; no latency/RSS closure
is inferred from unit tests.

| Dimension | Values |
| --- | --- |
| Feeds/records | 1, 100, 500, 10,000 where supported |
| Event/body size | 128 B, 4 KiB, 64 KiB |
| Producers | 1, 4, 20 |
| Writer | normal, slow, temporarily blocked |

Record operation latency, bytes cloned/serialized/written, lock hold time,
queued generations, compaction work, sync calls, and writer CPU.

Target properties:

- append/update cost is O(delta) amortized rather than O(retained state);
- unrelated feeds/stores progress independently;
- debounce occurs before whole-state serialization; and
- overload applies bounded admission without synchronous async-thread fallback.

### P04 — Forge event store

**Harness:** `cargo run -p medousa-forge --example p04_forge_store`
CI sizes: 0 / 100 / 10k events. Nightly: `MEDOUSA_P04_EVENTS=1000000`.

**Evidence status:** Retained metrics harness shipped (H06.11). Local 2026-08-15
darwin run recorded throughput, p50/p95/p99, bytes read/written, sync count,
decoded events, lock hold, cold/warm tail, and RSS. Closure for PERF-002 is
**not** met: steady append still rescans/decodes historical events
(`decoded_events_est ≈ n(n−1)/2`). Multi-OS packaging evidence pending.

**Findings:** PERF-002, ASYNC-001 (open)

#### 2026-08-15 darwin retained run (`p04_forge_store`)

| Phase | Events | Throughput (eps) | p50 / p95 / p99 (ms) | Bytes written | Bytes read (est) | Syncs (est) | Decoded events (est) | Lock hold (ms) | Cold / warm tail (ms) | RSS before → after |
| --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| warm | 100 | 230.8 | 3.979 / 4.983 / 5.046 | 23,050 | 1,222,048 | 99 | 4,950 | 423.002 | 1.397 / 0.019 | 7.7 MiB → 8.1 MiB |
| cold_reopen | 100 | 238.0 | 3.958 / 4.956 / 4.996 | 23,050 | 1,222,048 | 99 | 4,950 | 410.020 | 1.408 / 0.021 | 8.1 MiB → 8.2 MiB |
| warm | 10,000 | 13.9 | 71.953 / 133.556 / 138.491 | 2,367,508 | 11,834,535,198 | 9,999 | 49,995,000 | 720,419.288 | 136.241 / 0.026 | 8.2 MiB → 19.8 MiB |
| cold_reopen | 10,000 | 13.9 | 71.825 / 133.480 / 138.725 | 2,367,505 | 11,834,524,560 | 9,999 | 49,995,000 | 719,355.485 | 136.698 / 0.021 | 19.8 MiB → 19.9 MiB |

Retained-directory size at 10k events ≈ 2.37 MiB. Warm cached tail stays
sub-millisecond; append cost and decoded-event count grow with history.

Generate valid Forge histories at 0, 100, 10k, and 1m events with small and
large evidence payloads. Exercise reads and every common mutation, including
concurrent unrelated items.

Record replayed/decoded events, bytes read/written, allocations, sync calls,
subprocesses, executor blocking, lock duration, mutation p99, startup recovery,
and compaction duration/space amplification.

Target properties:

- steady mutation does not replay the complete historical log;
- snapshots/indexes have a verified generation and bounded recovery fallback;
- blocking Git/filesystem/subprocess work runs in bounded workers; and
- compaction does not block unrelated item progress beyond its budget.

### P05 — Coder checkpoint

**Harness:** `cargo run -p medousa-forge --example p05_coder_observation`
CI size: 1k files. Larger: `MEDOUSA_P05_FILES=100000`.

**Evidence status:** Scenario matrix harness shipped (H06.11). Local 2026-08-15
darwin run covers clean, small dirty, many untracked, large diff, concurrent
mutation/watcher, bounded/truncated, and resume budget envelope with wall-clock
and RSS. PERF-004 remains **open**: this matrix proves observation honesty and
budgets, not yet that model-only logical boundaries issue zero Git/subprocess
work. Multi-OS packaging evidence pending.

**Finding:** PERF-004 (open)

#### 2026-08-15 darwin retained run (`p05_coder_observation`, files=1000)

| Scenario | Files | Completeness | Limits | Wall (ms) | RSS before → after | Notes |
| --- | ---: | --- | --- | ---: | --- | --- |
| clean | 1000 | Exact | [] | 54.232 | 6.5 → 6.8 MiB | unchanged worktree |
| small_dirty | 1000 | Exact | [] | 28.549 | 6.8 → 7.0 MiB | 1 changed path |
| many_untracked | 700 | Exact | [] | 44.669 | 7.0 → 7.7 MiB | 500 untracked hashed/cached |
| large_diff | 400 | Exact | [] | 62.305 | 7.7 → 7.7 MiB | 400 changed paths |
| concurrent_mutation | 200 | Unknown | generation_changed | 26.188 | 7.8 → 7.8 MiB | capture flip mid-observe |
| concurrent_watcher | 120 | Unknown | generation_changed | 160.344 | 7.8 → 7.8 MiB | live fence bump during observe |
| bounded_truncated | 70 | Incomplete | per_file_bytes | 26.775 | 7.8 → 7.8 MiB | tight budgets; never Exact |
| budget_envelope | 300 | Exact | [] | 34.221 | 7.8 → 7.8 MiB | resume budgets; rss_delta=0 |

Use generated Git repositories at 1k, 100k, and 1m files with these states:

- clean;
- small tracked edit;
- many tracked edits;
- many small untracked files;
- one multi-gigabyte sparse/real untracked file as runner permits; and
- watcher generation unchanged versus changed.

Measure model-only boundary, non-mutating tool boundary, mutating tool boundary,
and resume validation.

Record Git subprocess count/time, filesystem entries/content bytes read,
diff/hash allocations, snapshot serialization bytes/passes, checkpoint pause,
worker utilization, and cancellation.

Target properties:

- a boundary with unchanged workspace generation performs no full diff or
  untracked-content hash;
- hashing streams into a digest and obeys accepted exceptional-work limits;
- logical checkpoint cost scales with checkpoint delta; and
- exact full validation is reserved for explicit mutation/resume conditions.

### P06 — Vault backend

**Findings:** PERF-003, CONSIST-001

**Harness:** `cargo run -p medousa --example p06_vault_backend`  
CI defaults stay small (24/64 notes). Retained/nightly scale:
`MEDOUSA_P06_NOTES=100000 cargo run -p medousa --example p06_vault_backend`.

Datasets contain 100, 10k, and 100k notes with shallow, deep, and wide trees;
small and large notes; links/tags; external edits; and controlled filesystem
timestamp behavior.

Record entries statted/read, content bytes parsed, index/link rebuilds, lock
duration, file bytes written, cold/warm read/list/search/write latency, startup
index recovery, and conflict latency.

Target properties:

- warm ordinary reads/listing do not recursively stat the full vault;
- one note mutation updates affected index/link entries rather than rebuilding
  global state;
- atomic compare-and-write adds bounded overhead; and
- external change detection scales with changed paths/events plus bounded
  reconciliation.

**Evidence:** H07.0 baselines record measurements via the harness; target gates
attach after H07.2/H07.1b. Multi-OS retained tables pending H07.6.

### P07 — Vault/Home UI

**Finding:** PERF-006

Use the P06 datasets with fully collapsed, partially expanded, fully expanded,
deep-chain, wide, and link-heavy views.

Record selection/edit latency, mounted rows, subtree visits, path-set/index
construction, wikilink resolution operations, allocations, long tasks, and heap.

Target properties:

- selection ancestor checks are O(depth) preparation plus O(1) per visible row;
- only visible rows perform reactive work;
- shared vault lookup maps build once per vault generation; and
- `L` links resolve in O(L) expected lookup work, not O(L × N notes).

### P08 — Home cold start and bundle

**Findings:** FRONT-001, ARCH-001, ARCH-002

Measure clean process launch to first interactive chat shell and first use of
Vault, Code, Browser, Workshop, Settings, and mobile/desktop-specific surfaces.

Record:

- Vite manifest initial static closure, total assets, and largest chunks;
- read/parse/compile/evaluate/module-initialization time;
- first paint and first interaction;
- heap after shell and after each lazy feature;
- CSS bytes/rule parse/style-recalc time; and
- runtime cycle inventory and feature-side-effect initialization.

Initial binding regression budgets are the measured byte/SCC baselines above.
H09 must set lower target budgets after the first feature-boundary design. A
lazy split that merely moves work into an immediate post-launch dynamic import
does not pass runtime targets.

### P09 — Dependencies and build/package cost

**Finding:** DEP-001

Measure the default daemon, minimal personal daemon, optional workload feature
sets, SDKs, and Tauri apps separately.

Record unique packages, duplicate-version names, enabled features, clean/warm
check/build/link time, peak build/link memory, artifact size, packaged app size,
and advisory/license/source exceptions.

Initial binding regression ceilings are 932 unique normal name/version pairs
and 93 duplicate-version names for the main daemon. H11 must lower them after
removing unused direct dependencies and define per-feature budgets. A new
dependency requires an owner, feature justification, size/build delta, and
duplicate-version explanation.

### P10 — Retention soak

**Findings:** MEM-001, MEM-002

**H04 implementation state:** feed tails, workspace projections/journal, task
output/replay, and terminal run registries now have enforced count/byte/TTL
bounds with threshold tests. The required multi-hour packaged soak and
post-idle RSS evidence remain pending.

Run multi-hour synthetic operation with repeated turns, workers, task runs,
browser requests, reconnects, failures, and cancellations. Include a workload
that crosses every retention threshold several times.

Record RSS/private bytes, allocator live/retained bytes, registry and queue
counts/bytes, file growth, task/handle descriptors, compaction/eviction events,
and post-idle steady state.

Target properties:

- live memory reaches a workload-proportional plateau after retention caps;
- completed runs and pending requests leave registries according to policy;
- disconnected clients/stalled writers cannot pin unbounded data; and
- on-disk growth matches documented journals/retention, not leaked snapshots.

## Regression policy

### Pull requests

- Run stable P01/P03 microbenchmarks and manifest/dependency budgets affected by
  the diff.
- Fail hard safety bounds immediately.
- Fail deterministic byte/count budgets on any increase unless the reviewed
  budget file changes with justification.
- For noisy latency probes, compare against the branch-point baseline on the
  same runner. Flag a statistically credible regression above the accepted
  noise/effect threshold; require rerun and owner review rather than blind
  percentage rounding.

### Nightly and release

- Run full P01–P10 on pinned/reference environments at their supported cadence.
- Retain raw result artifacts and a time-series summary.
- Bisect sustained regressions; do not normalize the baseline until the owning
  plan accepts the product tradeoff.
- Release evidence includes packaged cold start, retention soak, large
  vault/repository, and the correctness matrices.

## Budget change protocol

A budget change includes:

1. metric and workload identifier;
2. old/new value and raw comparison evidence;
3. product reason and affected users/hardware;
4. correctness/security/durability impact;
5. alternatives considered;
6. accountable owner and expiry/revisit condition; and
7. updates to the owning workstream and finding ledger.

Budget increases are architecture decisions when they weaken a hard resource or
user-visible service-level invariant. They are not drive-by fixture updates.

## Result schema

Each benchmark emits a machine-readable record containing:

```text
schema_version, workload_id, dataset_version
git_revision, dirty_state, feature_set, build_profile
machine_id/class, cpu, memory, storage, os, filesystem
compiler, node, webview and relevant dependency versions
warmup and sample policy
raw samples and summary statistics
allocation, syscall, queue, registry and byte counters as applicable
correctness/fault-matrix companion result IDs
artifact hashes and profiler trace locations
```

Synthetic fixtures contain no user content or credentials.

## Exit criteria

The performance findings close only when:

- P01–P10 have checked-in reproducible harnesses and versioned datasets;
- the owning plans replace every “target properties” section with accepted
  numeric safety/latency/resource budgets on named environments;
- algorithmic work matches the stated scaling property under dataset growth;
- hard queue, retention, payload, cancellation, and shutdown bounds pass;
- PR and nightly/release tiers retain raw evidence and enforce ratchets;
- corresponding crash/concurrency/security cases pass; and
- canonical operator/developer docs describe any user-visible limits or
  configuration.

Until then, improvements are useful evidence but not proof that PERF-007 is
resolved.
