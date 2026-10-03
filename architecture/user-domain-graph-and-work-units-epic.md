# User domain graph and composable work units

> **Status:** Implementation started — storage and intent foundation; execution integration pending
>
> **Date:** 2026-10-02
>
> **Related:** [Assistant ownership foundation](assistant-ownership-foundation-plan.md),
> [identity memory](cognitive-identity-memory-plan.md),
> [conversation context](conversation-context-addressing-and-derivation-plan.md),
> [Bots and remote execution](bots-and-authorized-remote-execution-epic.md),
> [runtime worlds](runtime-owned-worlds-epic.md), and
> [interaction and state](interaction-and-state-model.md)

## Product intent

The user is the domain. Notes, vault folders, projects, artifacts, feeds,
conversations, agents, and execution records describe relationships within that
domain. Any understood responsibility with a proper scope can be a goal, and
several responsibilities can compose into a larger unit of work.

Medousa must recognize and retain these responsibilities through ordinary
interaction. The runtime manages agent execution, continuity, dependencies, and
recovery. The user expresses intent, changes direction, and makes decisions
when needed through the surfaces they already use.

This epic establishes a shared graph of resource relationships and durable work
scopes over existing systems. It preserves the current UX and UI except where a
specific interaction needs a useful indication, result, or control. A Goals tab,
graph editor, agent management console, or manual orchestration workflow is not
required to use the capability.

For example, a vault folder can support an entire code project; that project can
produce several note folders; one design note can inform multiple projects; and
an artifact can track their combined progress. A conversation can establish work
over any of those resources. Closing that conversation does not end the work.

## Product decisions

1. **The user domain anchors meaning.** Domain membership comes from durable
   authenticated identity and explicit sharing, never the currently selected
   profile, a display name, or an inferred match across workshops.
2. **Resources remain themselves.** Notes stay notes, Coder projects keep their
   existing structure, and artifacts keep their existing surfaces and feeds.
   Linking resources does not move, upload, or duplicate their content.
3. **Relationships are explicit and navigable.** The runtime exposes typed links,
   inverse queries, provenance, and current resolution state. Agents do not have
   to reconstruct these facts from conversation prose on every turn.
4. **Work units compose.** A unit can include other units and existing resources.
   Composition records its own intent and completion or maintenance conditions.
5. **A session is an attachment.** It can originate, discuss, or execute work;
   the work identity and responsibility survive individual turns and sessions.
6. **The runtime retains responsibility.** An authorized agent, Bot, external
   provider agent, or service can initiate, coordinate, execute, review, or report
   work. Medousa Assistant is one participant, not the mandatory coordinator.
7. **Scope and authority remain distinct.** Relationships explain relevance;
   authenticated grants and destination policy determine permitted actions.
8. **Existing execution engines remain authoritative.** Forge, Stasis, turn
   workers, ACP, provider adapters, and delivery services retain their native
   execution records. The graph connects them and the work scope governs their
   coordination.
9. **Contact is independent of execution.** The initiator, coordinator, reviewer,
   reporting agent, and delivery channel may differ. Suppressing outreach does
   not cancel coordination or erase its results.
10. **Existing interfaces are the default.** New chrome must solve a demonstrated
    problem in understanding, steering, or resolving work. Users should not
    have to organize the AI before it can act on their intent.

These are proposed target decisions. Existing mode ceilings and admission
contracts continue to apply until the corresponding implementation ships.

## Reference scenarios

### Work through several agents and workshops

The user talks to Prox through Grok Bot on a remote daemon connected by a portal
to a local workshop and says:

> Send this change to codex-mini, then have Muse review the work. Let Prox tell
> me when it is ready.

The runtime resolves the exact project and requested executor configuration,
records the accepted intent and dependencies, and admits execution on the
workshop that owns the project. Prox may coordinate while Codex implements and
Muse reviews the exact resulting revision. Existing authorization can permit
the entire requested sequence; an absent grant produces a concrete blocker.
No participant needs to keep the original model turn running.

Changing the contact preference to Medousa voice, Instinct, Dots, or silence
updates the same responsibility. It does not reconstruct the plan or move the
project. Missing provider, model, review, or contact capabilities remain explicit
and are never silently substituted.

### Relationships across vault resources and projects

The user links a research folder to a project. An implementation later produces
documentation and test-notes folders. A shared design note informs that project
and a second project. All resources remain independently addressable, and the
runtime can answer both forward and inverse relationship queries.

If the user also asks to keep the documentation current, that establishes a
maintenance responsibility with relevant change triggers. Merely linking the
research folder supplies context; it does not request every possible action on
its contents.

### A larger responsibility over existing work

The user asks to keep a launch ready. The accepted scope includes a code change,
a maintained release note, and an artifact that summarizes readiness from feeds.
Each component has its own lifecycle. The aggregate evaluates a saved readiness
condition against their current evidence rather than treating every linked
object as a task that must terminate.

The same note or result may support several responsibilities. Reading shared
evidence does not launch duplicate work, and cancelling one responsibility does
not cancel executions owned by another.

## Existing foundations and current gaps

The following anchors were inspected for this proposal. They establish useful
primitives; they do not establish an end-to-end user domain graph or generic
work-unit runtime.

