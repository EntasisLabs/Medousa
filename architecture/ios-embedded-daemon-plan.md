# iOS daemon parity recovery train

> **Status:** Proposed plan lock — implementation paused pending review
>
> **First target:** iOS (`aarch64-apple-ios` and simulator)
>
> **Later targets:** Android, then browser WASM after the required Stasis updates
>
> **Date:** 2026-08-23
>
> **Baseline:** [ios-embedded-phase0-baseline.md](ios-embedded-phase0-baseline.md)

## Why this recovery train exists

The objective is not to create a mini daemon, a second mobile product, or a
reduced behavior model. The objective is to run the existing daemon correctly
inside a mobile host, using the same logic and contracts under a target-safe
feature composition.

The branch proved useful iOS compile, lifecycle, inference, and portability
work, but it also allowed the embedded composition to drift into a parallel
boot and premature delegation path. Four observed failures expose that drift:

1. Embedded boot reaches a delivery endpoint store before the canonical Stasis
   Surreal schema has created `delivery_endpoint`.
2. A remote health request returns HTTP 404 over Iroh even though the current
   full daemon declares `/v1/health`.
3. Chat history decoding reports a missing required `authority_id` even though
   the current response contract requires it.
4. Home can mix local embedded and remote workshop state or routing, allowing
   profiles, turns, and client state to cross workshop boundaries.

The table failure is a confirmed schema/bootstrap bug. The 404 and missing
field are contract/responder identity failures until proven otherwise; they
must not be papered over with optional fields or fallback routes.

This document supersedes the earlier milestone order in this file. Previous
commits are evidence and raw material, not architectural approval. They remain
subject to the keep/rework/remove audit below.

## Product definition

`medousa_daemon` is the product. Full, server, headless, embedded, mobile, and
eventually WASM are deployment and host compositions of that one product.

A fresh iPhone install boots its own daemon-backed **Personal workshop** in the
app sandbox. The phone can chat and perform all mobile-compatible work without
another Medousa daemon. AI inference may be remote through a ChatGPT account or
API credential.

The user may separately choose to:

- portal Home into a paired remote workshop; or
- grant the phone daemon permission to delegate selected heavy work to a
  particular paired daemon.

Neither choice is implied by pairing, by the existence of one remote workshop,
or by the other choice.

## Locked invariants

These are acceptance rules, not implementation suggestions.

1. **One daemon product.** All targets use the same daemon-owned authority,
   session, turn, persistence, memory, scheduling, and agent-loop logic.
2. **The daemon owns work.** Home, Tauri, and every other client may request,
   observe, cancel, or configure work within granted privileges; they do not
   create a second work engine or source of truth.
3. **Slim means a compile composition.** Feature gates remove incompatible host
   adapters and optional native workloads. They do not change common behavior,
   identity, persistence semantics, or wire contracts.
4. **Mobile is a complete workshop.** Its authority, profiles, sessions, turns,
   memory, notes, schedules, and storage are its own. A remote daemon is not its
   implicit backend.
5. **One active Home target.** Every daemon-owned Home operation resolves
   through the same active-workshop target: Personal uses the embedded daemon;
   a paired workshop uses only that remote daemon.
6. **Profiles are workshop-local.** A profile ID is meaningful only under its
   owning workshop authority. Home never passes a remote active profile into a
   Personal turn or combines profile catalogs.
7. **Pairing establishes trust, not intent.** Pairing does not switch the active
   workshop, connect on boot, poll an inactive daemon, or select a delegation
   target.
8. **Portal and delegation are independent.** Portal selection is a client
   routing choice. Delegation is an explicit daemon-owned binding and bounded
   capability grant. Both default to off for a newly paired relationship.
9. **Local root stays local.** The co-located Home bridge receives the embedded
   daemon's highest client privilege. No daemon-to-daemon transport may assert
   or inherit local-root privilege.
10. **One canonical schema path.** All deployments run the same ordered base
    schema/migration bootstrap before opening common stores. Optional native
    features may add versioned migrations, but may not redefine common tables.
11. **One contract vocabulary.** Direct in-process, HTTP, and Iroh adapters
    invoke the same operations and serialize the same DTOs. Transport absence
    is allowed; semantic drift is not.
12. **No silent storage substitution.** A configured persistent daemon fails
    boot with a typed diagnostic when schema or storage initialization fails.
    It does not silently become a file-backed or in-memory workshop.
13. **Mobile lifecycle changes availability, not truth.** iOS suspension may
    stop timers and active execution. Durable Stasis work, schedules, and turn
    state reconcile on wake/restart using existing recovery semantics.
