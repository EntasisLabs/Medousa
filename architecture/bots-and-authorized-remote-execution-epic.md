# Bots and authorized remote execution

> **Status:** Phases 1–4 complete — phases 5–7 planned
>
> **Date:** 2026-09-03
>
> **Related:** [turn runtime and lanes](turn-runtime-and-lanes.md),
> [identity manuscripts and recall](identity-manuscripts-and-recall-plan.md),
> [peer mesh](v0.6.0-peer-mesh-plan.md),
> [durable turn workers](durable-turn-worker-plan.md),
> [Forge](v0.7.0-forge-plan.md), and
> [daemon-owned OCI work environments](daemon-owned-oci-work-environments-epic.md)

## Product promise

Medousa supports two related experiences without inventing a second agent
runtime:

1. A **Bot** is a named, durable teammate. It combines a Specialist, a stable
   memory scope, defaults, and one or more conversations so the user experiences
   continuity instead of repeatedly assembling turn settings.
2. **Remote execution** lets a user or an authorized agent choose which
   connected workshop performs delegated or coding work. The selected workshop
   remains the authority for its own shell, files, projects, credentials, and
   execution policy.

Bots describe **who** is helping. Modes describe **how** a turn is handled.
Workshops describe **where** work runs. Work environments describe **inside
what reproducible workspace** environment-dependent operations run.

The complete product sentence is:

> Ask a durable teammate to handle a turn in a chosen mode, on an authorized
> workshop, optionally inside a reproducible work environment.

## Executive decisions

These decisions are locked for this epic:

1. **Bot is not a new agent mode.** General, Teacher, Instant, and Coder remain
   turn modes. A Bot may use any mode the runtime admits.
2. **Bot is not a renamed Specialist.** A Specialist remains reusable declared
   expertise. A Bot adds durable identity, memory continuity, and conversation
   ownership around one or more Specialists.
3. **Bot is not permanently identical to one session.** The first release may
   give each Bot one primary session, but the domain stores an explicit Bot id
   and Bot-to-session binding so fresh conversations, groups, and duplication
   remain possible.
4. **Execution target and work environment are separate.** The execution
   target selects the daemon. The optional work-environment specification
   selects the repository, checkpoint, OCI image, resources, and execution
   locality owned by that daemon.
5. **Placement does not grant authority.** Selecting a capable daemon is only a
   routing decision. The destination daemon independently admits or rejects the
   requested work.
6. **Permissions are directional and daemon-owned.** A policy stored on daemon
   A controls what peer B may ask A to do. It says nothing about what A may ask
   B to do.
7. **Pairing does not imply execution access.** Existing pairing and mesh
   transport grants establish identity and protocol access. Shell, coding,
   filesystem, network, and secret access require additional explicit policy.
8. **The destination enforces.** Home presents controls and sends
   administration requests, but it never becomes the authority that evaluates
   remote execution permissions.
9. **Coder has one execution authority at a time.** Project discovery, Forge
   leases, files, shell, PTY, builds, and evidence for a Coder undertaking all
   belong to the same selected workshop and work environment.
10. **No silent relocation of mutable work.** Automatic retry may move safe,
    stateless work. Stateful or mutating work moves only from a durable,
    fenced checkpoint that can be reconstructed exactly.

## Product vocabulary

| Product term | Domain meaning | Owns |
|---|---|---|
| Mode | Turn handling strategy | Prompt/context depth and foreground lane behavior |
| Specialist | Identity Manuscript | Declared expertise, prompts, tools, worker defaults, delivery |
| Bot | Durable teammate profile | Name, role, avatar, Specialist binding, memory scope, conversation bindings |
| Session | Conversation | Transcript and episodic turn history |
| Workshop | Medousa daemon surfaced to a client | Local authority, policy, credentials, runtime, projects |
| Peer | Another paired daemon or portal identity | Authenticated remote identity |
| Execution target | Requested and resolved workshop placement | Which daemon owns the worker loop |
| Work environment | Optional daemon-owned execution locality | Repo, checkout, OCI image, processes, PTY, checkpoint |
| Peer execution policy | Directional destination policy | What one peer may request from this workshop |
| Task execution grant | Bounded signed request authority | What one admitted job may do |

## Existing foundations

This epic composes and extends shipped systems:

