# Medousa hardening program

> **Status:** Active — Phase 0 documentation and safety truth
> **Started:** 2026-08-13
> **Source audit:** [Repo-wide technical critique](../repo-wide-technical-critique-2026-08-12.md)
> **Canonical product and integrator docs:** [docs/README.md](../../docs/README.md)

This program turns the repo-wide audit into bounded engineering work. It is the
control document for remediation status, dependencies, decisions, evidence,
and documentation closure. It is not canonical documentation for behavior that
has not shipped.

The audit is frozen evidence. Findings are clarified through linked design and
verification records; they are not silently rewritten out of the audit when an
implementation changes.

## Program rules

1. **Security and correctness before throughput.** Do not optimize an unsafe or
   lossy contract and call the finding closed.
2. **One primary owner per finding.** Cross-cutting work may appear in several
   plans, but only the primary plan owns closure.
3. **Decide once.** Durable product and architecture choices become ADRs.
   Execution details, sequencing, and checklists stay in this directory.
4. **Evidence closes work.** A merged implementation is not enough. Required
   abuse, crash, concurrency, compatibility, and performance evidence must be
   attached to the finding.
5. **Code and canonical docs ship together.** Update `docs/` only when behavior
   ships, except for immediate warnings that correct unsafe or false guidance.
6. **Delete the old path.** Each execution plan identifies superseded code,
   compatibility branches, duplicated state, and documentation to remove.
7. **No percentage theater.** Performance targets require a reproducible
   workload and baseline. Algorithmic findings may be accepted before a
   benchmark, but performance closure requires measurement.

## Status model

| State | Meaning |
| --- | --- |
| Proposed | Problem is mapped; decision or implementation contract is incomplete |
| Accepted | Required ADRs and plan invariants are approved |
| Implementing | Code or migration work is active |
| Validated | Exit criteria and required evidence pass on supported targets |
| Shipped | Code, migrations, observability, rollback, and canonical docs are released |
| Blocked | A named external decision or prerequisite prevents meaningful progress |

Only **Shipped** closes an audit finding. “Validated” deliberately leaves room
for packaging, migration, or documentation work.

## Release gates

### Gate A — contain authority

The following are release-boundary work and precede expansion of public/LAN or
embedded-browser capabilities:

- SEC-001: authenticated trust zones and reduced public router;
- SEC-002/SEC-003: validated identifiers and filesystem confinement; and
- DESKTOP-001: untrusted webviews with no ambient Tauri authority.

Until Gate A ships, non-loopback daemon binding is documented as an unsafe
trusted-network development escape hatch, not a secure pairing boundary.

### Gate B — trustworthy state

DUR-001, STORE-001, CONSIST-001, CONC-001, CONC-002, and DATA-001 must have
crash/concurrency tests before persistence and runtime performance work can be
declared complete.

### Gate C — bounded hot paths

The stream, persistence, Forge/Coder, and vault paths must have explicit queue,
retention, I/O, and latency budgets. No unbounded queue or whole-state rewrite
may remain on a request-critical path without a documented exception.

### Gate D — enforced architecture

Generated contracts, dependency boundaries, complete CI, deterministic tests,
and performance budgets must prevent the same classes of defect from returning.

## Workstreams