14. **No new synchronization model without proof of a gap.** Existing Stasis
    control-plane identity plus Medousa authority, session sequence, derivation,
    forking, pairing, and daemon-to-daemon primitives are used first. A new
    replica/delta protocol requires a named failing contract test and a separate
    design review.

## Terms that must remain distinct

| Term | Owner | Meaning |
|---|---|---|
| Personal workshop | Phone daemon | The independent workshop rooted in the mobile app sandbox |
| Active workshop | Home client | The one daemon whose UI state and operations Home currently displays |
| Known workshop | Home client | A saved connection entry; it implies neither active routing nor delegation |
| Paired peer | Existing auth flow | A trusted remote identity and credentials with only granted capabilities |
| Portal | Home client | An intentional switch from Personal to a remote workshop |
| Delegation binding | Phone daemon | An explicit, revocable choice of peer and capability ceiling for heavy work |
| Deployment profile | Build/host | A feature and adapter composition of the same daemon product |
| User profile | Selected daemon | An identity lane nested inside exactly one workshop authority |

The topology is intentionally asymmetric:

```text
                         explicit portal selection
Medousa Home ----------------------------------------------+
    |                                                      |
    | Personal selected                                    v
    | local-root bridge                         paired remote daemon
    v                                                      |
phone medousa_daemon                                       | own authority,
    | own authority, profiles, sessions, turns             | profiles, data
    | own Surreal + Stasis + Locus data                     |
    |                                                      |
    +---- explicit delegation binding + existing auth -----+

Pairing alone creates neither arrow.
```

## Target capability composition

The shared daemon baseline is the default behavioral layer. Host and workload
features are additive around it.

### Shared across full and mobile deployments

- installation/workshop authority and user-profile registry;
- canonical Surreal bootstrap and daemon-owned persistence;
- Stasis runtime composition, durable jobs, schedules, recovery, and control
  plane;
- Locus memory and semantic indexing through the existing adapter;
- sessions, transcript sequencing, derivation/forking, turns, cancellation,
  replay, and recovery;
- the production async agent loop and turn-completion protocol;
- notes stored under the owning daemon's filesystem/sandbox authority;
- lean Grapheme and the mobile-compatible subset of existing daemon tools;
- inference ports and explicit credentials;
- generated daemon operation/DTO contracts;
- pairing, authentication, signed-envelope, and capability-grant primitives
  needed by an explicitly configured remote relationship.

### iOS host adapters

- in-process lifecycle ownership rather than a child process;
- a local-root Tauri client bridge rather than loopback HTTP;
- app-sandbox filesystem roots;
- Keychain-backed inference and pairing secrets;
- foreground/background/wake hooks that trigger daemon recovery;
- target-supported outbound transport for explicit portal or delegation use.

### Native full deployment additions

- Axum/Mio listeners and native server transports;
- PTY and process hosting;
- Forge, Coder, and Detamu workloads;
- full native Grapheme/host integrations;
- other process-heavy or desktop-only delivery/integration adapters.

These additions may advertise extra capabilities. They must not replace the
shared services with different implementations.

### Deferred WASM work

WASM is not an iOS acceptance gate. After iOS parity is proven and the needed
Stasis releases land, the next effort gates or replaces native listener,
filesystem, timer, and networking hosts such as Axum/Mio. It reuses the same
daemon logic and contracts; it does not begin another slim-daemon architecture.

## Compile and module boundary rule

The Rust library consumed by the daemon binary and mobile host must expose one
shared daemon composition. The repository's existing `medousa-sdk` name remains
the client SDK; this plan does not create a competing "core SDK" product.

Feature gates should sit at target-incompatible adapters or optional workload
registration. A shared service must not have separate `full-daemon` and
`embedded-daemon` implementations merely because the compositions enter from
different hosts. In particular:

- full and embedded boot call the same ordered bootstrap phases;
- schema and migration modules compile in every persistent deployment;
- transport adapters call common operation/service handlers;
- common DTO fields do not become optional by target;
- full builds are the shared baseline plus native capabilities;
- mobile builds are the shared baseline plus iOS adapters and without
  unsupported native workloads.

## Current branch recovery policy

Before adding behavior, compare the branch and working tree against the
pre-mobile baseline and classify each hunk. Do not discard the branch wholesale
and do not preserve code merely because it already landed.

### Keep

- behavior-preserving moves that let the existing daemon logic compile on iOS;
- target qualification, dependency fixes, and repeatable device/simulator
  gates;