- Identity Manuscripts already declare persona, prompts, identity pins, worker
  routing, tool allowlists, Locus hints, delivery, schedules, and OpenShell
  configuration.
- Interactive turns already accept mode, model and stage routing, one primary
  manuscript, additional manuscripts, voice, tool constraints, session id, and
  Coder context.
- Locus already scopes episodic memory by workshop profile and chat session.
- Durable turn workers already persist intent, task, result, routing,
  manuscript, parent session, Coder work id, and continuity state.
- Mesh already authenticates peers with signed, expiring envelopes, monotonic
  sequence numbers, payload hashes, and capability grants.
- Remote delegated turns already create a durable Stasis job, transport a
  bounded context grant, execute on a receiving daemon, and return a signed
  result.
- Stasis placement already supports required capabilities, platform,
  architecture, region, and an exact target node.
- OCI work-environment federation already discovers capable paired daemons,
  reconstructs portable inputs, runs fenced work, and returns durable results.
- Forge already owns governed project state, execution leases, evidence,
  checkpoints, review, and disposition.

The current gaps are equally specific:

- Home selects a Specialist as client state; the daemon does not persist a
  durable Bot-to-session identity.
- Workshop spawn has no placement request.
- Turn work records do not preserve requested and resolved execution authority.
- Remote delegation uses one default binding rather than a per-spawn target.
- The current remote adapter accepts only reduced worker intents and rejects
  manuscript, stage, and model overrides.
- Remote admitted workers deliberately receive a safe web-and-utility tool
  ceiling, not shell or Coder authority.
- Peer mesh grants are coarse protocol grants; there is no structured,
  user-editable per-peer execution policy.
- Coder project selection assumes one workshop authority and does not expose
  that authority as an explicit choice.

## Target architecture

~~~text
BotProfile
  | specialist + bot memory + defaults
  v
BotSessionBinding -----> Session transcript
  |
  | resolved at turn admission
  v
Turn / WorkerSpawnSpec
  | mode + task + model + tools
  | requested execution target
  | optional work-environment requirements
  v
Execution router
  | capability match is not authorization
  v
Destination admission
  | mesh transport grant
  | AND peer execution policy
  | AND task execution grant
  | AND destination runtime policy
  | AND Specialist/tool ceiling
  | AND Forge/environment lease when applicable
  v
Selected workshop daemon
  | normal Medousa worker loop
  | optional daemon-owned work environment
  v
Signed result + durable provenance
  v
Origin session and Bot
~~~

There remains one agent runtime and one tool catalog. Local and remote
execution choose adapters and authority beneath the same semantic turn and
worker contracts.

## Locked domain contracts

The exact Rust layout may follow existing module conventions, but the semantic
fields and ownership below are required.

### Bot profile

~~~text
BotProfile {
  bot_id
  display_name
  role_description
  avatar_ref?
  primary_manuscript_id
  additional_manuscript_ids[]
  memory_scope_id
  default_mode?
  primary_session_id?
  archived
  revision
  created_at
  updated_at
}

BotSessionBinding {
  bot_id
  session_id
  kind: primary | secondary
  bot_revision_at_bind
  created_at
}
~~~

Rules:

- Bot ids, memory scopes, and bindings are daemon-owned durable identities.
- The profile references Manuscripts rather than copying their prompts and tool
  policies.
- Stable behavior belongs in the Bot profile or referenced Manuscript.
  Task-specific instructions remain conversation messages.
- A Bot may suggest defaults, but it cannot carry or grant workshop authority.
- A Bot profile revision is recorded for provenance. Existing transcripts are
  not rewritten when the profile changes.
- Duplicate copies profile configuration and Specialist references, but creates
  a new Bot id, memory scope, and primary session. It does not copy learned
  memory or transcript history.
- Archiving a Bot does not delete its sessions or memory.
- Existing unbound sessions continue as ordinary Medousa sessions.

### Memory layers

Bot continuity composes existing memory rather than replacing it:

| Layer | Scope | Purpose |
|---|---|---|
| User identity graph | User/profile | Stable people, preferences, and world facts |
| Bot memory | Bot id | Durable working relationship, recurring context, Bot-specific learning |
| Session memory | Session id | Episodic conversation trail |
| Turn scratch | Turn/work id | Temporary reasoning and execution state |

The first implementation may use the primary Bot session as the physical Locus
scope, but public/domain contracts use Bot identity so a later second session
does not fork the Bot accidentally.

