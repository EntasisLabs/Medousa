# iOS embedded daemon

> **Status:** Recovery milestones 1–3 complete — local daemon composition next
>
> **First target:** iOS (`aarch64-apple-ios` and simulator)
>
> **Deferred targets:** Android, browser WASM
>
> **Date:** 2026-08-23

## Product invariant

`medousa_daemon` owns every operation that constitutes work: authority,
sessions, transcript sequencing, turns, memory, tools, persistence, scheduling,
delegation, and recovery. Embedded, desktop, server, and headless are deployment
and integration profiles of that same product.

Medousa Home and the mobile UI are clients. A co-located client receives the
daemon's highest local privilege through the trusted in-process/local bridge.
That privilege never crosses a daemon-to-daemon boundary.

An embedded mobile daemon talking to another daemon reuses the existing
pairing, authentication, signed-envelope, transport, and capability-grant
flows. New requirements extend those flows; they do not create a privileged
shortcut.

## Product goal

A fresh Medousa install on iPhone boots an embedded deployment of
`medousa_daemon` inside the Tauri process. The local UI can immediately use its
privileged daemon client to create and continue conversations, run the
production agent loop, use the mobile-safe Grapheme tool surface, and persist
Locus memory. AI inference may remain remote for the first release.

The user can still select a paired full daemon with the existing workshop
client. The embedded daemon can later delegate heavier work to that daemon
through authenticated daemon-to-daemon flows while preserving turn and
conversation provenance.

## Locked decisions

| Area | Decision |
|---|---|
| Runtime owner | `medousa_daemon`; never Home, Tauri, or a mobile-only service model |
| Runtime form on iOS | In-process daemon composition; no child process and no loopback HTTP requirement |
| Client privilege | Co-located Home/mobile client receives explicit local-root capability |
| Peer privilege | No implicit trust; reuse pairing, authentication, signed envelopes, and capability grants |
| Control plane | Stasis cluster nodes, capabilities, queue ownership, jobs, and agent envelopes |
| Conversation identity | Existing Medousa `AuthorityId`, `SessionRef`, `ExecutionRef`, entry sequence, and derivation model |
| Turn lifecycle | Existing production loop, turn event spine, replay, cancellation, commit, and recovery contracts |
| Persistence | One daemon-owned Stasis/Locus/Surreal composition rooted in the app sandbox |
| Tools | Existing daemon tool implementations registered through a mobile-safe allowlist |
| Inference | Explicit credential-provider adapter backed by the existing daemon integration secret in Keychain |
| Remote execution | Authenticated daemon-to-daemon request with bounded context and returned provenance |
| Backgrounding | Foreground execution with cancellation/checkpoint recovery; no promise of suspended execution |
| Notes/vault | Not required for the first iOS deployment |
| WASM | Deferred until the iOS daemon profile is proven and later Stasis support is available |

## Existing authority to reuse

The mobile deployment must compose these existing contracts instead of
reimplementing them:

- Medousa authority, session, transcript-entry, execution, and context-manifest
  types;
- the immutable authority-scoped session store and deterministic derivation
  service;
- the durable turn event log, monotonic sequence assignment, replay cursor,
  commit marker, cancellation, and active-turn reattachment;
- Stasis `RuntimeComposition`, control-plane node registration, capability
  tags, queue ownership, typed jobs, agent envelopes, causation, handoff, and
  durable waits;
- pairing identities, signed nonces, bearer/session credentials, signed mesh
  envelopes, delivery receipts, and LAN/Iroh routing;
- the existing daemon memory bundle and Locus semantic-index adapter;
- the production Medousa loop, completion FSM, perception policy, and tool
  implementations.

Moving one of these implementations into a portable crate is allowed when it
is behavior-preserving and reduces the iOS compile boundary. Creating a second
implementation or source of truth is not.

## Target composition

```text
Medousa mobile UI (client)
    |
    | explicit local-root client capability
    v
iOS in-process medousa_daemon deployment
    |-- Stasis RuntimeComposition and control plane
    |-- existing authority/session/turn services
    |-- Locus + Surreal mobile persistence
    |-- production agent loop
    |-- filtered existing tool registry
    `-- Keychain-backed inference adapter

Paired full medousa_daemon
    ^
    | existing pairing + auth + signed transport
    | bounded context grant / task request / result provenance
    `---------------------------------------------
```

The UI may also select a paired workshop directly through the existing SDK and
Iroh transport. That remains a client-to-daemon connection. Transparent heavy
work delegation is a separate daemon-to-daemon operation and must not be
implemented in the UI.

## Identity and trust boundaries

- The embedded daemon has the stable Medousa authority derived from its local
  installation identity.
- The UI does not mint daemon authority, session, execution, job, or turn
  identities.
- Stasis owns cluster-node identity and orchestration correlation; Medousa owns
  conversation authority and provenance. Any mapping is metadata, not a new
  identity system.