| Foundation | Current code | Gap addressed by this epic |
|---|---|---|
| User identity and relational memory | [identity_memory.rs](../src/identity_memory.rs), [identity_store_ext.rs](../src/identity_store_ext.rs), [user_profiles.rs](../src/user_profiles.rs) | Connect resource and work references to the user domain without duplicating learned identity or conflating semantic links with policy relationships |
| Vault metadata and backlinks | [note.rs](../src/vault/note.rs), [links.rs](../src/vault/links.rs), [contracts.rs](../src/vault/contracts.rs), [mutation.rs](../src/vault/mutation.rs) | Note links use paths; generalized resource identity, folder identity, cross-kind links, and move reconciliation are needed |
| Code work and project UX | [Forge model](../crates/medousa-forge/src/model.rs), [project explorer](../apps/medousa-home/src/lib/components/lme/explorers/LmeCodeExplorer.svelte), [undertaking bindings](../apps/medousa-home/src/lib/stores/undertakings.svelte.ts), [agent handoff](../apps/medousa-home/src/lib/utils/undertakingWorkspace.ts) | Repository groups, Forge work threads, attempts, and chat attachments need precise graph mappings; the repository grouping and the work item are distinct |
| Artifact lineage and feed relationships | [artifact_store.rs](../src/artifact_store.rs), [environment types](../crates/medousa-types/src/environment.rs), [feed_store.rs](../src/feed_store.rs), [recurring_feed.rs](../src/recurring_feed.rs) | Artifact/session lineage and component/feed bindings are separate contracts; they need shared references and explicit maintenance semantics |
| Assignment projections | [assignment types](../crates/medousa-types/src/assistant_assignment.rs), [assistant_assignments.rs](../src/assistant_assignments.rs), [ledger store](../crates/medousa-acp-client/src/coordination/store/assistant_ledger.rs) | One assignment binds a native execution; a composable responsibility above assignments and independent of a source session is missing |
| Durable events and continuation | [coordination types](../crates/medousa-types/src/coordination.rs), [owner inbox](../crates/medousa-acp-client/src/coordination/store/owner_inbox.rs), [continuation host](../src/daemon/coordination/host.rs), [owner intake](../src/daemon/coordination/owner_intake.rs) | Owner events require an owner session; the host admits peer-terminal continuation and blocks unsupported wake sources; generic work-scoped intake is needed |
| Context discovery and exact provenance | [pointer index](../src/context_pointer_index.rs), [session references](../crates/medousa-types/src/session.rs), [context derivation](../src/context_derivation.rs) | Ranked breadcrumbs and transcript manifests need a bounded graph neighborhood and work-scoped context manifest |
| Provider conversations | [external conversations](../src/daemon/external_conversations.rs), [external access](../src/daemon/external_conversations/access.rs), [provider types](../crates/medousa-types/src/external_conversation.rs) | Grok callbacks and Muse/Instinct/Dots messages need normalized assignment correlation, participant admission, and durable event subscriptions |
| Federation and contact | [peer coordination](../src/peer_coordination_mesh.rs), [completion delivery](../src/peer_completion_delivery.rs), [peer policy](../src/peer_execution_policy.rs), [channel delivery](../src/channel_delivery.rs), [Home notifications](../src/home_notifications.rs), [Live](../src/live_handlers.rs) | Work-scoped graph federation and contact decisions across reporters/transports are needed; existing return paths are not generic voice outreach |

The [Assistant ownership foundation](assistant-ownership-foundation-plan.md)
already plans durable continuation, placement, bounded dependencies, and contact
policy. This epic generalizes its target assumption that one Assistant session
owns coordination. Its ledger, receipts, grants, and recovery work should be
extended, with outstanding correctness gates retained. This proposal does not
retroactively mark those gates complete or change shipped behavior.

## Domain model

The names below describe proposed contracts, not frozen API schemas.

| Contract | Responsibility |
|---|---|
| UserDomainRef | Stable domain identity and authenticated membership or mapping across authorities |
| ResourceRef | Resource kind, authority, and stable native or daemon-issued identity; locator and revision are separate |
| RelationshipRecord | Source and target refs, relation kind, domain, owning authority, revision, provenance, and validity or tombstone state |
| WorkUnit | Accepted intent, authoritative coordinator daemon, scope revision, lifecycle policy, aggregate budget, and durable decision history |
| WorkScope | Explicit resource refs, child-unit refs, bounded selectors, dependency conditions, and context manifest |
| ParticipantBinding | Authenticated actor, adapter, role, permitted actions, exact executor/model constraints, and optional conversation attachment |
| WorkEventSubscription | Work or resource selector, event kinds, revision/generation, replay cursor, recipient binding, and admitted reaction policy |
| WorkDecision | Event evidence, deciding actor, scope revision, resulting command refs, and completion or blocker evidence |
| ContactDecision | Work/result refs, reporting participant, recipient, transport, contact preference revision, attempts, and receipts |

The user domain is the semantic root, not a claim that one daemon owns every
resource or that one global profile id is sufficient. Shared resources may be
visible in several user domains. Their native owner and governing authority
remain exact and must be rechecked on resolution and execution.

### Stable identity and resolution

Use authority-qualified references consistently with `SessionRef` and existing
workshop identity. Equal native ids on two workshops must never identify the
same resource. Keep display labels, filesystem paths, transport endpoints, and
current revisions separate from identity.

Forge work, repository groups, artifacts, feeds, and conversations have different
native identity rules. Register mappings through adapters rather than deriving
them from matching titles or incidental UI groups. Feed-backed components and
stored artifact payloads are distinct resource kinds even when one renders the
other.

Notes and folders need durable identity mappings tied to the vault root and
source class. A managed move can preserve identity; an externally observed move
must be reconciled from evidence and may remain ambiguous. Equal content hashes
do not prove identity. Deletion records a tombstone; a later file at the same
path does not silently inherit the deleted object's relationships. The identity
mapping must not require rewriting every note or replacing normal markdown.

### Relationship meaning and composition

Initial relation kinds should cover `contains`, `part_of`, `informs`, `documents`,
`produced_by`, `reads_from`, `depends_on`, and participant bindings such as
`executes`, `reviews`, and `reports`. Define direction and inverse query semantics
for each. Preserve exact source evidence for native facts and actor attribution
for declared links.

Semantic relationships can be many-to-many and cyclic. The admitted work-unit
composition and execution-dependency projection must reject cycles that would
make lifecycle evaluation or scheduling recursive without a bound. Iterative
fix/review loops are explicit bounded rounds, not unrestricted dependency cycles.

A unit is a saved scope and responsibility over part of the graph. Membership
does not include all transitively reachable resources. Folder selectors must
state whether they select current members or track future members, which event
kinds matter, and what limits apply. Changes to graph membership do not silently
expand the authority or context of an already admitted execution.

The proposed graph can represent both composition and shared context:

```mermaid
flowchart TD
    D[User domain] --> W[Launch readiness unit]
    W -->|includes| C[Code work unit]
    W -->|includes| M[Documentation maintenance unit]
    W -->|tracks| A[Readiness artifact]
    C -->|acts on| P[Code project]
    M -->|maintains| F[Notes folder]
    P -->|produces| F
    N[Design note] -->|informs| P
    N -->|informs| P2[Another project]
    A -->|reads from| F
    D --> P2
```