Memory is context, not authority. A remembered statement cannot grant tools,
choose a secret, relax policy, or prove a changing consequential fact.

### Execution placement

~~~text
ExecutionTargetSelection =
  SameAsParent
  | Exact { runtime_id }
  | Auto { requirements }

ExecutionPlacementResolution {
  requested
  resolved_runtime_id
  resolution_reason
  resolved_at
}
~~~

Rules:

- SameAsParent is the compatibility default for omitted placement.
- Home workshop or route ids are resolved at the client/API boundary into a
  stable daemon runtime identity. Raw URLs are not execution identities.
- Exact means no fallback to a different daemon.
- Auto chooses only from reachable, capable, authorized candidates.
- Placement requirements reuse Stasis concepts rather than creating another
  scheduler vocabulary.
- Capability advertisement answers whether a daemon can perform a class of
  work. It does not answer whether this caller may request it.
- The requested selection and resolved runtime are persisted on every durable
  worker and included in result provenance.
- Child workers default to SameAsParent unless the parent or user supplies an
  admitted override.

### Work environment

Execution placement answers **which daemon**. Work-environment requirements
answer **which reproducible execution locality inside that daemon**.

Normal research and conversational workers may need only an execution target.
Coder, builds, tests, and portable mutable work may also carry the existing
WorkEnvironmentSpec or a durable reference to one.

Neither field may be named only environment in a public contract because that
would make daemon placement and OCI/workspace materialization ambiguous.

### Peer execution policy

Transport capability strings remain the coarse gate for signed mesh message
types. A separate structured policy stores workload authority:

~~~text
PeerExecutionPolicy {
  peer_device_id
  enabled
  assistant_work
  sandbox_execution
  host_shell
  coder_work
  work_environment_materialization
  allowed_project_ids[]
  allowed_root_refs[]
  allowed_tool_domains[]
  allowed_mcp_server_ids[]
  allowed_secret_refs[]
  network_policy
  allow_agent_targeting
  expires_at?
  revision
  created_at
  updated_at
}
~~~

The persisted representation may normalize these fields into typed scopes.
The product may expose calm presets, but presets compile to explicit scopes and
are never an alternate enforcement path.

Recommended initial presets:

| Preset | Meaning |
|---|---|
| Connected only | Messaging and synchronization; no remote work admission |
| Assistant work | Bounded research/general workers with safe daemon tools |
| Sandboxed work | Assistant work plus command execution inside an enforced sandbox |
| Approved projects | Forge/Coder access only to selected project identities |
| Custom | Individually managed scopes |

Host shell is an advanced independent permission. A generic shell cannot be
made read-only by classifying command text. Any read-only experience must use
typed diagnostic operations or an OS/container boundary with read-only mounts.

Permissions are edited from the active workshop:

> When Home is connected to workshop A, Settings → Connection edits policies
> stored and enforced by A for peers connected to A.

For each peer, Home presents:

- **Allowed on this workshop** — editable inbound policy owned by the active
  daemon.
- **Available on that workshop** — read-only capabilities and effective
  permissions advertised by the peer.

Editing another daemon's inbound policy requires connecting to that daemon and
having its execution-administration authority. A portal cannot mutate policies
on a daemon merely because both are visible in Home.

### Task execution grant

Every remote job carries a signed, bounded grant derived from its admitted
worker specification. At minimum it binds:

- origin and destination runtime identities;
- parent session, Bot id when present, work id, and correlation id;
- worker intent and requested tool domains;
- optional project/work-environment identity;
- issued-at, expiry, and idempotency identity;
- policy revision used at admission; and
- resource, network, secret, and publication constraints when applicable.

The effective authority is always:

~~~text
mesh transport grant
  INTERSECT peer execution policy
  INTERSECT task execution grant
  INTERSECT destination runtime policy
  INTERSECT mode, worker-intent, and Specialist tool policy
  INTERSECT Forge and work-environment lease when applicable
~~~

No field in a Bot profile, chat message, model output, placement selection, or
capability advertisement participates as an authority grant.

### Revocation

- Disabling a peer or reducing its policy blocks new admissions immediately.
- Every policy change increments its revision and emits an audit event.
- Active work receives cancellation when a removed scope is required.
- Fences prevent a revoked stale attempt from publishing further mutable
  results.
- Already completed durable results remain visible with their original policy
  revision and provenance.