| ID | Planned document | Scope | Primary findings | Depends on | State |
| --- | --- | --- | --- | --- | --- |
| H01 | [01-daemon-trust-and-auth.md](01-daemon-trust-and-auth.md) | Daemon trust zones, authentication, CORS, bootstrap and route exposure | SEC-001 | ADR-013 | Draft |
| H02 | [02-identifier-and-filesystem-authority.md](02-identifier-and-filesystem-authority.md) | Validated IDs, path derivation, symlinks, deletion inventory | SEC-002, SEC-003, DATA-001 | ADR-014 | Draft |
| H03 | [03-turn-stream-v2.md](03-turn-stream-v2.md) | Bounded single-writer stream, replay, journal, bridge, UI tail | PERF-001, DUR-001, MEM-002, TYPE-001, PERF-005 | ADR-015, H05 | Draft |
| H04 | [04-persistence-and-crash-consistency.md](04-persistence-and-crash-consistency.md) | Feed/workspace/task storage ownership and commit policy | STORE-001, STORE-002, MEM-001 | ADR-016 | Draft |
| H05 | [05-runtime-context-and-concurrency.md](05-runtime-context-and-concurrency.md) | Request-scoped turn/browser state and cancellation | CONC-001, CONC-002 | ADR-017 | Draft |
| H06 | [06-forge-coder-scaling.md](06-forge-coder-scaling.md) | Incremental Forge state, checkpoint observation, blocking work | PERF-002, PERF-004, ASYNC-001 | ADR-016, H05 | Draft |
| H07 | [07-vault-scaling-and-consistency.md](07-vault-scaling-and-consistency.md) | Incremental index, atomic mutation, backend/frontend lookups | PERF-003, PERF-006, CONSIST-001 | ADR-014, ADR-016 | Draft |
| H08 | [08-desktop-browser-isolation.md](08-desktop-browser-isolation.md) | Remote webview capabilities and request-correlated bridge | DESKTOP-001 | ADR-018, H05 | Draft |
| H09 | [09-home-runtime-boundaries.md](09-home-runtime-boundaries.md) | Feature loading, runtime cycles, store/component/CSS ownership | FRONT-001, ARCH-001, ARCH-002 | ADR-020, H03 | Draft |
| H10 | [10-api-contract-generation.md](10-api-contract-generation.md) | Authoritative API definition and generated clients/tests | CONTRACT-001 | ADR-019, H01 | Draft |
| H11 | `11-package-and-dependency-boundaries.md` | Optional workload features and dependency budgets | DEP-001 | ADR-020 | Proposed |
| H12 | `12-quality-gates.md` | CI matrix, deterministic tests, benchmarks and budgets | CI-001, TEST-001, PERF-007 | All workstreams | Proposed |

## Planned durable decisions

New decisions use the next available identifiers while preserving the existing
duplicate ADR-010 history.

| ADR | Decision | Related existing decision | State |
| --- | --- | --- | --- |
| [ADR-013](../../docs/architecture/decisions/adr-013-daemon-trust-zones-and-auth.md) | Daemon trust zones, mandatory authentication, CORS, and public exposure | Narrows ADR-003 and ADR-011 | Proposed |
| [ADR-014](../../docs/architecture/decisions/adr-014-identifier-and-filesystem-authority.md) | Validated identifiers and handle-relative filesystem confinement | New | Proposed |
| [ADR-015](../../docs/architecture/decisions/adr-015-bounded-durable-turn-pipeline.md) | Bounded single-writer durable turn pipeline | Supersedes ADR-004's per-event write tradeoff; preserves replay contract | Proposed |
| [ADR-016](../../docs/architecture/decisions/adr-016-transactional-store-ownership.md) | Transactional store ownership and crash-consistency policy | Extends durable runtime decisions | Proposed |
| [ADR-017](../../docs/architecture/decisions/adr-017-request-scoped-runtime-context.md) | Request-scoped runtime context; no process-global turn state | Extends ADR-005/ADR-008 | Proposed |
| [ADR-018](../../docs/architecture/decisions/adr-018-untrusted-webview-isolation.md) | Untrusted webview isolation and minimal browser bridge | Revises browser-host assumptions | Proposed |
| [ADR-019](../../docs/architecture/decisions/adr-019-generated-api-contract.md) | Route-owned generated API and client contract | Replaces handwritten parity convention | Proposed |
| [ADR-020](../../docs/architecture/decisions/adr-020-feature-boundaries-and-lazy-runtime.md) | Feature boundaries, lazy loading, and optional workload packaging | Formalizes Home-first optional packages | Proposed |

An ADR must state exactly which earlier consequence it supersedes. New language
must not quietly contradict an accepted decision while leaving both marked
current.

## Planned verification records

| Record | Purpose | State |
| --- | --- | --- |
| [Security abuse matrix](verification/security-abuse-matrix.md) | Credentials, route exposure, CORS/CSRF, traversal, symlinks, and webview IPC abuse | Draft baseline contract |
| [Crash/concurrency matrix](verification/crash-concurrency-matrix.md) | Kill points, concurrent mutations/turns, cancellation, replay, and deletion | Draft baseline contract |
| [Performance budgets](verification/performance-budgets.md) | Reproducible datasets, machines, metrics, baselines, and regression budgets | Draft baseline contract |

Verification records contain commands, fixtures, environment identity, raw
artifact locations, and pass/fail thresholds. Results should be append-only or
linked to immutable CI artifacts; do not replace a bad baseline with a better
one without retaining the history.

## Finding ledger