Work admission supplies the lifecycle conditions, triggers, and effective
authority associated with these relationships.

Parents retain their own success conditions. A completed child does not complete
its parent, and an ongoing child can satisfy a readiness condition without being
terminated. Shared child units and assignments expose whether a parent may
observe, steer, or cancel them; composition alone grants none of these rights.

## Graph storage and agent access

Expose one logical relationship contract with forward and inverse indexes.
Each relationship has one authoritative writer and revision history. Resource
metadata and backlinks are projections of that contract, not independently
editable copies that must happen to agree.

The graph references existing source stores. It does not become another store
of note bodies, repository state, provider transcripts, execution truth, or
learned identity. Bridge the existing identity graph through typed adapters;
validate support for resource kinds before deciding which records belong in
Stasis identity storage. Do not encode the entire work graph in user preferences
or treat arbitrary cognitive-memory edges as executable authority.

Commit native source facts before publishing graph observations. If a source
commit succeeds and indexing fails, retain a repairable observation or replay
marker and expose incomplete coverage. Relationship writes need revision
preconditions and durable mutation receipts. Rebuilding an index cannot invent
work intent, execution grants, or historical ownership.

Agent queries must support exact resolution, forward/inverse relationships,
responsibility membership, participants, current status, pending dependencies,
and the evidence behind a conclusion. Responses include bounded pages, stable
cursors, source revisions, provenance, and coverage or unavailable reasons.
Denied resource identities and labels must not leak through relationship counts
or inverse queries.

At turn admission, compile a small relevant graph neighborhood with exact refs
and current responsibilities. Agents can pull additional authorized context as
needed. Neither the full user graph nor raw journals belong in every prompt.
An inferred association can guide discovery; it is not a committed relationship
or an instruction until admitted through the appropriate runtime command.

## Recognizing and admitting work

Ordinary user intent can create, join, revise, or conclude work. The reasoning
agent identifies the requested outcome and relevant references; the runtime
validates and records an attributable acceptance before dispatching effects.
The user does not need to fill out a goal form or maintain an agent plan.

Admission binds the user domain, work authority, intent, scope revision,
participants, effective grants, completion or maintenance policy, contact
preference, and limits. An exact command key retries the same intent; a changed
intent is a revision or a new command, not a retry. Concurrent requests that
claim the same resource need explicit reconciliation rather than heuristic
merging based on topic similarity.

Existing project bindings, explicit resource references, and durable request
associations can join known work. Similar titles and a model's recollection are
not sufficient. If ambiguity affects which resource will be changed or what
responsibility is accepted, resolve that concrete ambiguity through the current
interaction. Avoid asking the user to manage agents or confirm routine work
already covered by their request and existing authorization.

Reading a note, rendering a feed, or linking resources does not by itself accept
a background maintenance obligation. When the user requests ongoing work, save
its trigger, freshness condition, stop condition, and limits. A completed refresh
is an occurrence of the ongoing unit; it does not finish the responsibility.

Proposed lifecycle states distinguish accepted, active, waiting, needs attention,
paused, satisfied, failed, and cancelled. Native unknown, unavailable, and
interrupted evidence remains visible in the projection. Final state names and
transitions must be defined with existing assignment lifecycle contracts in
Phase 0. Terminal execution, fulfilled intent, and successful delivery are
separate facts.

## Coordination and durable event hooks

Persist event subscriptions and dependency decisions before execution can
produce a result. Combine command admission and subscription registration in
one durable boundary, or use a recoverable outbox plus replay from an exact
source cursor. A completion racing registration must still reach its intended
recipient without relying on an in-memory callback.

Existing jobs, workflows, recurrence, turn workers, ACP sessions, and provider
conversations expose normalized events through adapters. Preserve each native
attempt, receipt, and uncertainty state. Provider transport acceptance is not
task completion. Uncorrelated messages remain messages until an admitted adapter
can establish the exact work/assignment association.

Normalize events into durable work-scoped intake. Serialize state-changing
coordination decisions by work authority and generation, while allowing distinct
executor sessions to run concurrently. Keep session serialization when a
decision also writes to a conversation. Coordinator handoff increments a fencing
generation; stale coordinators cannot issue further commands.

A reaction may advance a deterministic dependency, wake an authorized reasoning
participant, request input, or record evidence. Event data and model-written
instructions cannot mint authority. Agents can register admitted reactions and
delegate within their effective scope; each destination still validates its
own grants, adapter capabilities, and Forge leases.

Persist decision and command identities before effects, and consume the event
only after durable result correlation. Repeated or reordered events must not
regress terminal facts or relaunch an uncertain command. Recovery reconciles
started decisions from committed evidence. Missing process-local tickets or
provider custody cannot justify a replacement execution.

Apply aggregate limits for cost, elapsed time, wake rate, concurrency, retries,
causal depth, review rounds, and graph/event fan-out. Child units receive bounded
allocations under their governing responsibility. Shared execution has one
accounting identity and explicit budget attribution; observing it from another
unit does not dispatch or charge it again. Exhaustion leaves an inspectable
blocker rather than an endless chain of agents waking each other.

## Federation and authority

Every work unit has one authoritative coordinator daemon and an explicit user
domain association. Resources and assignments may belong to other workshops.
Graph reads use authenticated mappings and permission-filtered projections;
commands use existing signed mesh/portal transport and destination admission.

A portal attachment does not silently relocate the unit, copy its resources, or
merge workshop identities. Explicit custody transfer, if supported, requires a
durable fenced handoff and acknowledgment. Otherwise the original authority
retains coordination custody through reconnect and restart.

Expose incomplete graph coverage and unavailable authorities. Distinguish a
deleted resource, denied resolution, and an offline source without leaking
private metadata. Do not report an offline workshop as an empty graph or infer
work failure from a lost connection.

Filesystem authority follows the workshop daemon. Home remains a window into
that daemon's disk, with local folder pickers, Reveal, and local file conversion
only when co-located. Linking a vault resource to a remote project establishes
a reference and separately admitted access, not an upload pipeline.