- Revocation cannot undo mutations that completed before it; the UI says this
  plainly.

## User experience

### Bots

The user creates a Bot by choosing a name, job, avatar, and Specialist. Opening
the Bot opens its primary conversation. The composer continues to show and
change the turn mode independently.

The implementation may initially place Bots beside chats or in a compact Bots
section. The UI must not expose the internal phrase manuscript plus Locus
session.

### Manual execution target

Worker and Coder surfaces use human workshop names:

~~~text
Run on
  This Mac
  Mac mini
  Studio PC
  Auto
~~~

Every active or completed remote item visibly reports its execution workshop.
Errors name the requested workshop and distinguish offline, incapable,
unauthorized, expired, missing-project, and execution failures.

### Agent-selected execution

Agent selection is opt-in per destination peer. The agent receives an
authorized inventory containing opaque runtime ids, labels, availability, and
usable capability classes. It never receives connection credentials, private
addresses, or denied capabilities.

The agent may select:

- an exact authorized runtime from that inventory;
- SameAsParent; or
- Auto with declared requirements.

The destination still performs admission. Discovery is not a grant.

### Remote Coder

Coder chooses execution authority before project:

~~~text
Run on: Mac mini
Project: Medousa
Workspace: Current checkout | Isolated copy
~~~

Project identity includes its owning runtime and stable Forge/repository id.
A path alone is never a portable project identity.

After an undertaking begins, its execution authority is sticky. Switching
workshops starts a new undertaking or an explicit portable handoff. It does not
retarget the existing shell or lease silently.

## Seven implementation phases

Each phase is a reviewable merge unit with an observable exit test. A phase may
contain a small stack of atomic commits, but no commit spans phases and no phase
lands a client control before daemon enforcement exists.

### Phase 1 — Durable Bots ✅

**Outcome:** A named Bot reliably resolves the same Specialist and memory
continuity from any client connected to its workshop.

Implementation:

- Add versioned BotProfile and BotSessionBinding domain records and a
  daemon-owned store.
- Add daemon APIs for list, create, read, update, archive, duplicate, open, and
  bind session.
- Resolve Bot identity at turn admission before prompt preparation.
- Feed the resolved primary and additional Manuscripts through the existing
  manuscript pipeline.
- Bind Bot memory through the existing Locus/identity machinery without adding
  a second memory engine.
- Persist Bot id and profile revision in turn provenance.
- Add the Home Bot creation, selection, and conversation surface.
- Preserve ordinary sessions and current Specialist selection.

Acceptance:

- Reopening Home or connecting a second Home returns to the same Bot and
  primary conversation.
- The Bot retains Bot-scoped context while switching between General and
  Teacher.
- Duplicating a Bot copies configuration but not transcript or learned memory.
- Archiving a Bot preserves its underlying durable data.
- A Bot cannot add tools or permissions beyond the admitted mode and
  Specialist intersection.

Suggested commit boundary:

- feat(bots): persist profiles and bind durable conversations
- feat(runtime): resolve bot identity at turn admission
- feat(home): add calm bot creation and selection

### Phase 2 — Shared execution placement contract ✅

**Outcome:** Every worker can express where it should run, and every durable
record says where it actually ran.

Implementation:

- Add optional ExecutionTargetSelection to WorkshopSpawn and the internal
  worker spawn command.
- Normalize client workshop references to daemon runtime ids at one boundary.
- Add requested placement, resolved runtime, resolution reason, and parent
  runtime to TurnWorkRecord and worker result projections.
- Reuse Stasis PlacementConstraints for internal matching and exact
  target-node placement.
- Introduce one execution router interface used by local and remote
  implementations.
- Treat omitted placement as SameAsParent.
- Adapt the existing single delegation binding into an ingress default during
  migration rather than keeping it inside the execution adapter.
- Return typed unavailable and unsupported-target errors before execution.

This phase does not expand remote tool authority or claim remote worker parity.

Acceptance:

- Existing local worker spawning behaves identically when target is omitted.
- An exact local target resolves deterministically.
- An exact unavailable remote target fails without falling back.
- Parent and child provenance agree on requested and resolved targets.
- Old work records deserialize safely with local/unknown provenance defaults.

Suggested commit boundary:

- feat(runtime): carry explicit worker placement and provenance
- refactor(workshop): route execution through one target resolver

