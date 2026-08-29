# Daemon-owned OCI work environments

> **Status:** Active implementation — Phase 4 complete, ready for Phase 5
>
> **Date:** 2026-08-28
>
> **Stasis baseline:** `stasis-rs` 0.10.0
>
> **Related:** [iOS daemon parity recovery train](ios-embedded-daemon-plan.md),
> [durable turn workers](durable-turn-worker-plan.md),
> [turn runtime and lanes](turn-runtime-and-lanes.md),
> [Forge core](v0.7.0-forge-plan.md), and
> [Cursor: Git at any scale](https://cursor.com/blog/git-at-any-scale)

## Product promise

Medousa can place a durable job on any capable paired daemon, reconstruct the
exact work environment there, run it with the same agent and tool runtime, and
return durable, reviewable output. The receiving machine does not need the
project's toolchain installed on its host; it needs Medousa, an OCI-capable host
adapter, the declared resources, and access to the workload's durable inputs.

This is the foundation for delegated development environments. A coordinator
can split one feature into independent jobs, place them on several daemons,
collect commits and evidence, and reconcile the results without sharing a
database, filesystem, or live daemon process.

## Executive decision

There is one Medousa runtime and one source of runtime truth:

- the **Medousa daemon** owns agents, the tool FSM, tool registration, policy,
  credentials, lifecycle coordination, and durable delivery contracts;
- **Stasis** owns durable jobs, placement, leases, retries, recovery,
  provenance, and federated job/result exchange;
- **Forge** owns governed development work, execution leases, Git evidence,
  review, and disposition;
- the **OCI work environment** owns only the disposable execution locality:
  its workspace, processes, PTYs, toolchain, and build outputs; and
- **Git plus durable artifact storage** owns portable workload content.

The daemon does **not** run inside the work environment. The container is not a
second daemon, agent runtime, tool registry, database, or scheduler. It is a
daemon-owned adapter target for environment-dependent operations.

The environment contract attaches to a generic Stasis job. Agent and coding
jobs are its first consumers, not a special execution model baked into it.

```text
Stasis + daemon database
durable control state
        |
        | jobs, placement, leases, environment specs, provenance
        v
Medousa daemon
agent loop + tool FSM + registry + environment coordination
        |
        | materialize / execute / checkpoint / publish
        v
OCI work environment
normal local Git repo + filesystem + toolchain + processes
        |
        | commits, bundles, snapshots, evidence, artifacts
        v
Git remote / durable object storage
portable workload truth
```

## Why this boundary

Stasis already lets Medousa move daemon-defined jobs between daemons. The
missing guarantee is that a selected daemon has the dependencies and workspace
needed to execute a job. OCI solves that dependency and execution-locality gap;
it does not replace the runtime that already coordinates the job.

This also follows the useful boundary from Cursor's
[Git at any scale](https://cursor.com/blog/git-at-any-scale): keep ordinary Git
repositories on fast local disk, treat worker-local replicas as disposable,
store durable truth outside them, and atomically publish small control-plane
pointers only after immutable content is durable. Medousa does not need to copy
Cursor's Git storage system to adopt those invariants.

## Locked invariants

These are acceptance rules, not implementation suggestions.

1. **One runtime.** Agents, the tool FSM, registry, job handlers, policy, and
   database remain in the daemon for local, embedded, and remote execution.
2. **OCI is an execution adapter.** A container receives scoped operations and
   data; it never becomes a second Medousa authority.
3. **One tool catalog.** Environment execution changes adapters behind tools,
   not tool identity, descriptions, policy, or registration lists.
4. **No host dependency assumption.** Placement may depend on OCI support,
   architecture, accelerator, disk, memory, and policy. Project compilers,
   package managers, and libraries belong in the pinned image.
5. **No shared database or filesystem.** A destination daemon reconstructs
   from durable identifiers and content. Correctness never depends on mounting
   another daemon's workspace or database.
6. **Local Git stays local.** Active repositories live on fast storage local to
   the executing daemon. Network filesystems and per-object remote Git calls
   are not the workspace model.
7. **Local workspaces are disposable.** The loss of a container or daemon may
   cost work since the last durable checkpoint, but cannot invalidate an
   acknowledged completed job.
8. **Persist before success.** A job cannot complete until its required commit,
   checkpoint, evidence, and result artifacts are durably addressable.
9. **Publication is fenced and atomic.** Mutable refs or result pointers update
   with an expected prior value. A stale worker cannot publish over newer work.
10. **Conflicts are work.** An expected-base mismatch produces a typed conflict
    and, when policy allows, a reconciliation job. It never force-updates.
11. **Placement affinity is optional.** Reusing a warm image, repository, or
    environment improves latency but is never required for correctness.
12. **Handoff reconstructs; it does not teleport.** Initial migration recreates
    an environment from its spec and durable checkpoints. Live process-memory
    or container checkpoint migration is outside this epic.
13. **The daemon owns lifecycle.** Create, restore, start, exec, attach,
    checkpoint, stop, retain, and delete flow through one environment port.
14. **Adapters vary only at real host boundaries.** Linux, macOS, Windows, and
    future hosts may need different OCI adapters. Business logic does not fork
    by client or operating system.
15. **Untrusted output stays untrusted.** A successful process exit does not
    bypass Forge evidence, secret scanning, review, or publication policy.
16. **Work environments are job-agnostic.** Any admitted durable job may request
    an environment; environment lifecycle is not coupled to an AI-only handler.

## Terms and ownership

| Term | Owner | Meaning |
|---|---|---|
| Durable job | Stasis | Work identity, payload, requirements, placement, attempt, lease, retry, and terminal result |
| Delegation binding | Medousa daemon | Explicit authority to send bounded work to a particular peer |
| Work environment spec | Medousa domain | Portable declaration needed to materialize an execution locality |
| Work environment | Destination daemon | One locally materialized, daemon-owned OCI environment |
| Work environment handle | Environment adapter | Opaque capability used for scoped filesystem, process, exec, and PTY operations |
| Governed environment | Forge | User work, baseline, branch, evidence, review, and disposition across executor attempts |
| Job lease | Stasis | Which daemon/attempt may progress and complete the durable job |
| Execution lease | Forge | Which executor may mutate and seal the governed development attempt |
| Workspace checkpoint | Git/artifact store | Immutable content required to reconstruct or review work |
| Publication pointer | Git remote/control store | Small mutable ref advanced with expected-base compare-and-swap |

Stasis and Forge leases guard different things. The execution path carries both
when development work is governed. It must not invent a third independent work
ownership model. Environment operations are fenced by the active Stasis
attempt and, when present, the Forge environment/lease generation.

## Work environment contract

The domain contract is runtime-neutral. OCI-specific request types stay inside
the host adapter.

### `WorkEnvironmentSpec`

The initial contract contains:

| Field | Contract |
|---|---|
| `environment_id` | Stable logical environment identity, independent of the local container id |
| `workspace_id` | Stable governed workspace/work identity |
| `repository` | Repository identity and authorized origin, never ambient caller-controlled shell text |
| `base_commit` | Exact immutable commit used to materialize the workspace |
| `image` | OCI registry/repository reference, immutable digest, and declared platform; a mutable tag alone is invalid |
| `checkpoint_ref` | Optional immutable commit, bundle, snapshot, or artifact manifest used to resume |
| `requirements` | OCI capability, OS/architecture, CPU, memory, disk, accelerator, and policy requirements |
| `mounts` | Typed, least-privilege mounts; daemon database and host roots are never implicit |
| `network_policy` | Explicit egress/service policy rather than host-network inheritance |
| `secret_refs` | Opaque daemon-owned references, resolved only for the scoped execution |
| `fence` | Stasis attempt generation and optional Forge execution/environment generation |
| `publication` | Intended result/ref and expected current value for compare-and-swap |
| `retention` | Delete, retain-warm-until, or preserve-for-debug policy with a hard upper bound |

The spec contains references and policy, not raw database records, host paths,
plaintext secrets, image layers, or repository contents.

### `WorkEnvironmentPort`

The daemon-facing port must support these semantic operations:

```text
materialize(spec) -> WorkEnvironmentHandle
inspect(handle) -> WorkEnvironmentState
start(handle, fence)
exec(handle, request, fence) -> execution stream/result
attach_pty(handle, request, fence) -> attachment handle       # later phase
checkpoint(handle, policy, fence) -> immutable checkpoint ref
stop(handle, reason, fence)
release(handle, retention, fence)
```

Required properties:

- every mutating operation is idempotent or accepts an idempotency key;
- the opaque handle is scoped to one daemon and cannot be serialized as a
  portable container identity;
- stale fences fail before mutation or publication;
- inspection distinguishes absent, materializing, ready, running,
  checkpointing, stopped, failed, and released states;
- adapter errors preserve typed causes such as image unavailable, resource
  admission denied, checkpoint missing, stale fence, and runtime unavailable;
- cancellation stops the scoped execution without cancelling unrelated daemon
  work; and
- adapter absence is an advertised placement fact, not a reason to register a
  different agent or tool runtime.

### Tool routing

Tools remain registered exactly once. At turn admission, the daemon binds the
active environment handle into the execution context. Environment-dependent
ports resolve through that handle:

| Operation class | Runs where |
|---|---|
| Agent loop, tool FSM, model calls, memory, web, orchestration, Stasis, federation | Daemon |
| Filesystem and repository mutations for the delegated workspace | OCI environment |
| Shell, PTY, build, test, package manager, language server, code intelligence | OCI environment |
| Forge leases, evidence, review, disposition, result publication | Daemon, operating on environment-produced content |
| Client rendering and interactive controls | Home/TUI/other client |

No `register_oci_tools`, remote-only catalog, or container-side tool FSM may be
introduced. Existing tools gain or reuse environment-aware host ports. A tool
that needs an unavailable environment capability returns a typed unavailable
result or causes placement to choose another daemon.

### Deployment composition

An OCI adapter is an advertised daemon-host capability, not a client tier. A
desktop or server composition may provide it directly or through an optional
installed runtime. A mobile daemon without a compatible host adapter still
uses the same Stasis, agent, tool, and environment contracts: it declines local
placement and may delegate the job to an authorized capable daemon. Nothing in
Home decides or emulates the execution path.

## Durable content and publication

The daemon database and Stasis records store metadata, digests, provenance,
leases, and pointers. They do not store live worktrees, Git packfiles, image
layers, or arbitrary build directories.

The first implementation uses existing Git and Forge primitives wherever they
fit:

- exact base commits and checkpoint commits for tracked source state;
- Forge evidence and export bundles for governed, portable work;
- a durable artifact/blob port for untracked outputs, logs, manifests, and
  large generated assets; and
- expected-base Git ref updates or equivalent compare-and-swap publication for
  accepted results.

Every content object is written and digest-verified before its pointer is
published. Completion is ordered as follows:

```text
execute
  -> checkpoint source state
  -> persist evidence and required artifacts
  -> verify immutable digests are readable
  -> compare-and-swap result/ref pointer
  -> record durable Stasis result
  -> acknowledge job completion
```

If the daemon crashes after content publication but before Stasis completion,
the retry discovers the already-published digest/idempotency key and completes
without producing a second result. If compare-and-swap fails, the work remains
preserved and the job reports a typed publication conflict.

Event notification, gossip, and warm-cache catalogs may be lossy hints. Any
consumer opening a result verifies it against durable truth.

## Lifecycle and failure semantics

```text
place job
  -> acquire Stasis job lease
  -> acquire/verify Forge execution lease when governed
  -> materialize local Git repository
  -> create or restore OCI environment
  -> bind environment-aware tool ports
  -> run the normal Medousa job/agent loop
  -> checkpoint and persist outputs
  -> publish with expected-base CAS
  -> complete Stasis job
  -> retain warm or garbage-collect local environment
```

| Failure point | Required behavior |
|---|---|
| No capable daemon | Job remains placeable/deferred with named unmet requirements; it is not sent optimistically |
| Image pull/materialization fails | Retry according to Stasis policy; do not create a Forge result |
| Source daemon disappears before delivery | Federated Stasis retry/recovery preserves the job identity |
| Destination dies before checkpoint | Lease expires; replacement reconstructs from the last durable checkpoint |
| Destination dies after checkpoint | Replacement resumes from the immutable checkpoint and idempotency record |
| Stale daemon resumes | Fence rejection prevents environment mutation and publication |
| Artifact write succeeds, pointer update fails | Immutable artifact remains collectable; retry or reconciliation may reuse it |
| Expected base advanced | Preserve branch/bundle and schedule or request explicit reconciliation |
| Client disconnects | Daemon and Stasis continue; the client reconnects to durable state |
| Container cleanup fails | Job result stays valid; cleanup becomes separate maintenance work |

## Parallel delegated development

Parallel work is fan-out at the durable job layer, not several agents sharing a
mutable directory.

```text
coordinator job at base commit B
    +-> job A -> environment A -> commit A + evidence
    +-> job B -> environment B -> commit B + evidence
    +-> job C -> environment C -> commit C + evidence
                              |
                              v
                  reconciliation job at expected base B
                  -> validate / combine / test / publish with CAS
```

Each child receives the same immutable base or an explicitly declared
dependency checkpoint. Each owns a distinct environment, Forge attempt, Stasis
job identity, fence, and result. The parent consumes durable result references,
not local paths. Reconciliation is itself durable work and may run on any
capable daemon.

## Security boundary

The initial OCI adapter must default to:

- digest-pinned images and verified platform metadata;
- rootless/unprivileged execution where the host supports it;
- no host OCI socket, daemon database, Medousa data root, or ambient home
  directory mounted into the environment;
- typed, minimum-scope workspace and artifact mounts;
- explicit CPU, memory, process, disk, and execution-time limits;
- explicit network policy with host networking disabled by default;
- secret material resolved by the daemon at execution time, scoped to the job,
  excluded from specs/checkpoints/logs, and revoked after use;
- bounded logs and artifacts with content-type/size validation; and
- cleanup that cannot follow unresolved host paths or stale handles.

OCI isolation is a containment layer, not proof that produced code or artifacts
are safe. Forge governance and review remain in force.

## Phase 0 federation fit audit

Stasis 0.10 supplies the runtime-neutral federation vocabulary. Medousa's
existing delegated-turn path supplies the production transport and the
application-specific conversation behavior. They meet at the following seams:

| Concern | Stasis 0.10 owner | Current Medousa seam | Decision |
|---|---|---|---|
| Job identity and retry | `Job`, `NewJob`, attempts, leases, backoff | `DelegationService` enqueues `workflow.medousa.delegation` | Stasis remains authoritative; do not add another task identity |
| Placement | `PlacementConstraints`, `WorkerCapabilities` | Delegation currently targets one explicitly bound peer | Carry generic requirements on the job; capability-based destination selection starts with environment admission |
| Payload identity | `BlobDescriptor` on `RemoteJobEnvelope` | Bounded `DelegatedTaskRequest` crosses the signed mesh route inline | Move application bytes behind a blob adapter when remote job federation is wired; never put workspace contents in job records |
| Cross-runtime delivery | `FederatedDeliveryPort`, `FederatedIngressPort` | `DelegatedTaskTransport` resolves the paired route, authenticates it, and calls `/v1/mesh/tasks` | The Medousa mesh becomes the production adapter for the Stasis ports; it is not replaced by an in-memory bus |
| Signals and terminal results | `FederatedSignalEnvelope`, `FederatedTerminalResult` | Submit-or-observe returns signed `DelegatedTaskObservation` values and terminal agent envelopes | Migrate transport identity and terminal delivery to Stasis envelopes while preserving Medousa result content |
| Durable waiting | Stasis durable waits and agent-event ingress | `RuntimeDelegationWaitStore` maps the agent turn wait onto the runtime durable wait store | Retain the mapping until terminal federation can drive the existing ingress directly |
| Ownership transfer | `OwnershipHandoffStore`, resource leases, fencing tokens | No environment ownership is transferred today | Adopt this contract for environment handoff; do not invent a Medousa ownership table |
| Conversation authority | Outside generic job federation | Context grants, session derivation, transcript commits, and completion presentation | Remains Medousa-owned application behavior |

The audit found no production Stasis adapter that makes Medousa's current peer
transport removable. Stasis 0.10 includes public contracts and in-memory
reference implementations for federation, blobs, and ownership handoff; it
does not include Medousa pairing, LAN/Iroh routing, durable production blob
storage, or a reverse delivery endpoint for an embedded mobile origin.

Therefore Phase 0 removes only coordination that is actually superseded:

- fabricated STTP input/output identifiers are gone; provenance is optional
  and structured through `ProvenanceRef` only when a real source exists;
- every job carries Stasis placement constraints, defaulting honestly to
  unrestricted until a host capability is required; and
- every Stasis dependency and independent lock graph moves together to 0.10.

The polling driver is retained for the proven delegated-turn path. Replacing
it before a production `FederatedDeliveryPort` can deliver terminal results
back to mobile would remove durable behavior, not duplicate coordination. The
Phase 6 transport migration must delete that polling path in the same slice
that installs signed terminal delivery; it must not leave both paths active.

No local substitute is added for the remaining upstream-neutral gaps. Phase 1
defines only the Medousa-owned work-environment domain and ports. Production
federation, blob, and ownership adapters are introduced in the phases that can
exercise their complete lifecycle.

## Phased delivery

Each phase must leave a useful, testable boundary and must not pre-build the
next phase inside a temporary architecture.

### Phase 0 — Stasis 0.10 alignment and fit audit

**Goal:** consume the released generic durable-job/federation contracts and
name the exact Medousa seams before adding OCI code.

- Upgrade all Medousa Stasis pins and lockfiles together to `0.10.0`.
- Map existing delegation handler/payloads onto generic job requirements,
  placement, attempt/lease, provenance, signal, and result contracts.
- Remove Medousa-owned coordination code made redundant by Stasis 0.10 rather
  than wrapping it in another abstraction.
- Record any proven upstream gap before implementing a local substitute.
- Add contract tests for federated placement, lease expiry, idempotent terminal
  delivery, and no-shared-database recovery.

**Exit:** current daemon-to-daemon delegation passes on Stasis 0.10, and the OCI
epic depends only on public Stasis contracts.

### Phase 1 — Runtime-neutral environment domain and ports

**Goal:** establish the one adapter seam the rest of Medousa can depend on.

- Add validated `WorkEnvironmentSpec`, identity, fence, checkpoint, retention,
  state, and error types in the appropriate domain/type crates.
- Add `WorkEnvironmentPort` and durable content/materialization ports without
  importing an OCI client type into the agent or tool domain.
- Add an in-memory/fake adapter for lifecycle and fencing contract tests.
- Bind an optional environment handle into the existing turn/job execution
  context.
- Make capability advertisement distinguish `work_environment.oci` from
  project dependencies inside an image.

**Exit:** a normal job can be admitted with or without an environment handle;
the agent loop and catalog are identical in both cases.

### Phase 2 — One local OCI lifecycle adapter

**Goal:** prove daemon-owned materialization and execution on one supported host
using an existing OCI runtime.

- Select the initial runtime integration after a bounded spike; Medousa builds
  an orchestration adapter on OCI, not a container runtime from scratch.
- Implement digest-pinned image resolution, local repository materialization,
  create/start/inspect/exec/stop/release, resource admission, and bounded logs.
- Keep the repository and runtime storage on fast local disk.
- Reconcile orphaned local environments on daemon boot using durable leases and
  labels, preserving unknown work rather than deleting it.
- Use prebuilt images first; image authoring/build pipelines are not required
  for this phase.

**Exit:** a daemon can execute a fenced command inside a reproducible OCI
environment and safely reconcile it after restart.

**Landed boundary:** the full daemon now detects a Docker-compatible CLI host,
opens one `DockerCliWorkEnvironmentPort`, and advertises
`work_environment.oci` only while that adapter is available. It materializes an
exact Git commit on daemon-local storage, resolves the image as
`reference@sha256:digest`, applies CPU/memory/disk admission, creates a
deny-network container, and supports fenced create/start/inspect/exec/stop and
release. Command execution is admitted by the existing Forge execution service
with a one-hour timeout ceiling, a one-MiB combined output ceiling, and durable
idempotency receipts scoped to the complete Stasis/Forge fence.

Daemon boot reconciles only Medousa-labeled containers. Known records recover
their observed state; missing or corrupt records and unknown labeled containers
are reported and preserved. The adapter is absent from embedded composition,
and a missing Docker engine makes the daemon decline OCI placement without
changing its runtime or catalog. The real-engine conformance test proves pinned
Git/image materialization, scoped execution, restart reconstruction, stale-fence
rejection, unknown-container preservation, and explicit release. The initial
adapter consumes prebuilt development images with `/bin/sh`, supports only
deny-network policy and the workspace bind, and intentionally leaves PTY,
checkpoint restoration/publication, artifact/cache mounts, and allow-list
network enforcement to their named later phases.

### Phase 3 — Environment-aware tool adapters

**Goal:** run real Medousa work through the same catalog and FSM.

- Route workspace filesystem, Git, shell, build/test, package, and code
  intelligence operations through the bound environment handle.
- Preserve daemon-native memory, web, model, orchestration, federation, and UI
  tools outside the environment.
- Reject accidental host-path or host-shell fallback when a delegated
  environment is required.
- Carry both Stasis and Forge fences on mutating governed operations.
- Keep PTY attachment deferred; one-shot/streamed execution is sufficient to
  prove this phase.

**Exit:** the normal agent loop edits and verifies a repository whose toolchain
exists only in the OCI image, with no OCI-specific tool catalog.

**Landed boundary:** turn admission still binds one runtime-neutral
`WorkEnvironmentBinding`; the existing code store, general shell, Coder shell,
and code-intelligence tools now resolve that binding before selecting their
host adapter. The catalog ids, schemas, placement metadata, mode surfaces, and
tool FSM are unchanged. Filesystem paths are lexically confined to
`/workspace`, host absolute paths and parent traversal fail closed, and no
environment-bound operation falls back to the daemon host.

`code.read`, `code.search`, and optimistic-digest `code.write` execute through
the fenced environment port. Writes use bounded stdin and an atomic temporary
file inside the workspace. General and Coder one-shot shell execution use the
same port, so Git, builds, tests, package managers, and image-local toolchains
need no separate OCI tool registrations. Mutating execution requires the
active Stasis attempt plus both Forge generations. Every tool invocation gets
a turn-scoped idempotency identity; the Docker adapter includes that identity
and the complete fence in its durable receipt key.

Code-intelligence URIs are rebound from the governed daemon-local worktree to
`file:///workspace/...` and execute an image-provided `medousa-code query`
adapter. Missing image support is explicit and never proxies to the host.
Daemon-native model, memory, web, orchestration, federation, and UI operations
remain outside the environment. Shared PTY tools likewise return an explicit
unavailable result while attachment is deferred, rather than opening a host
PTY.

The real-engine conformance test now drives the production catalog adapters to
write and read a source file and verify it with the environment shell inside a
digest-pinned, deny-network Docker environment. Unit contracts prove path
confinement, bounded stdin, complete Stasis/Forge fencing, image-local code
intelligence, and fail-closed mutation. The unchanged Coder catalog contract
and embedded-daemon feature profile both compile and pass independently.

### Phase 4 — Durable checkpoints and atomic publication

**Goal:** make a local environment truly disposable.

- Reuse Forge checkpoint/evidence/export primitives for governed source work.
- Add the durable artifact/blob adapter needed for non-Git outputs.
- Verify content digests before publishing result pointers.
- Implement expected-base/CAS publication and typed conflict results.
- Add retention, compaction, and garbage-collection roots for images,
  repositories, bundles, checkpoints, and orphaned immutable artifacts.

**Exit:** after acknowledgment, deleting every source container and local
workspace still leaves enough durable data to inspect, resume, or reconcile the
job.

**Landed boundary:** the environment contract now carries a validated immutable
checkpoint descriptor rather than a loose provenance pointer. A checkpoint is
a small typed manifest that names its exact workspace/base/checkpoint commits,
complete Stasis/Forge fence, Forge-exported Git bundle, explicit non-Git
artifacts, creation time, and provenance. Forge creates a real checkpoint
commit and portable bundle; restore imports that bundle and checks out the
exact commit without requiring the original worktree or container.

The full daemon provides the first production Stasis `BlobTransferPort` over a
confined filesystem CAS. Bundles and artifacts stream through `StoreRoot`
capabilities, are addressed by SHA-256, and are re-read and digest-verified
before their descriptors can enter a manifest. Duplicate content compacts to
one object. Named checkpoint and publication roots carry bounded retention;
startup mark-and-sweep removes expired roots and old unreferenced objects while
preserving recent crash orphans. Environment release still owns disposable
container/worktree cleanup, while the digest-pinned image and authorized Git
origin remain independently reconstructible inputs rather than checkpoint
payloads.

Publication is a locked, atomic expected-value update in a separate confined
control store. Replaying the same checkpoint is idempotent; a different current
value returns a typed conflict containing the preserved losing checkpoint and
never overwrites the winner. Immutable content is rooted before acknowledgment,
and a retry after pointer publication repairs the permanent publication root
before returning the same result identity.

The real-engine conformance test checkpoints tracked and untracked source plus
an explicitly requested ignored artifact, publishes it, proves idempotent
replay and typed conflict, reopens the adapter, deletes the original container
and workspace, and reconstructs a fresh runnable environment from the losing
checkpoint. Focused contracts also prove portable Forge bundle restoration,
streamed CAS round-trips and deduplication, root-based garbage collection, and
cross-process-safe publication CAS. This phase intentionally lands a local
durable backend behind Stasis's public blob port; cross-daemon blob transport
is Phase 6, and Stasis-owned lifecycle sequencing is Phase 5.

### Phase 5 — Durable environment workflow

**Goal:** make environment lifecycle a resumable Stasis-coordinated workflow.

- Express materialize, execute, checkpoint, publish, and release as durable,
  idempotent boundaries under the parent job.
- Place only on daemons advertising compatible OCI/runtime resources.
- Persist environment spec, attempt/fence, phase, checkpoint refs, and
  provenance; persist no local container id as portable truth.
- Reconcile source/destination restarts, lease loss, cancellation, and timeout.
- Make cleanup independent so cleanup failure cannot corrupt job completion.

**Exit:** killing either daemon at every lifecycle boundary produces one
recoverable job and at most one published result.

### Phase 6 — Remote placement, handoff, and reconstruction

**Goal:** run the same environment-backed job on another paired daemon.

- Send the signed Stasis job plus environment/content references and resource
  requirements through the existing delegation path.
- Materialize independently on the destination; do not proxy every filesystem
  or Git operation to the source.
- Return signed result/checkpoint/artifact references with provenance.
- Prefer warm-compatible destinations only as a placement score.
- Prove retry and reconstruction when the chosen destination disappears.

**Exit:** a destination with no project toolchain installed on the host can run
and return the job using only its declared image and durable inputs.

### Phase 7 — Parallel development and reconciliation

**Goal:** support the three-independent-workers development flow end to end.

- Fan one coordinator plan into independently leased Stasis jobs and Forge
  attempts at exact bases.
- Collect immutable commits, evidence, and artifacts from each child.
- Add a durable reconciliation job that combines compatible results, runs
  verification, and publishes with expected-base CAS.
- Preserve every conflicting child result for review rather than choosing a
  winner silently.

**Exit:** three daemons can complete independent changes, lose all local
containers, and still reconcile the returned work on a fourth daemon.

### Phase 8 — Attachments, UX, and operations

**Goal:** make remote work observable and optionally interactive without moving
runtime ownership into the client.

- Expose environment phase, placement, checkpoint age, resource use,
  provenance, retry, conflict, and retention state through daemon APIs.
- Add bounded log streaming and later PTY attachment as capabilities of an
  existing execution, with reconnect and authorization.
- Let Home/TUI request, observe, cancel, retain, or release; clients never own
  environment lifecycle truth.
- Add operator controls for image policy, resource limits, storage pressure,
  network policy, and garbage collection.

**Exit:** users can understand where work ran, what it produced, why it moved or
failed, and whether a retained environment is consuming resources.

### Phase 9 — Hardening and additional host adapters

**Goal:** qualify the system beyond the first runtime/host pair.

- Add supported macOS, Windows, and Linux adapters only where host mechanics
  require them; reuse the same domain and lifecycle tests.
- Test hostile images, mount/path escape, secret leakage, decompression bombs,
  runaway processes, disk exhaustion, stale handles, and runtime compromise
  assumptions.
- Establish SLOs and quotas for image pull, cold/warm materialization,
  checkpoint, failover, cleanup, and disk high-water behavior.
- Add image provenance/signature policy and supply-chain evidence.

**Exit:** every supported adapter passes the same conformance, recovery,
security, and durability matrix.

## Verification matrix

The epic is not complete without automated proof of these cases:

| Contract | Proof |
|---|---|
| One runtime/catalog | Local and environment-backed jobs compile the same tool surface; no OCI registry exists |
| Dependency portability | Host lacks the project toolchain; pinned image job still builds/tests |
| Placement honesty | A daemon without OCI or required resources never accepts the job |
| Local-repo invariant | Active Git operations use destination-local storage |
| No shared state | Source and destination use separate databases and filesystems |
| Lease fencing | Expired source and replacement race; only current attempt mutates/publishes |
| Crash matrix | Kill before/after every lifecycle boundary; retry converges without duplicate completion |
| Persistence-before-success | Published job remains inspectable after all local workspaces are removed |
| CAS safety | Base advances before publication; result is preserved and conflict is explicit |
| Idempotent terminal path | Crash after publication before completion; retry returns the same result identity |
| Client independence | Disconnect/background Home; daemon job continues and reconnects from durable state |
| Parallel isolation | Three child jobs cannot see or mutate each other's workspace |
| Reconciliation portability | A separate daemon reconstructs and combines child results from durable refs |
| Security | No daemon DB/socket/home-root access; secrets absent from specs, logs, and checkpoints |
| Cleanup safety | Stale/unknown environments are preserved or quarantined until ownership is proven |

## Non-goals

- Running the Medousa daemon, agent loop, tool FSM, or Stasis runtime inside
  every workload container.
- Creating local, remote, mobile, or OCI-specific copies of tool registration
  and business logic.
- Building a new OCI runtime, Kubernetes replacement, or general cluster
  scheduler.
- Requiring Kubernetes, a shared filesystem, a shared database, or a permanent
  central coordinator.
- Building Cursor's Git hosting/WAL architecture before Medousa's measured
  workload requires it.
- Live migration of process memory, TCP sessions, or a running container.
- Treating PTY sharing as the coordination or durability protocol.
- Automatic merging, rebasing, or force-updating when independently produced
  work conflicts.
- Installing project dependencies directly onto destination hosts.
- Allowing clients to manufacture environment handles, leases, or publication
  authority.

## Open implementation choices

These choices are intentionally deferred behind locked contracts:

| Choice | Default until measured |
|---|---|
| Initial OCI backend | Docker-compatible CLI behind the full-daemon `WorkEnvironmentPort`; other hosts remain adapter choices |
| Image construction | Consume digest-pinned prebuilt images first; build service later |
| Durable blob backend | Local durable adapter for single-daemon proof plus an object-store-capable port for federation |
| Checkpoint form | Forge checkpoint commit/export bundle for source; typed artifact manifests for non-Git state |
| Warm retention | Bounded TTL and disk-pressure eviction; never part of correctness |
| Network access | Deny/default-minimum with explicit per-job policy |
| Secret injection | Runtime-scoped opaque references; no plaintext in Stasis payloads or environment specs |
| PTY | Attach to an already-owned execution after durable lifecycle is proven |

Any choice that changes an invariant above requires an explicit architecture
review. Backend selection within the port does not.

## Starting state and code anchors

| State | Evidence / starting seam |
|---|---|
| Upstream generic job foundation | `stasis-rs` 0.10.0 is published; the Phase 0 working tree upgrades every Medousa pin together |
| Remote turn delegation | `src/delegation.rs` owns the current waitable Stasis job, binding, recovery, and result delivery path |
| Signed bounded task transport | `src/delegated_task.rs` owns delegated request/result validation and execution provenance |
| Runtime composition | `src/runtime/stasis_wire.rs` and `src/runtime/platform.rs` register the canonical handlers |
| Runtime port seam | `crates/medousa-runtime/src/ports.rs` is the intended dependency direction for host capabilities |
| Governed Git work | `crates/medousa-forge/` owns environment generations, execution leases, checkpoints, evidence, and CAS dispositions |
| Tool execution binding | `src/agent_runtime/execution_context.rs` and Coder tool adapters are the current scoped-execution seams to extend, not duplicate |

Progress begins here:

- [x] Generic durable job/federation support published in Stasis 0.10.0.
- [x] Medousa daemon-to-daemon delegated turns proven on the current branch.
- [x] Phase 0a — upgrade Medousa and adopt optional structured provenance and placement.
- [x] Phase 0b — map federation ownership and remove only coordination superseded by Stasis.
- [x] Phase 1 — lock the runtime-neutral environment contract.
- [x] Phase 2 — land and prove one local OCI lifecycle adapter.
- [x] Phase 3 — route the existing tool catalog through bound environments.
- [ ] Phases 4–9 — implementation and qualification.

## Definition of done

This epic is complete when Medousa can durably place, reconstruct, execute,
checkpoint, publish, and reconcile governed work across independent daemons;
the destination host needs no project dependencies outside the pinned image;
clients can disconnect without owning the work; stale workers cannot publish;
and every acknowledged result survives loss of all participating local
containers and workspaces.

The end state is **placement flexibility with honest execution locality**:
Stasis decides and remembers the work, the daemon runs the one runtime, OCI
supplies the environment, and durable content can be reconstructed wherever the
next valid lease lands.