## Contact and existing surfaces

Persist contact preference independently from coordinator and execution roles.
Support meaningful changes, completion, required decisions, or silence, with
an explicit reporting participant and authorized route. Resolve later changes
against the current preference revision before sending. Contact attempts and
receipts remain associated with the work outcome.

Reuse existing channel delivery, outbox, notifications, and provider send paths.
Normalize external reporting as an attributable participant action rather than
impersonating the user or pretending Medousa wrote another agent's reply.
Uncertain sends require native reconciliation; they do not authorize a duplicate
message. Mute, snooze, or unavailable delivery does not stop accepted work.

Voice contact is an adapter milestone with explicit capability and receipt
semantics. A Live invitation, an accepted in-app voice call, and PSTN/SIP calling
are different capabilities. Start with existing Live entry points where suitable;
implement actual outbound calling separately if required for the supported
product promise. Never represent an invitation or notification as a completed
voice call. Voice transport ends independently of ongoing work.

Surface results through the current conversation, project phase/review,
note updates, artifact/feed state, and activity or notification channels. Add a
small relationship or work-context affordance only where the existing surface
cannot explain relevant state. Preserve the current navigation, project/thread
vocabulary, composer, and agent selection unless a specific phase demonstrates
why a change is necessary.

## Delivery phases

### Phase 0 Contract and source audit

Map stable identities, native revisions, ownership, visibility, events, and
recovery limits for each resource and executor. Define relationship semantics,
work lifecycle transitions, aggregate completion, user-domain mappings, and
single-writer authority. Select the storage boundaries after auditing Stasis
identity capabilities and existing persistence interfaces.

**Exit gate:** executable contract fixtures cover many-to-many links, composition,
finite and ongoing work, shared evidence, cross-authority identity, and native
uncertainty. Every proposed record has an owner and replay strategy; no phase
assumes that an existing process-local guarantee is durable.

### Phase 1 Resource identity and relationship store

Implement typed resource refs, native adapters, canonical relationship writes,
forward/inverse indexes, bounded resolution, provenance, revisions, tombstones,
and repair. Start with vault notes/folders, repository and Forge work refs,
sessions, artifacts/components, and feeds. Bridge identity and existing links
without migrating content or overwriting their native stores.

**Exit gate:** one note links to several projects, a project links to several
folders, and inverse reads agree after restart. Managed rename preserves identity;
ambiguous external moves, path reuse, index failure, and revoked visibility are
handled explicitly. Queries cannot leak another user domain.

### Phase 2 Accepted work scopes and composition

Persist work units above existing assignments, with explicit intent, resource
membership/selectors, child units, lifecycle conditions, aggregate budgets, and
conversation attachments. Add idempotent create/join/revise/pause/cancel/inspect
commands through current typed tool and authenticated service boundaries.

**Exit gate:** a conversation creates one accepted responsibility; a later session
joins it by exact reference. A combined code/note/artifact unit evaluates saved
readiness correctly. Shared children, dependency cycles, scope revisions, and
cancellation cannot create duplicate work or unintended cascades.

### Phase 3 Work intake and execution adapters

Generalize the assignment ledger and owner inbox into work-scoped observation
and decision paths. Normalize native executors and provider events. Add durable
subscription registration, command fences, replay, coordinator generations,
bounded wakes, and restart reconciliation using existing Stasis scheduling.

**Exit gate:** implement/review/fix runs in distinct sessions against exact
revisions and stops at its saved bounds. Completion racing subscription, duplicate
callbacks, busy conversations, crashes around every commit/effect boundary, and
lost native custody cannot silently drop work or relaunch uncertain effects.

### Phase 4 Any authorized participant and federated coordination

Expose admitted graph/work commands and subscriptions to Medousa agents, Bots,
ACP peers, and provider agents through their supported authenticated adapters.
Extend narrow external-agent scopes deliberately; do not grant ambient admin
access. Implement work-scoped federation, source associations, exact execution
constraints, and coordinator handoff where required.

**Exit gate:** Prox through Grok Bot coordinates Codex on a portal-connected
workshop and an exact Muse review, while the originating conversation is closed.
Provider adapters must prove real send/receive and work correlation. A deterministic
adapter test is necessary but does not replace live acceptance evidence. Offline
targets, revoked grants, lost responses, and stale coordinator generations remain
recoverable without silent model/provider substitution.

### Phase 5 Maintenance and event propagation

Bind explicit note/folder maintenance and artifact/feed responsibilities to
selected native changes, schedules, freshness conditions, and occurrences.
Bound event coalescing and fan-out; fence feedback from generated updates so
the runtime does not create infinite self-triggering maintenance loops.

**Exit gate:** a maintained note updates on relevant admitted changes, unrelated
linked resources do not wake work, an artifact consumes attributable results,
and ongoing status remains truthful across missed events, cursor retention gaps,
restart, pause, and changed membership. A retention gap reconciles a snapshot;
it never claims complete unseen event history.

### Phase 6 Contact policy and useful presentation

Persist contact decisions above existing transports, including reporter choice,
silence, preference revisions, retries, and receipts. Integrate work context only
into existing surfaces that need it. Deliver and validate voice contact as an
explicit adapter capability rather than assuming Live already provides outreach.

**Exit gate:** the same work can return through Prox, Muse, Instinct, Dots, or
Medousa using a verified route, and can run silently. Changing contact preference
does not restart execution. Duplicate events, uncertain sends, expiry, and offline
recipients do not produce misleading delivery claims or duplicate logical contact.
The ordinary request requires no new goal-management workflow.

### Phase 7 Compatibility and operational closeout

Audit legacy mappings, rebuilds, retention, repair diagnostics, capability
advertisement, SDK contracts, documentation, and performance. Preserve current
single-session and project behavior while rolling out supported graph/work
capabilities incrementally.

**Exit gate:** legacy sessions, vaults, projects, artifacts, and recurring work
remain usable; migration is idempotent; supported capabilities have scenario and
live acceptance evidence; unsupported adapters remain explicit. Run repository
CI parity before the implementation PR is ready.

## Acceptance matrix