### Phase 3 — Per-peer daemon permissions ✅

**Outcome:** The owner of each workshop explicitly controls what every paired
peer may request from it.

Implementation:

- Add a versioned daemon-owned PeerExecutionPolicy store keyed by peer device
  identity.
- Keep mesh envelope capabilities as the coarse protocol gate; evaluate the
  structured execution policy during task admission.
- Add administration APIs guarded by the daemon's existing execution
  administration authority.
- Add Settings → Connection controls for each peer on desktop and mobile.
- Show simple presets first and expandable scopes second.
- Distinguish editable inbound policy from read-only remote capability and
  permission status.
- Add policy revision, audit events, expiry, revoke, and typed denial reasons.
- Compile each admitted task into a bounded execution grant.
- Default new and legacy peers to no shell, no Coder, no secret access, and no
  agent-selected routing.
- Treat an existing task.request grant as legacy Assistant work only; it never
  upgrades to shell or Coder implicitly.

Acceptance:

- A may authorize B without B authorizing A.
- Changing A's policy while connected to A does not write B's local policy.
- A denied request fails before a worker, shell, environment, or Forge lease is
  created.
- Revocation prevents new work and fences further publication from affected
  active attempts.
- Reinstalling or disconnecting Home does not erase daemon-owned policy.
- Audit output identifies peer, scope, policy revision, decision, and work id
  without leaking secrets.

Suggested commit boundary:

- feat(mesh): persist and enforce directional peer execution policy
- feat(api): expose peer execution administration
- feat(home): manage connected peer permissions

Landed boundary:

- Destination-owned, pairing-bound policies now compile every admitted remote
  task into an expiring signed execution grant.
- Native administrator routes expose policy read, update, presets, expiry, and
  audit without granting browsers peer-administration authority.
- Settings → Connection presents calm per-peer presets with optional advanced
  scopes, while pairing removal revokes policy and active delegated work.

### Phase 4 — Remote worker contract parity ✅

**Outcome:** A remotely placed worker carries the same semantic worker
specification and lifecycle as a local worker; policy alone determines its
effective tools.

Implementation:

- Replace the reduced remote payload with a versioned WorkerSpawnSpec carrying
  intent, task, acknowledgement, manuscript ids, stage role, model hint,
  parent mode/context, placement, and bounded tool request.
- Remove the remote adapter's hard rejection of manuscript, stage, and model
  fields.
- Preserve Bot, session, parent, route, model, and target provenance across the
  signed request and result.
- Route local and remote execution through the shared execution router.
- Intersect requested tools with the Phase 3 policy and all destination
  ceilings.
- Bring status, cancellation, durable completion, and steering semantics to
  parity where the canonical worker lifecycle supports them.
- Keep safe legacy remote requests working through an explicit compatibility
  decoder.

Contract parity is not permission parity. A destination may still deny a valid
worker specification or narrow it to a smaller tool set.

Acceptance:

- The same research worker produces equivalent local and authorized remote
  contracts.
- A remote Specialist receives its resolved manuscript and route settings.
- An unauthorized tool never appears in the destination worker catalog.
- Cancellation and terminal results remain correlated and idempotent across
  disconnects and retries.
- Removing the default delegation binding does not strand explicitly targeted
  work.

Suggested commit boundary:

- feat(delegation): transport the canonical worker specification
- feat(workshop): unify local and remote worker lifecycle

Landed boundary:

- Local and remote workers now share one versioned semantic spawn snapshot,
  including Specialist, Bot, parent-mode, route, placement, and exact requested
  tool provenance.
- Destination admission intersects exact tools with its directional peer grant
  and the normal runtime and Specialist ceilings before registry construction.
- Signed cancel and steer mutations target the persisted destination directly,
  verify their original execution authority, and deduplicate steering across
  retries and restarts.
- Clearing or changing the ingress default no longer cancels work that already
  captured an explicit destination.

### Phase 5 — User and agent target selection

**Outcome:** Users may select an exact workshop, and agents may discover and
choose only destinations the user has authorized for agent routing.

Implementation:

- Build an execution-target inventory from pairing identity, reachability,
  signed worker capabilities, Phase 3 policy, and Stasis requirements.
- Expose that inventory to Home and through the existing runtime/capability
  discovery surface.
- Add exact, SameAsParent, and Auto selection to relevant worker UI.
- Allow an agent to reference only opaque ids returned by the authorized
  inventory.