- thin Tauri lifecycle, Keychain, sandbox, and local-root bridge adapters;
- changes that extend an existing daemon contract for a demonstrated mobile
  host gap;
- mobile-safe filtering that registers existing tools without copying their
  implementations;
- transport diagnostics and tests that do not introduce new routing authority.

### Rework onto the shared path

- the embedded bootstrap and store initialization;
- embedded direct commands that bypass common daemon operations;
- schema setup currently owned by more than one module;
- inference and tool composition that currently depends on a parallel turn
  owner;
- useful bounded delegation grants, provenance, or transport work that can be
  attached to existing Stasis/Medousa primitives after local parity is green.

### Remove or leave out of the build

- duplicate authority, identity, session, turn, stream, replay, schema, or
  capability models;
- a second embedded-only implementation of daemon business logic;
- auto-selection of the only paired portal as a delegation target;
- boot-time connection to a remote merely because Home knows it;
- Home code that packages, schedules, executes, or commits daemon work;
- remote-profile fallback during Personal turns;
- silent persistence fallback;
- any replica/synchronization protocol duplicating existing Stasis and Medousa
  primitives;
- PTY, Forge, Coder, Detamu, Axum/Mio listeners, or other unsupported native
  workloads in the mobile composition.

The modified iOS Live Activity `Info.plist` is deployment/user work outside this
train and must remain untouched.

## Phased recovery plan

Every phase lands as one reviewed commit only after its acceptance gates pass.
If a phase reveals a product-level choice or a missing upstream primitive, stop
at the boundary and amend this plan before proceeding.

### Phase 0 — Recover the branch to one-daemon boundaries

**Goal:** Remove architectural violations without throwing away useful mobile
qualification and adapter work.

Work:

- inventory committed changes since the pre-mobile baseline plus the entire
  uncommitted delegation diff;
- record each changed area as keep, rework, or remove under the policy above;
- make premature delegation dormant and default-unbound;
- remove automatic portal/delegation selection and any client-owned work path;
- preserve behavior-moving compile work and thin host adapters;
- add focused characterization tests for the four reported failures where the
  existing harness permits them;
- establish the exact full, embedded, iOS device, and iOS simulator build
  commands used by every later phase.

Gate:

- both full and embedded libraries compile;
- existing daemon golden-turn behavior remains green;
- no pairing or workshop-registry state can activate delegation;
- the diff ledger accounts for every branch-owned change;
- no unrelated user file enters the commit.

### Phase 1 — One boot pipeline and one canonical schema

**Goal:** Make every persistent deployment initialize the same daemon
prerequisites in the same order.

Work:

- extract or expose the existing full-daemon bootstrap phases as the shared
  daemon bootstrap; do not create a second generalized runtime;
- make full binary, headless/server hosts, and iOS in-process host call it;
- define one ordered, versioned base schema/migration manifest for common
  Medousa, Stasis, and Locus stores;
- run that manifest before resolving, reading, seeding, or registering any
  store, node, schedule, or delivery endpoint;
- remove duplicate ownership of delivery table definitions;
- make persistent boot failures consistent and fail closed across targets;
- expose a common schema revision for diagnostics.

Gate:

- a fresh full daemon and fresh embedded daemon create the same common schema;
- both reopen an existing database and apply upgrades idempotently;
- endpoint-enabled embedded boot cannot query `delivery_endpoint` before its
  migration;
- stale-lock recovery does not skip or partially apply schema work;
- a forced schema failure aborts both deployments rather than selecting a
  different store;
- common schema manifests/revisions match across feature builds.

This phase fixes the confirmed `delivery_endpoint` failure and the structural
cause that allowed schema behavior to diverge.

### Phase 2 — One operation contract across direct, HTTP, and Iroh surfaces

**Goal:** Make transport an adapter around a shared responder, with enough
identity to diagnose the process actually answering.

Work:

- route in-process, HTTP, and Iroh requests through the same daemon operations
  and generated DTOs;
- add a shared runtime descriptor containing at least workshop authority,
  product version/build revision, contract revision, base schema revision,
  deployment target, and advertised capabilities;
- return that descriptor from the direct health operation and `/v1/health`;
- make clients fail with a typed compatibility diagnostic when required
  contract identity is absent or incompatible;
- keep `authority_id` required in session/history coordinates and find the
  actual old/wrong responder instead of defaulting it;
- include HTTP method and path in Iroh transport errors without logging bearer
  tokens or response bodies containing secrets;
- verify generated route inventories against the full router.

Gate:

- direct, HTTP, and Iroh health calls decode the same descriptor fields;
- authenticated `/v1/health` over LAN and Iroh cannot return an unexplained 404;
- session creation/history DTOs round-trip across all supported transports with
  the same required `authority_id`;
- a stale or wrong daemon build is named by revision in the error;
- contract generation and documentation checks are green.

This phase resolves the reported health 404 and missing `authority_id` by
proving the responder and contract, not by adding fallbacks.

### Phase 3 — Strict workshop routing and state isolation in Home

**Goal:** Ensure Home is a client of exactly one selected daemon at a time and
never combines authorities.

Work:

- introduce one active-workshop resolver used by every daemon-owned Tauri/Home
  operation, including health, identity/profiles, sessions, turns, notes,
  memory, schedules, and capabilities;
- route Personal exclusively through the embedded local-root bridge;
- route a selected paired workshop exclusively through its authenticated remote
  transport;
- stop mobile development URL rewriting from mutating the Personal workshop;
- make switching atomic from the UI's perspective: stop old streams/effects,
  clear or park old scoped state, select the target, then load its state;
- key persisted client state such as last session, pins, drafts, promoted asks,
  and chat configuration by workshop ID and, where needed, authority;
- validate every selected profile against the active workshop before turn
  admission;
- keep the Home known-workshop registry separate from the phone daemon's
  delegation configuration.

Gate:

- Personal health/profile/session/turn tests make zero remote calls;
- paired-workshop tests make zero embedded calls;
- switching cannot render or submit with the previous workshop's profile,
  session, stream, or draft;
- merely adding or pairing a workshop starts no connection, polling, switch, or
  delegation;
- deleting/revoking a portal cannot damage Personal data;
- isolation tests use distinct authority/profile/session canaries.

### Phase 4 — Restore the complete mobile-compatible daemon surface

**Goal:** Make mobile a full independent workshop within the limits of its host,
not a chat-only substitute runtime.

Work:

- compare the pre-branch full daemon's shared services against the mobile
  composition and close only target-related gaps;
- compose the existing authority/profile, session/turn, Stasis, Locus, notes,
  lean Grapheme, scheduling, recovery, and async agent-loop implementations;
- register the existing mobile-compatible tools under an explicit capability
  ceiling;
- keep notes and file work within the phone daemon's sandbox authority;
- bind ChatGPT-account and/or API-key inference through the existing daemon
  inference port and Keychain-backed credentials;
- gate unsupported host integrations at their adapters/registrations rather
  than gating shared daemon logic behind `full-daemon`.

Gate:

- with no paired workshop configured, an iPhone can create a local profile,
  start/reopen a session, complete an inferred turn, replay it, use memory, and
  create/read a note;
- a mobile-safe Grapheme tool runs through the production turn path;
- the descriptor advertises the same common capabilities and only truthful
  host-specific differences;
- PTY, Forge, Coder, Detamu, and native listener graphs do not enter the mobile
  dependency closure;
- the full daemon still uses the same shared logic and retains its additional
  capabilities.

### Phase 5 — iOS lifecycle, schedules, and recovery hardening

**Goal:** Make daemon truth survive real mobile lifecycle behavior.

Work:

- treat suspend as loss of execution time, not loss of durable state;
- leave live turns under runtime/OS ownership when Home backgrounds; app
  lifecycle must not synthesize cancellation or checkpoint outcomes;
- let process suspension pause/resume the live owner naturally; after process
  termination, use existing Stasis lease recovery for durable work and the
  existing turn journal for idempotent timeline reattachment;
- on wake/restart, run the existing Stasis recovery and schedule reconciliation
  path before admitting conflicting work;
- define and test catch-up/coalescing behavior using Stasis policies rather than
  an always-running mobile timer;
- verify same-root reopen, Keychain credential redaction, schema upgrade, memory
  reopen, and turn reattachment;
- measure startup, memory, binary size, thermal behavior, and cancellation on a
  physical device as well as simulator.

Gate:

- a scheduled item persisted before suspension is recovered according to its
  policy after wake/restart;
- interrupted turns do not duplicate transcript sequence or terminal events;
- restart reopens the same authority, profiles, notes, sessions, memory, and
  Stasis work;
- credentials never enter Surreal, logs, events, or crash diagnostics;
- iOS device and simulator gates are repeatable.

At this point the independent mobile product goal is complete.

### Phase 6 — Explicit remote portal and heavy-work delegation

**Goal:** Add optional distributed compute without changing workshop ownership
or reimplementing existing distributed primitives.

Work:

- keep portal selection as the existing explicit Home workshop switch;
- add a separate daemon-owned, revocable delegation binding whose default is
  `None` and whose target is an exact paired peer identity;
- require an explicit user action to create/change that binding and an existing
  authenticated capability grant to use it;
- reuse Stasis job/agent/control-plane identity and existing Medousa session
  derivation, sequence, fork, turn, pairing, auth, and signed transport flows;
- audit the current uncommitted task/grant/provenance work and retain only the
  fields or adapters that bridge a demonstrated gap;
- derive a bounded remote worker context and return a result/provenance receipt
  to the initiating daemon-owned turn;
- keep both workshop stores independent; delegation does not import remote
  profiles, switch Home, or merge session catalogs.

Gate:

- pairing alone leaves delegation unset and emits no task traffic;
- selecting a remote portal does not bind it for delegation;
- binding a peer does not switch Home away from Personal;
- multiple paired peers never trigger heuristic or "only peer" selection;
- remote peers cannot assert local-root privilege or exceed the granted tool,
  context, deadline, and resource ceiling;
- retries are idempotent under existing identities;
- the initiating daemon records returned provenance without creating a second
  transcript owner.

### Phase 7 — Release parity and future-target handoff

**Goal:** Prove the architecture as a supported product path and leave a clean
boundary for Android/WASM.

Work:

- run repository CI parity plus target compile/dependency checks;
- add cross-feature schema, contract, and capability-manifest diff gates;
- document the independent Personal workflow, explicit portal workflow, and
  explicit delegation workflow;
- record measured iOS dependency and runtime budgets against the Phase 0
  baseline;
- inventory remaining native host assumptions for Android and WASM without
  implementing a new runtime model.

Gate:

- full and mobile deployments pass the same common behavior suite;
- schema/contract drift fails CI before packaging;
- user documentation never implies that pairing, portal, or delegation are the
  same action;
- WASM follow-on work is limited to named host adapters and upstream readiness.

## Error-to-phase map

| Observed issue | Owning phase | Required proof |
|---|---|---|
| Missing `delivery_endpoint` table | Phase 1 | Same ordered schema bootstrap before endpoint access |
| `/v1/health` 404 over Iroh | Phase 2 | Shared operation + method/path/build-aware transport test |
| Missing `authority_id` while loading chat | Phase 2 | Required cross-transport DTO round-trip and responder revision |
| Profiles/turns/state may mix across workshops | Phase 3 | Single target resolver and distinct-authority isolation tests |
| Remote chosen without explicit intent | Phase 0 and Phase 6 | Default-unbound behavior and separate portal/delegation tests |

## Cross-phase verification matrix

The exact commands are recorded in Phase 0 and then kept stable. The matrix
must cover:

| Dimension | Required variants |
|---|---|
| Rust composition | full daemon; embedded daemon without default full features |
| Apple target | iOS device; arm64 simulator |
| Database state | fresh; existing current; prior revision; failed/locked reopen |
| Transport | in-process direct; authenticated HTTP/LAN; authenticated Iroh |
| Workshop | Personal only; one paired inactive; paired active; multiple paired |
| Identity | distinct authority, profile, session, and turn canaries |
| Lifecycle | first boot; warm reopen; suspend/wake; killed/restart |
| Capability | common mobile-safe surface; full native additions; denied remote root |

Tests that rely on optional fields, silent fallback, sleeps, shared global
state, or one-process assumptions do not count as parity proof.

## Commit and review cadence

- The approved plan itself may land as the plan-lock commit.
- Each numbered phase is one cohesive commit after its gates pass.
- Do not mix phases merely because nearby files overlap.
- Before each commit, show the scoped diff, tests run, and retained known risks.
- Never include unrelated worktree changes.
- Prefer corrective commits over destructive history rewriting unless the user
  explicitly requests a rebase.
- If a phase fails an invariant, stop and revise the plan; do not compensate in
  a later phase with another abstraction.

## Final definition of done

The recovery is complete when an iPhone, with no paired Medousa daemon, boots
its own workshop and uses the same daemon logic to manage profiles, sessions,
turns, memory, notes, schedules, recovery, tools, and persisted state while
using remote AI inference only as configured.

Home can intentionally portal to another workshop without contaminating either
workshop. Separately, the phone daemon can intentionally delegate bounded heavy
work to an exact authenticated peer without assuming privilege, switching the
UI, merging profiles, or inventing new turn/replica ownership.

Full desktop/server builds remain that same daemon with additional native host
capabilities. Future WASM work changes adapters and feature closure—not the
product or its behavioral contracts.