| Scenario | Required evidence |
|---|---|
| Folder supports a project | Exact bidirectional relationship; content stays on its native authority |
| Project produces several folders | Each folder has independent identity and an attributable production relation |
| Note informs multiple projects | One note identity, several links, no duplicated content or execution |
| Unit combines other units | Saved aggregate condition; child progress cannot falsely complete the parent |
| Prompt becomes work | Durable acceptance before effects; no mandatory goal form or new navigation |
| Another session continues work | Same work ref and scoped context, independent of the initiating session |
| Any admitted agent delegates | Exact participant and effective scope; existing destination checks still apply |
| Codex completes and Muse reviews | Result/revision-bound dependency and a correlated provider review receipt |
| Restart during handoff or execution | Reconciliation from durable evidence; no replacement for uncertain effects |
| Coordinator changes | New fencing generation; old coordinator cannot dispatch |
| Resource is renamed or recreated | Evidence-based identity continuity or explicit ambiguity/tombstone |
| Shared work is cancelled by one parent | Ownership rules preserve unrelated consumers and independently owned work |
| Note or artifact is maintained | Admitted triggers, freshness and occurrence evidence, bounded feedback |
| User requests silence | Coordination proceeds and results persist with no unsolicited contact |
| Reporter or contact route changes | Same unit, latest admitted preference, attributable delivery attempts |
| Remote authority is offline | Incomplete coverage and waiting/unavailable evidence, never invented emptiness |
| User visibility or grant is revoked | Reads and actions recheck scope; no leaked relationship metadata |

## Compatibility and rollout

Legacy records retain their native ids and behavior. Import only relationships
supported by exact existing bindings or source facts. A session id, matching
title, wikilink, or related feed does not retroactively establish an accepted
background obligation. Older work without sufficient provenance stays inspectable
as legacy work rather than receiving invented ownership or continuation grants.

Keep graph metadata outside ordinary note content by default; optional portable
metadata must use the same authoritative contract. Retention and deletion must
preserve the minimal evidence needed by active responsibilities, or produce a
visible unresolved reference. Removing a view or conversation is not implicit
cancellation. Existing visibility/deletion policy still applies to referenced
content and may block further use until resolved.

Ship graph inspection and linking before broad autonomous execution. Then add
finite work composition and the provider review reference flow, followed by
maintenance and contact adapters. Capabilities advertise only composed, verified
support. Do not claim Phase 4 is complete because internal ACP coordination works
without a verified external-provider path.

As each implementation lands, update [engine coordination](../docs/engine/coordination.md),
[external conversations](../docs/engine/external-conversations.md), relevant
`docs/engine/` and `docs/sdk/` contracts, generated API parity, and user guides
indexed in `docs/README.md`. This architecture proposal is not documentation of
currently shipped behavior.

## Implementation constraints and exclusions

Use existing persistence admission and `ForgeExecutionService` for blocking
Forge, filesystem, Git, and process work in async paths. Windows background
spawns use `detach_new_session` and `CREATE_NO_WINDOW`. Optional adapters are
installed through Settings → Packages under the existing binary-resolution
rules. Never claim a missing package or unavailable adapter is ready.

This epic does not require a new scheduler, wholesale graph database migration,
replacement identity or memory system, vault upload flow, project UX redesign,
autonomous scope expansion, agent marketplace, or automatic compute provisioning.
Publishing, merging, deployment, and unrelated mutations require their own
accepted intent and authority. Supporting voice contact does not imply every
telephony transport ships in the first increment.

## Decisions to resolve before implementation

- Select the graph persistence port and backend after the Phase 0 source audit;
  retain one canonical relationship writer and a repairable projection model.
- Specify note/folder identity placement, portable metadata behavior, and the
  evidence required to reconcile moves outside Medousa.
- Define authenticated user-domain mappings for portal and peer workshops,
  including shared resources and revoked membership.
- Define typed work commands, scope revision semantics, and aggregate budget
  accounting using the existing tool/admission contracts.
- Establish the provider completion/correlation capabilities needed for Muse,
  Instinct, Dots, and Grok Bot; unverified transport is an acceptance blocker for
  that adapter, not proof that the whole runtime is unavailable.
- Choose the first supported voice-contact adapter and distinguish invitation,
  acceptance, call connection, and call completion receipts.

## Progress

### First implementation slice — 2026-10-02

Stasis 0.13's identity model provides people, contacts, channel/policy profiles,
and identity relationships; its typed entity writes do not provide the native
resource/work records required here. Keep those identity entities authoritative
and bridge their authenticated user IDs with an authority-qualified
`UserDomainRef`. Do not encode projects or work scopes in user preferences.

Added `medousa-types::work_unit` contracts and a dependency-light `medousa-work`
store over the existing capability-confined `medousa-store` primitives. Each
domain has one bounded snapshot and a cross-process writer lock. Revision checks,
idempotent commands, immutable intent events, and atomic publication retain scope
and receipts together. LocalApp uses the identity frozen at turn admission;
bound principals use their authenticated owner. No active-profile fallback occurs
at graph query/write time, and no remote identity mapping is inferred.

The full daemon composes the store and exposes `work.graph`, `work.get`, and
`work.record` through its existing runtime tools and schema catalog. The current
UI is unchanged. Models register unresolved resource claims; available native
facts and revisions remain adapter-owned. Scope, composition, lifecycle, and
contact preferences are retained without execution or delivery side effects.

Executable fixtures cover restart and exact replay, many-to-many links and inverse
reads, locator changes and tombstones, path reuse, shared children, mixed
composition/dependency cycles, domain/authority isolation, cursor conflicts,
competing writers, bounded commands, corruption, symlink substitution, and
publication failures before/after rename. Locator fixtures exercise the registry
contract; they do not claim that the vault managed-move adapter has shipped.

This slice does **not** satisfy the full Phase 0–2 exit gates. Native resource
identity adapters and repair, bounded native resolution, aggregate budgets and
mixed maintenance readiness, subscriptions, coordinator decisions/generations,
provider federation, and contact delivery remain outstanding. Shipped behavior
and current limits are recorded in [the engine guide](../docs/engine/work-units.md).