- Require allow_agent_targeting for automatic or model-selected routing.
- Make Auto selection deterministic for the same candidate set and selection
  key.
- Persist candidate requirements and the final selection reason.
- Permit automatic retry elsewhere only for explicitly retry-safe work.

Acceptance:

- Home and the agent see no denied capability as available.
- Explicit user selection works even when agent targeting is disabled.
- An invented, stale, offline, or unauthorized runtime id is rejected.
- Auto selects only a capable and authorized workshop.
- A mutating job never relocates after partial execution without a durable
  fenced checkpoint.
- Activity UI says where each worker ran.

Suggested commit boundary:

- feat(runtime): expose authorized execution targets
- feat(agent): select eligible worker placement
- feat(home): add worker target controls and provenance

### Phase 6 — Remote Coder on an existing project

**Outcome:** Coder can run completely on a selected workshop when that workshop
already owns the repository.

Implementation:

- Put workshop selection before project selection in Coder.
- Query projects and repository state from the selected workshop, never from
  the client filesystem.
- Define project references as runtime id plus stable Forge/repository
  identity.
- Bind each Coder undertaking to one execution authority and optional existing
  work environment.
- Route project preparation, Forge leases, files, code intelligence, shell,
  PTY, builds, evidence, review, and disposition through that binding.
- Admit Coder only when peer policy grants the project and required execution
  scopes.
- Support current checkout and isolated copy only as implemented by the
  destination workshop.
- Surface offline, permission-revoked, missing-project, lease-conflict, and
  stale-workspace states without falling back locally.

This phase does not transfer a local dirty tree to another workshop.

Acceptance:

- A user connected from mobile can choose Mac mini, choose a project present on
  Mac mini, and complete a Coder undertaking there.
- Every Coder tool and PTY reports the same execution authority.
- Current checkout changes remain on the destination checkout.
- Isolated-copy disposition uses destination Forge semantics.
- Switching the UI's default workshop cannot retarget an active undertaking.
- Revoked Coder permission blocks new mutations and future publication.

Suggested commit boundary:

- feat(coder): bind project discovery and work to a selected workshop
- feat(home): add remote coder workshop and project flow

### Phase 7 — Portable Coder and durable handoff

**Outcome:** A governed coding undertaking can be reconstructed on another
authorized workshop from exact durable inputs and return reviewable results.

Implementation:

- Reuse Forge checkpoints, export bundles, durable blobs, and the existing OCI
  WorkEnvironmentSpec rather than copying live directories.
- Capture tracked and admitted untracked state behind existing secret and
  unsafe-file checks.
- Bind exact base commit, checkpoint digest, image digest, resources, network,
  secret references, fences, and publication expectations.
- Materialize the project on the selected destination and run the normal Coder
  runtime through environment-aware tool ports.
- Persist evidence and checkpoints before acknowledging completion.
- Return signed result provenance and reconcile with compare-and-swap rather
  than force-updating.
- Support explicit handoff or retry from the last durable checkpoint.
- Complete the OCI epic's remaining loss-and-reconstruction proof as part of
  the phase exit.

Acceptance:

- A dirty but safely capturable project can be delegated without a shared
  filesystem or database.
- Unsafe or secret-bearing state is blocked before transport.
- Losing the destination after an acknowledged checkpoint does not lose the
  checkpoint or completed result.
- A separate authorized daemon can reconstruct the same immutable inputs.
- Stale attempts cannot publish over newer work.
- Conflicts preserve both sides and return typed reconciliation work.
- No raw credential, daemon path, live container id, or mutable host reference
  becomes portable state.

Suggested commit boundary:

- feat(forge): produce portable governed coder checkpoints
- feat(coder): execute portable work on an authorized workshop
- feat(federation): reconcile signed remote coder results

## Cross-phase migration rules

- All new serialized records are versioned and additive.
- Existing chats remain ordinary sessions with no Bot binding.
- Existing Manuscripts remain valid and do not gain user-specific Bot state.
- Existing worker requests with no placement use SameAsParent.
- Existing remote task.request peers migrate to Assistant work only.
- Existing delegation binding becomes a default target preference until the
  new target UI is established; it does not remain a hidden authority source.
- Existing safe remote delegated tasks continue under the legacy decoder until
  contract parity is fully deployed.