| Finding | Severity | Primary owner | Gate | State | Closure evidence |
| --- | --- | --- | --- | --- | --- |
| SEC-001 | Critical | H01 | A | Proposed | Authentication/route abuse matrix |
| SEC-002 | Critical | H02 | A | Proposed | Cross-platform traversal/destructive-operation tests |
| SEC-003 | High | H02 | A | Proposed | Symlink/junction race tests |
| DESKTOP-001 | Critical | H08 | A | Proposed | Packaged remote-origin IPC denial/bridge tests |
| PERF-001 | Critical | H03 | C | Proposed | Stream allocation/I/O/latency profile |
| MEM-002 | High | H03 | C | Proposed | Stalled-consumer memory and cancellation stress |
| STORE-001 | Critical | H04 | B | Proposed | Concurrent append and crash-recovery tests |
| DUR-001 | Critical | H03 | B | Proposed | Injected write/sync failure tests |
| PERF-002 | Critical | H06 | C | Proposed | Forge replay/mutation benchmark and compaction proof |
| MEM-001 | Critical | H04 | C | Proposed | Retention/eviction stress and restart test |
| ASYNC-001 | High | H06 | C | Proposed | Executor-blocking and saturation profile |
| PERF-003 | High | H07 | C | Proposed | Vault cold/warm scaling benchmark |
| CONSIST-001 | High | H07 | B | Proposed | Atomic compare-and-write race test |
| CONC-001 | Critical | H05 | B | Proposed | Concurrent-turn isolation matrix |
| CONC-002 | High | H05 | B | Proposed | Correlated concurrent browser request tests |
| TYPE-001 | High | H03 | D | Proposed | Generated exhaustive protocol/reducer tests |
| STORE-002 | High | H04 | C | Proposed | Serialization-to-delta amplification benchmark |
| PERF-004 | Critical | H06 | C | Proposed | Checkpoint repository-size/dirty-byte benchmark |
| PERF-005 | Critical | H03 | C | Proposed | Browser streaming render/long-task profile |
| FRONT-001 | High | H09 | D | Proposed | Manifest and cold-start budgets |
| ARCH-001 | High | H09 | D | Proposed | Zero-new-cycle check and migration ledger |
| PERF-006 | High | H07 | C | Proposed | Large-vault tree/link interaction benchmark |
| ARCH-002 | High | H09 | D | Proposed | Boundary tests and deleted legacy ownership paths |
| CONTRACT-001 | High | H10 | D | Proposed | Generated artifact diff plus black-box API contract |
| DEP-001 | High | H11 | D | Proposed | Feature/dependency graph and build/package budgets |
| CI-001 | High | H12 | D | Proposed | Required green supported-platform matrix |
| TEST-001 | High | H12 | D | Proposed | Hermetic repeated parallel suite |
| PERF-007 | High | H12 | D | Proposed | Checked-in benchmark suite and retained baselines |
| DATA-001 | Medium | H02 | B | Proposed | Fresh-process deletion inventory |

## Required plan template

Every H01–H12 plan contains:

1. status, accountable owner, reviewers, audit IDs, and ADR dependencies;
2. current evidence and code anchors;
3. invariants, threat/failure model, and non-goals;
4. current and target ownership/data flows;
5. API, event, storage, and compatibility changes;
6. concurrency, cancellation, durability, and resource limits;
7. observability and operator diagnostics;
8. migration, staged rollout, and rollback;
9. tests, benchmarks, and exact exit criteria;
10. canonical documents changed at ship time; and
11. superseded code, flags, data, and documents to remove.

## Documentation closure

Implementation PRs update the relevant canonical surfaces as they ship:

| Area | Canonical documentation |
| --- | --- |
| Daemon authentication/exposure | `docs/engine/`, `docs/configuration-reference.md`, LAN/pairing cookbooks and guides |
| Session/filesystem authority | HTTP API, vault/artifact/session docs, upgrade/data-dir runbook |
| Streaming and durability | Engine/SDK streaming guides, component-engine, Home app reference, connection runbook |
| Forge/workspace/vault behavior | Respective `docs/engine/*.md` guides and operator runbooks |
| Desktop browser | Home app reference plus browser architecture docs |
| Generated API/SDKs | SDK overview/reference/transports/Python docs and contributor rules |
| Packages/dependencies | Packages guide and build-from-source cookbook |
| CI/performance | Contributor documentation, this program, and retained verification records |

## Phase 0 checklist

- [x] Preserve the audit as the evidence baseline.
- [x] Create this program control document and assign every finding once.
- [x] Correct the ADR index without renumbering accepted history.
- [x] Remove duplicate roadmap focus text and reopen incomplete CI hardening.
- [x] Mark non-loopback daemon binding as unsafe until H01 ships.
- [x] Create the three verification records.
- [x] Draft ADR-013 and H01 together.
- [ ] Review and accept the Gate A threat model before implementation.