Validation for this slice: 14 `medousa-work` storage tests, 3 admitted-host tests,
6 runtime action/schema tests, and 56 shared-type tests passed. The daemon library
and binary compile check, strict clippy over all targets of `medousa`,
`medousa-work`, and `medousa-types`, strict docs verification, and diff whitespace
checks passed. The full workspace/hermetic CI matrix and frontend checks have not
been run for this slice; no frontend files changed and no PR has been opened.

### Composition and budget milestone — 2026-10-02

The composition milestone now has exact local conversation attachments, separate
scope revisions, and bounded maintenance checkpoints. Checkpoints pin current
adapter-owned resource versions and observations; parent completion checks named
requirements without terminating shared maintenance work. Scope/native changes,
pauses, and expiry prevent stale checkpoints from completing another parent.
Contact changes leave readiness intact. Legacy command digests remain replayable.

- [x] Durable graph and accepted intent foundation
- [x] Exact conversation joins and mixed finite/maintenance composition contracts
- [x] Durable aggregate budget reservations and native-only custody accounting
- [ ] Native identity/resolution adapters and durable projection repair
- [ ] Native dispatch paths enforcing reservations and settling actual usage

The bounded ledger charges shared executions once per aggregate, checks new
scope additions, retains charges after removal, and preserves native overruns.
Cancellation does not refund unknown custody; native not-started settlement does.
Absolute deadlines survive restart. A model may save ceilings but cannot reserve
or settle native execution custody. This is not yet an executor integration or
aggregate wake/retry/review policy.

The completed milestones are narrower than the phase exit gates below. Native
readiness refresh and runtime execution admission are still required to demonstrate
the full end-to-end Phase 2 fixture.

Validation for this milestone: 19 storage tests, 56 shared-type tests, 4
admitted-host tests, and the runtime action/schema checks passed. Strict clippy
over all targets of `medousa`, `medousa-work`, and `medousa-types`, strict docs
verification, and diff whitespace checks passed. Fixtures include native-custody
publication faults, aggregate cost overflow, immutable settlement replay, stale
maintenance checkpoints, and legacy snapshot/command compatibility. The full CI
matrix has not been run; no frontend files changed and no PR has been opened.

### Native user-vault identity milestone — 2026-10-02

The first native adapter now issues stable user-vault note/folder references and
publishes exact bounded observations through `work.resolve`. It uses an explicit
configured root, the admitted owner domain, and daemon-side capability-confined
IO. Neither the active UI selection nor a model-supplied owner can retarget it.
The existing UI and native note bodies are unchanged.

Managed atomic writes and note moves preserve a daemon-issued ID across owner and
store restart. Delete retains a permanent tombstone; restore and path reuse issue
new IDs. A synced sidecar retains the namespace, locator, physical evidence, and
monotonic observation revision. A root writer lock holds identity custody through
native publication and graph projection. Unchanged observations produce no new
graph event. Exact many-to-many links and inverse queries survive a real native
move, not just a model-written locator fixture.

Native journals retain identity bindings until sidecar projection is durable.
Projection failures return a committed native outcome with repair required;
startup and explicit resolution replay intents/receipts without repeating the
file mutation. Relocation recovery verifies its physical witness and completes
directory fences before publishing a receipt. Replaying a completed old receipt
cannot roll a resource back to an earlier path. Copied registries, corrupt
metadata, missing publication witnesses, and external moves fail without guessing
identity from paths or equal content.
Write witnesses come from the staged file handle; replacement immediately after
publication cannot inherit that identity. Unchanged sidecar and graph replays
finish interrupted parent fences without appending new observations.

- [x] Exact native user-vault note/folder observations
- [x] Managed note write/move identity preservation and deletion tombstones
- [x] Durable projection repair for witnessed native mutations
- [x] Real native revision refresh invalidating maintenance readiness
- [ ] External ambiguity reconciliation and operational repair affordance
- [ ] Project/overlay, artifact, feed, and execution identity adapters
- [ ] Native event subscriptions and automatic observation refresh

Note observation is capped at 1 MiB; an oversized note keeps its identity but is
unavailable as readiness evidence. Folders prove metadata, not recursively ready
membership. Sidecar capacity is bounded and preserves historical IDs. A write
without a durable publication witness retains an ambiguous intent that blocks
identity resolution and further native mutations until explicit reconciliation;
that repair workflow is still outstanding. Platform file identifiers are
reconciliation evidence, not globally portable resource identity.

This closes the native user-vault milestone, not the full Phase 1 gate or the
Prox → executor → review → contact pipeline. See the
[engine guide](../docs/engine/work-units.md) for the shipped action and limits.

Validation: 74 vault/native-adapter tests, 25 shared persistence tests, 19 work
store tests, 4 admitted-host tests, and 6 runtime action/schema tests passed.
Strict clippy over all targets of `medousa`, `medousa-store`, `medousa-work`, and
`medousa-types`, strict docs verification, and diff whitespace checks passed.
The embedded daemon library compile check passed with its existing feature
warnings. Fault fixtures include projection failure after native publication,
interrupted graph publication, completed old receipt replay, and external
replacement immediately after native create/replace. The full workspace/hermetic
CI and frontend matrix have not been run; no frontend files changed and no PR
has been opened. Windows/wasm qualification remains outstanding.

### Native reconciliation milestone — 2026-10-02

`work.reconcile` now exposes bounded inspection, evidence-based external move
adoption, and conservative native journal quarantine through the existing runtime
tools. It retains the admitted owner and explicit configured root; no Goals
management screen, UI change, or user-managed repair ledger was introduced.

An adoption checks an exact native revision, physical object, kind, and original
locator custody. It preserves the reference and many-to-many graph links while
retiring availability for a physically replaced destination resource. Folder
adoption affects only the selected folder, not an inferred descendant scope.
Copies with equal bytes, stale observations, tombstones, symlinks, and foreign
authorities/namespaces cannot authorize a merge or reattachment.

Quarantine retains the original intent and optional native receipt in a synced
archive. A single sidecar publication retains the decision and affected identity
facts before active-intent cleanup. Unproven IDs do not attach to current bytes;
old readiness is invalidated by unavailable facts, and tombstones stay permanent.
Quarantine reports an unresolved native outcome and replays no file effects.
Ambiguous startup leaves native reads available while writes and identity
resolution wait for repair.