- A local-root capability is admitted only by the co-located daemon bridge.
- Pairing a remote daemon grants only the capabilities represented by the
  existing authenticated relationship.
- A remote daemon rejects local-root claims received over network transports.
- Replica imports preserve session IDs, entry IDs, entry sequences, and content
  digests and fail closed on conflicts.

## Compile-boundary strategy

The first engineering problem is limiting what the daemon deployment compiles,
not inventing a smaller daemon API.

1. Characterize the production loop and retain its golden parity tests.
2. Move already-existing runtime logic behind portable crate/module boundaries
   only where target compilation requires it.
3. Gate host-only adapters such as process execution, desktop vault access,
   browser hosts, full Grapheme host support, delivery surfaces, and telemetry.
4. Build the mobile daemon from the same services with a restricted adapter and
   capability composition.
5. Measure the resolved iOS dependency graph at every milestone; a new crate is
   not considered portable merely because it has `default-features = false`.

Compile-time features choose available adapters. Runtime Stasis capabilities
remain the source of truth for what a deployed daemon advertises and accepts.

## Genuine integration gaps

The initial iOS deployment needs only the following new boundaries:

- Tauri lifecycle ownership for starting/stopping the in-process daemon;
- a local privileged client bridge to that daemon;
- explicit daemon-owned Keychain-backed inference credentials;
- iOS foreground/background cancellation and recovery policy;
- a mobile-safe registration filter over existing tools;
- target-specific feature gates and dependency checks.

Distributed heavy-work execution additionally requires:

- a bounded context manifest and authenticated task request;
- remote materialization of a derived worker session;
- result and provenance receipts;
- idempotent replica deltas with a high-water cursor.

These additions must attach to existing session, turn, mesh, and Stasis
contracts.

## Explicitly forbidden parallel architecture

Do not add:

- a client-owned session or turn store;
- a mobile-only node descriptor beside Stasis `ClusterNode`;
- a mobile runtime profile used as a second capability/control plane;
- a second turn registry, stream sequencer, replay model, or outcome store;
- an embedded identity file beside the daemon installation authority;
- copied Locus adapters or copied tool implementations;
- a bespoke daemon-to-daemon authentication path;
- client code that packages, schedules, executes, or commits daemon work.

## Recovery milestones

Each milestone lands as one reviewed commit after its acceptance gates pass.

### Milestone 1 — iOS qualification

- Pin the mobile-capable Stasis/Locus releases.
- Compile/link Stasis + Locus, lean Grapheme, `medousa-engine`, and Keychain for
  device and simulator targets.
- Preserve the strict Keychain round-trip diagnostic and startup/size baseline.
- Add the repeatable CI dependency gate.

### Milestone 2 — portable daemon runtime boundary

- Recover only behavior-preserving moves of the production loop, completion
  FSM, loop state, and policy.
- Keep daemon compatibility reexports while downstream imports migrate.
- Remove any parallel node/capability model from the extraction.
- Prove daemon golden-turn parity and measure the resulting iOS graph.

### Milestone 3 — mobile daemon adapters

- Add an explicit-credential AI adapter implementing the existing Stasis port.
  The iOS composition in Milestone 4 binds it to the daemon integration secret
  already stored in Keychain.
- Add capability-confined filesystem entry points useful to all daemon
  deployments.
- Express the mobile tool surface as an exact registration ceiling over
  existing implementations. Stasis capabilities and turn admission remain
  authoritative.
- Reuse the shared Stasis/Locus composition and apply compatibility fixes to
  its one semantic adapter.

### Milestone 4 — local iOS daemon turn

- Boot one embedded daemon from Tauri managed state.
- Grant the co-located UI local-root capability.
- Create a daemon-owned session and execute one production foreground turn.
- Persist/replay it through the existing session and turn stores.
- Verify suspend, cancellation, restart, and credential redaction behavior.

### Milestone 5 — authenticated heavy-work delegation

- Extend the existing mesh/task request and context-derivation contracts.
- Materialize a bounded worker session on the paired daemon.
- Execute using existing Stasis job/agent identity and return provenance.
- Prove remote peers cannot exercise local-root authority.

### Milestone 6 — replica deltas

- Export and import idempotent transcript deltas using existing identities.
- Track a high-water cursor and preserve the single-writer rule.
- Fail closed on digest/sequence conflicts and test interrupted synchronization.

## Acceptance gates

Every retained or new slice must answer all of these questions:

1. Which daemon-owned contract is authoritative?
2. Is this implementation reused, moved, or genuinely new?
3. Does any client become a second source of truth?
4. Does any network path assume local privilege?
5. Do desktop daemon behavior and golden-turn tests remain unchanged?
6. Does the slice compile for both iOS device and simulator targets?
7. Does it materially improve or preserve the measured mobile dependency graph?

If a change cannot pass those gates, it does not belong in the mobile daemon
recovery.