- No migration grants shell, Coder, secrets, project access, or agent targeting.

## Observability requirements

Every admitted local or remote worker records:

- Bot id and Bot profile revision when present;
- parent session and work correlation;
- requested target and resolved runtime;
- selection reason and advertised capability snapshot;
- requesting peer and destination policy revision;
- requested and effective tool domains;
- project and work-environment ids when present;
- Stasis job/attempt and Forge lease/fence when present;
- terminal status, result provenance, and denial/cancellation reason.

User-facing activity uses workshop labels. Logs and durable records use stable
runtime identities. Secret values, private connection details, and raw
credentials never enter either surface.

## Verification strategy

Every phase adds tests at the owning boundary:

- domain serialization and backward compatibility;
- daemon store restart and migration;
- turn admission and policy intersection;
- signed envelope tamper, replay, expiry, and identity tests;
- asymmetric two-daemon permission matrices;
- three-daemon exact and Auto placement;
- disconnect, cancellation, retry, and revocation;
- Home desktop and mobile interaction tests;
- remote Coder project and authority consistency;
- portable checkpoint, loss, reconstruction, and publication conflict.

Phase completion runs the repository CI parity suite from AGENTS.md. Phases
that change public behavior also update the canonical engine, SDK, and
docs/guides documentation in the same merge unit.

## Non-goals

- Running a separate agent runtime for each Bot.
- Treating a Bot, Manuscript, memory entry, or model output as authority.
- Sharing daemon databases, host filesystems, or live process memory.
- Letting Home manufacture runtime identities, grants, leases, or environment
  handles.
- Treating pairing, reachability, or advertised capability as permission.
- Parsing arbitrary shell commands to claim they are read-only.
- Silently moving an active Coder undertaking between workshops.
- Falling back mutating work to another daemon without a durable checkpoint.
- Copying plaintext secrets or ambient host credentials with portable work.
- Making OCI mandatory for conversational or stateless delegated workers.

## Code anchors

| Concern | Starting seam |
|---|---|
| Specialist composition | src/identity_manuscript.rs |
| Interactive turn contract | crates/medousa-types/src/daemon_api.rs |
| Turn preparation | src/agent_runtime/turn_orchestrator.rs and src/agent_runtime/daemon_interactive_turn.rs |
| Session projection | src/session_catalog.rs and src/session_meta_store.rs |
| Locus scope | src/locus_memory.rs |
| Home Specialist selection | apps/medousa-home/src/lib/stores/activeAgent.svelte.ts |
| Workshop spawn | src/workshop_contract.rs and src/workshop_api.rs |
| Durable workers | src/agent_runtime/turn_worker/store.rs and src/agent_runtime/turn_worker/run.rs |
| Remote delegation | src/delegation.rs, src/delegation_tools.rs, and src/delegated_task.rs |
| Remote tool ceiling | src/agent_runtime/turn_worker/policy.rs |
| Mesh identity and grants | src/mesh/envelope.rs, src/mesh/grants.rs, and src/mesh/registry.rs |
| Remote receiver | src/mesh/task.rs |
| Generic job placement | src/runtime_job_spec.rs |
| Work-environment contract | crates/medousa-runtime/src/work_environment.rs |
| Federated placement | src/mesh/work_environment_federation.rs |
| Portable parallel work | src/work_environment_parallel.rs |
| Coder authority | src/agent_runtime/modes.rs and src/agent_runtime/coder_tools.rs |
| Forge governance | crates/medousa-forge/ |
| Workshop settings | apps/medousa-home/src/lib/components/settings/SettingsWorkshopsSection.svelte |

## Epic exit criteria

This epic is complete when:

1. A named Bot provides durable Specialist and memory continuity without
   becoming a new runtime or mode.
2. Every worker has explicit, durable requested and resolved execution
   placement.
3. Every destination workshop exposes and enforces directional per-peer
   execution permissions configurable from that workshop's Settings.
4. Remote workers use the canonical worker contract while destination policy
   determines their effective authority.
5. Users and opted-in agents can choose among only authorized capable
   workshops.
6. Coder can operate coherently on a project already owned by a remote
   workshop.
7. Portable governed Coder work survives handoff, loss, reconstruction, and
   conflict without shared mutable state or authority leakage.

The resulting model stays simple at the product surface:

> Choose who helps, how they should help, and where authorized work should run.