Domain-bound command keys and exact digests make retries attributable. A replay
returns its saved decision and projects current facts rather than rolling back a
later locator. Startup/identity admission completes receipt-committed cleanup;
archive-before-snapshot failures preserve the active intent. A failed metadata
transaction cannot use its uncommitted in-memory receipt to clear a journal.
Changed or corrupt metadata remains explicit failure, not an identity reset.

- [x] Native external move adoption from exact physical and revision evidence
- [x] Bounded operational inspection and retained unresolved-publication archives
- [x] Durable repair receipts, restart cleanup, and graph projection replay
- [ ] Project/overlay, artifact, feed, and execution identity adapters
- [ ] Native event subscriptions and automatic observation refresh
- [ ] Repair history compaction and cross-platform qualification

Inspection caps directory entries at 128 and journals at 64 KiB. The sidecar
retains at most 256 repair receipts within its existing 1 MiB bound. Full history
denies new reconciliation without evicting old decisions; exact replay remains
available. The runtime has a repair path for ambiguity, not arbitrary root/store
corruption or a globally portable filesystem identity guarantee. This closes the
native ambiguity milestone, not the full Phase 1 gate or execution coordination.

Validation: 86 vault/native-adapter tests, 4 admitted-host tests, and 6 runtime
action/schema tests passed. Strict clippy over all targets of `medousa`,
`medousa-store`, `medousa-work`, and `medousa-types`, strict docs verification, and
diff whitespace checks passed. The embedded daemon library compile check passed
with its existing feature warnings. Repair fixtures cover publication failure,
restart cleanup, changed evidence, full receipt history, owner isolation, readiness
invalidation, and replay after later native moves. Full workspace/hermetic CI,
frontend CI, and Windows/wasm qualification remain outstanding; no frontend files
changed and no PR has been opened.

### Native project and governed overlay milestone — 2026-10-02

`work.resolve_project` now resolves an exact owned Forge work into separate
repository-group and work-thread references, or observes an exact note/folder in
its current governed overlay. The daemon host supplies Forge authority through
bounded execution admission; requests cannot select a raw repository path or
assert an owner, native revision, actor, or availability. Existing app UI and
Coder project/thread behavior stay in place.

Repository groups receive UUID identities in a synced, capability-confined Forge
sidecar. The native Git common-directory path and physical object are evidence,
not the graph ID. Several work threads share a repository reference; each keeps
its own Forge work reference. Verified physical relocation retains identity,
while a physically replaced repository at the same locator gets a different ID.
Retained unavailable predecessors invalidate old readiness and preserve links.
Copied or corrupt registries cannot silently issue replacement identities.

The adapter pins overlays to both environment branch and generation: Forge
candidate checkouts can share a generation. It opens only the existing overlay
below the governed workspace through a no-follow child capability, with no user
vault or cwd fallback. Overlay notes/folders retain independent native UUIDs and
`project` source references. Stale pins, wrong source/kind/root, and escapes fail
without granting scope or repeating file effects. External overlay move/journal
reconciliation remains a subsequent extension of the repair adapter.

Project observations prove repository identity metadata, and Forge work revisions
prove native lifecycle metadata. Neither is a live checkout content digest or an
execution/review/completion decision. Graph writes keep native custody through
publication, replay unchanged receipts, and repair projection after interruption.
They create no work unit or inferred intent relationship. Existing `work.record`
relationships support real note → several projects and project → several folders
links, including inverse queries after restart.

- [x] Workshop-owned repository IDs distinct from paths and Forge work IDs
- [x] Owner-bound native project and Forge lifecycle observations
- [x] Exact governed overlay note/folder observations with branch/generation pins
- [x] Restart replay, retained replacement availability, and real cross-resource links
- [ ] Artifact/feed and execution identity adapters
- [ ] Overlay reconciliation, subscriptions, and live checkout revision coverage
- [ ] Project registry compaction and cross-platform qualification

The registry retains at most 256 project identities, eight physical identities per
locator, and 1 MiB. It does not evict history to admit a new resource. This closes
the project/overlay observation milestone; the full Phase 1 gate and autonomous
executor/reviewer coordination remain open.

Validation: 21 work-unit/native-adapter tests, 6 runtime action/schema tests, and
25 shared persistence/confinement tests passed. The work-unit suite ran with
`--test-threads=1` after a parallel sweep encountered retryable vault custody
overload; parallel qualification remains outstanding. Strict clippy over all
targets of `medousa`, `medousa-store`, `medousa-work`, and `medousa-types`, strict
docs verification, and diff whitespace checks passed. The embedded library
compile check passed with its existing feature warnings. Fixtures cover actual
repositories, many-to-many links and inverse queries, readiness invalidation,
physical replacement, retained relocation/history, interrupted graph publication,
owner/read-only admission, stale environment pins, and confinement. Full workspace
CI, frontend CI, and Windows/wasm qualification have not run; no frontend files
changed and no PR has been opened.

### Native artifact, component, and feed milestone — 2026-10-02

`work.resolve_content` now observes existing artifact payload revisions,
profile-owned environment components, retained feed streams, and previously
observed exact references. Admission derives the domain from the frozen turn;
requests cannot supply a profile, owner, native revision, availability, path,
or payload. Artifact sources retain native chat visibility checks. Work lifetime
remains independent of the chat used to ask for an observation.

References preserve three distinct native kinds. Artifact identity includes the
full source session and exact native artifact ID, avoiding legacy short-session
collisions. The adapter does not follow aliases, prefixes, latest chains, or
cross-session fallbacks. Component/feed identity preserves each native logical
profile-scoped key, without inventing a physical-incarnation guarantee. Missing
resources refresh unavailable; metadata changes invalidate pinned readiness.
Artifact coverage proves index metadata and file presence, component coverage
proves configuration, and feed coverage proves the daemon's retained stream.
None proves rendered correctness, execution, review, or completion.

Component observations expose exact feed references and report configured artifact
IDs/aliases as unresolved until the source session is known. Artifact observations
expose source-chat lineage. Native bindings are current metadata, not automatically
inferred intent edges. Existing `work.record` links connect these resources with
vaults, projects, and work units, and remain queryable after restart or native loss.
Feed producer refs do not silently become verified graph relationships.

Publication holds daemon native custody through the graph commit. Unchanged facts
replay the original receipt; post-publication interruption recovers without
republishing HTML, appending feed events, or repeating file effects. Native read or
parse errors fail closed. File artifact scans are bounded at 4 MiB, component specs
at 4 MiB, and feed logs at 16 MiB; component binding responses cap feed references
at 128. Responses exclude HTML, binary bodies, component config bodies, feed payloads,
and summaries. Native `user:…` feed profiles now use their canonical opaque storage
keys even when no legacy filesystem path can represent that profile.

- [x] Separate native artifact, component, and feed graph identities
- [x] Exact owner-bound observation with revision/readiness checks
- [x] Native binding metadata without guessed cross-session artifact edges
- [x] Restart replay, publication custody, and durable cross-resource relationships
- [x] Bounded local native execution identity/admission and executor → reviewer coordination (see next milestone)
- [ ] Alias reconciliation, subscriptions, and automatic maintenance propagation
- [ ] Physical-incarnation/deletion history and multi-process content writer custody
- [ ] Full workspace, parallel, and cross-platform qualification

This closes the artifact/component/feed observation milestone. The Phase 1 exit
gate and autonomous coordination remain open; existing UI and native stores stay
in place. A runtime observation does not create a work unit, schedule, executor,
reviewer, or contact delivery.

Validation: 31 work-unit/native-adapter tests, 6 runtime action/schema tests,
14 artifact-store tests, 8 feed-store tests, and 3 environment-store tests passed
(62 distinct tests). Work-unit qualification used `--test-threads=1`; full parallel
qualification remains outstanding. Strict clippy over all targets of `medousa`,
`medousa-store`, `medousa-work`, and `medousa-types`, strict docs verification, and
diff whitespace checks passed. The embedded library compile check passed with its
32 existing feature warnings. The final component-reader change was rechecked by
strict clippy and the work-unit suite, including corrupt/oversized persisted specs
with a warm native cache. Fixtures also cover real native stores, legacy ID
collisions, source-session and owner isolation, cross-resource links after restart,
readiness invalidation, append/put publication custody, and post-commit replay.
Full workspace/hermetic CI, frontend CI, Windows/wasm, and live Surreal-backed
artifact observation qualification remain outstanding; no frontend files changed
and no PR has been opened.

### Native executor → reviewer milestone — 2026-10-02

`work.coordinate` registers durable execution/review intent independently of an
originating chat. `work.coordination` inspects it from any admitted owner chat.
Plans bind the admitted domain, scope generation and resource versions, exact
native proposals/assignments, one governed Forge work, and a bounded deadline.
Pending proposals can be registered; each stage still requires its existing
operator approval and exact grant. Existing approve/dispatch surfaces queue a
registered stage with a nullable binding; no UI changes or new grants are added.

The daemon recovers plans through bounded pages and nonblocking coordinator
leases. Registration and dispatch share assignment custody, including direct
native dispatch, so torn registration cannot launch outside the controller.
Unknown claims are retained without replacement launches. Work-owned terminal
events do not admit source-chat AI continuations or contact delivery.

After a completed executor receipt, the runtime pins the observed clean governed
checkout's branch, generation, and full HEAD. Review requires an explicitly
approved `medousa-work-review-v1` contract and a strict JSON verdict tied to that
exact receipt/revision and the unit's completion condition. Native completion
alone cannot satisfy work. Approved current revisions publish both actual native
Assignment references; changes requested, failed execution, invalid review,
changed revisions, and elapsed deadlines publish `needs_attention`. Saved native
results replay publication after interruption without repeating provider effects.
Model state claims cannot bypass native review, including after rescoping.

- [x] Durable local executor/reviewer plan, native grants and stage fencing
- [x] Exact revision review, native assignment evidence and result publication
- [x] Restart/uncertain-launch/duplicate-wake/publication recovery qualification
- [ ] Metered/composite/maintenance execution and automatic revision loops
- [ ] Muse/Instinct/Dots/bot adapters, federated work and contact/voice delivery
- [ ] Full workspace, live provider, cross-platform and live Surreal qualification

This closes the bounded local native execute/review milestone. It supports the
existing Codex, Cursor and Hermes native adapters, standalone finite units, and
at most two new effects. Units with children/dependencies, reservations, or cost
budgets (including ancestor budgets) fail admission while ACP cost is unknown.
The Phase 3 exit gate remains open for broader adapters and acceptance evidence.

Validation: 273 daemon tests (two ignored), seven runtime action/schema tests,
78 ACP coordination/client tests, and 21 work-store tests passed (379 distinct
passing tests). Daemon qualification used the final compiled test harness with
`--test-threads=1` after recovering disk space; the first final run exhausted the
disk and its filesystem failures were discarded only after successful rerun.
The eight execute/review integration fixtures use real native stores, Forge and
Git with fake provider effects, including fast completion, uncertainty, duplicate
wakes, revoked approval, pause/cancel/rescope fences, changed revisions and
interrupted result publication. No paid/live provider run was performed.
Strict clippy passed over all targets of `medousa`, `medousa-types`,
`medousa-store`, `medousa-work`, and `medousa-acp-client`; strict docs, explicit
changed-file formatting and diff whitespace checks passed. The embedded library
compile passed with its 32 existing feature warnings after the disk recovery.
Full workspace/hermetic CI, frontend CI, parallel vault qualification, Windows/wasm and live Surreal
remain outstanding. No frontend files changed and no PR was opened.

### Phase exit gates

- [ ] Phase 0 — contract and source audit
- [ ] Phase 1 — resource identity and relationship store
- [ ] Phase 2 — accepted work scopes and composition
- [ ] Phase 3 — work intake and execution adapters
- [ ] Phase 4 — any authorized participant and federated coordination
- [ ] Phase 5 — maintenance and event propagation
- [ ] Phase 6 — contact policy and useful presentation
- [ ] Phase 7 — compatibility and operational closeout

Phase completion requires the corresponding exit evidence; implemented storage
contracts alone do not establish autonomous execution or live provider acceptance.
