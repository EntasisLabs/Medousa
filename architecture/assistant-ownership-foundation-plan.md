# Assistant ownership foundation

Status: implementation plan; no runtime capabilities are added by this document.
Date: 2026-09-17.
Branch: `codex/assistant-ownership-foundation`, based on the validated iOS Live branch.

## Product decision

Medousa is a platform for context continuity. Assistant is the owner of delegated
intent across conversations, agents, workshops, devices, and interaction modes.
It is not a second personality, a replacement runtime, or an always-running model.
Existing agent turns do the reasoning; existing execution authorities do the work.
Closing Live ends audio transport, not ownership of accepted work.

The first foundation must let Medousa initiate external-agent work, adopt existing
work, retain responsibility, wake later, and return an attributable outcome.
The mobile client controls and presents this world; filesystem and execution
authority remain with the relevant workshop daemon.

## Reference scenario and boundaries

The user leaves a project running with Codex, starts Live from a phone, and asks
Medousa to send the completed changes for review. Medousa adopts the exact work,
waits for completion, assigns review to another peer, routes permitted fixes back,
and returns the verified result. The user can leave Live throughout this process.
Medousa later offers a topic-bearing invitation to resume Live with the outcome.

Local Cursor ACP is the initial review backend. Cursor Cloud is a distinct future
adapter, not something the existing local adapter can be represented as doing.
Route-aware errands, maps integration, automatic voice outreach, deployment,
agent marketplaces, and automatic compute provisioning are not foundation scope.

## Existing implementation to extend

- `src/workshop_api.rs`: location-neutral status, targets, spawn, cancel, steer;
  execution routing and authorized inventory.
- `src/workshop_contract.rs`, `src/delegated_task.rs`: placement, semantic worker
  contracts, correlation, and remote execution admission.
- `src/agent_runtime/turn_worker/`: persistent records, worker execution, cohort
  intake, and a host tool-loop resumed with worker receipts.
- `src/daemon/agents.rs`, `crates/medousa-acp-client/`, SDK `agents` accessors:
  external ACP creation, prompting, streams, cancellation, and permissions.
- `src/runtime/agent_platform/`, `src/runtime/stasis_wire.rs`: ACP terminal
  envelopes and waitable handlers. External waits currently use process-local
  storage; this is a known restart boundary, not durable recovery evidence.
- `src/shared_session_catalog.rs`, `src/shared_mode.rs`: human membership-aware
  room indexing over existing session transcripts. Not yet a generic peer roster.
- `crates/medousa-types/src/session.rs`, `src/context_derivation.rs`, context
  pointer tools: authority-qualified context, provenance, and derived sessions.
- `src/recurring_schedule.rs`, `src/recurring_handlers.rs`,
  `src/recurring_agent_turn.rs`, `src/turn_continuation.rs`: scheduling and
  canonical agent execution, persisted continuation and delivery bindings.
- `src/peer_execution_policy.rs`: destination-owned execution grants; pairing is
  not permission to execute, read secrets, or route agents.
- Channel delivery, mesh inbox/receipts, portable Coder handoff, Live delegation,
  and remote Live Activity updates: reuse existing transport and result paths.

These anchors establish primitives, not proof that the reference scenario works
end-to-end. The Stasis `agent_session` handler must not be mistaken for an external
ACP spawn adapter simply because of its name.

## Invariants

1. One Medousa identity and context platform; no parallel assistant memory store.
2. Every accepted assignment has a durable owner and an exact execution binding.
3. Every command/event has actor, authority, causation, correlation, and dedupe ids.
4. Channel membership grants visibility only within its scope, not execution rights.
5. External messages and tool outputs are untrusted data, never new permissions.
6. An acknowledgment, disconnect, or participant departure cannot implicitly
   approve an action, cancel work, or suppress a pending result.
7. No success claim without a terminal receipt and relevant verification evidence.
8. Existing single-session serialization remains intact; concurrent peers use
   distinct execution sessions and serialized owner-event intake.
9. Scheduling and autonomous continuation have bounded authority and budgets.
10. Offline/unavailable workshops remain visible as unavailable; never silently
    substitute another host, agent, or model contrary to the requested constraints.

## Implementation progress

Slice 0 has an initial, additive contract seam in
`crates/medousa-types/src/coordination.rs` and an adapter-independent admission
harness in `crates/medousa-acp-client/src/coordination.rs`. It covers exact
owner/channel/context/target/executor bindings, separate owner and executor
sessions, unavailable adapters, and an injected authorization boundary.
The fake harness exercises Codex, Cursor, and Hermes without starting processes.

The next increment adds create-only channel snapshots, assignment/command claims,
and peer execution bindings in `coordination/store.rs`, using the existing
capability-confined `medousa-store` root. Records survive reopen, exact replay is
idempotent, concurrent claims have one winner, and conflicting/corrupt records
fail closed. An unresolved claim is not a license to relaunch an external agent.
The synchronous store must be called through host execution admission from async
services. Initial snapshots are immutable; membership edits are not yet exposed.

`src/daemon/coordination.rs` now connects local dispatch to the shared ACP create
and prompt services, using admitted storage/Forge operations rather than a second
client or model loop. Explicit operator grants bind the entire immutable request
and expire within 24 hours. Dispatch checks authenticated ownership, local host
identity, attached source visibility, contiguous committed ranges, canonical
range digests, context budgets, and the selected Forge work. Each assignment has
a deterministic independent executor session. Approval is rechecked after
discovery and provider startup; explicit revocation cancels known live custody.
Expiry limits admission, not an automatic wall-clock termination of running work.

Persistent command claims precede provider effects. Exact recorded replay works
even when the runtime becomes unavailable; unresolved or malformed outcomes
require reconciliation rather than automatic respawn. A binding persistence
failure cancels known custody while retaining the uncertain claim. Focused fake
adapter and store tests cover replay, concurrent claims, restart, exact grant
scope/expiry/revocation, and source visibility/provenance/budgets.

This is an internal service seam, not yet a boot-composed user flow or public HTTP
surface. Human approval UI, model-facing tools, boot/retry-host composition,
restart reconciliation, and autonomous follow-up commands remain to be wired. The raw
admission port alone does not claim persistence or exactly-once execution. No
model-facing peer spawn tool is advertised yet. Real provider integration and
device presentation still require validation beyond the fake harness.

The next internal increment persists immutable terminal receipts against recorded
peer custody before owner wakeup. Duplicate/out-of-order lifecycle events cannot
replace a terminal or start a second intake. Receipt capture remains permitted
after dispatch approval expiry/revocation so evidence is not lost; owner intake
requires a separate exact operator grant, current ownership/source visibility,
and unrevoked assignment authority. Idle timeout is interrupted, not proof of
peer completion. A completed receipt means the prompt ended, not verified Forge
work completion. Each delegated custody accepts only one prompt.

Owner intake holds a nonblocking cross-process fence scoped to the owner session
across channels and persists a turn attempt before calling the canonical
turn-ticket/runtime path. Initial continuation is result-only: no tools, no
manufactured user transcript turn, and no inherited operator capabilities.
Consumption requires successful terminal delivery plus an execution-correlated
committed assistant decision reference/digest. Known pre-execution admission
rejections can retry within a fixed budget; timeouts and uncertain started turns
cannot blindly rerun. Completion wakeups retry busy sessions with bounded host
backoff. Pending receipts and acknowledged decisions survive store reopen;
internal drain/resume ports are ready for a future startup/retry host, but are not
yet boot-composed. A crash between owner execution and acknowledgment requires
explicit reconciliation; process-local turn tickets are not restart evidence.
Receipt/prompt/scan budgets fail closed rather than hiding missing evidence.

Service extraction must preserve the existing ACP creation path's Forge leases,
permission/secret routing, event pump, and cancellation behavior. Its current
single-agent-per-chat registry cannot be reused as a shared multi-peer channel
session; each peer requires an independent executor session. Missing-runtime and
development-stub behavior must stay distinguishable from real accepted work.

## Channel and context contract

Use a logical coordination channel, distinct from an ingress/delivery channel such
as Telegram, Discord, or Home. Naming in public APIs must make this distinction
explicit; do not reinterpret existing channel mapping keys in place.

Proposed durable records, reusing existing stores where their semantics fit:

- Coordination channel: authority-qualified id, owner principal, title, lifecycle,
  member grants, attached session references, and ordered coordination events.
- Participant: stable peer/principal id, human/Medousa/internal/external kind,
  workshop authority, runtime adapter, lifecycle, and scoped visibility.
- Session attachment: channel id, `SessionRef`, purpose, sharing policy, and context
  range/cursor. Attachments reference transcripts rather than copying all history.
- Assignment: stable id, owner peer/principal, executor peer, intent, source context
  manifest, work/project binding, execution ids, grant reference, budgets, status,
  receipt references, and delivery binding.

An external peer has its own runtime session. Publish its attributed output into
the channel through a canonical transcript/event path. Never relabel it as human
speech or merge all participants into one mutable ACP session. Existing memory,
chat search, and pointers resolve channel-attached sessions subject to membership
and original source authority; attaching a session cannot launder private context.

Human rooms remain available without requiring a user to convert a Personal
installation into an organization/team brain. Reuse authorization primitives,
not the assumption that every coordinating channel is an org room.

Join/leave/pause/end are distinct. Leaving stops future participation but does not
cancel an assignment. Cancellation is explicit and authorized. Revoked access
blocks subsequent context reads, tool actions, and result publication as policy
requires; retained audit records have explicit retention rules.

## Peer delegation contract

Expose model-facing typed actions through the existing schema/tool conventions,
backed by a shared service rather than tools calling daemon HTTP handlers.
Candidate action families (names provisional until public-schema review):

- channel create/inspect/attach-session/join/leave;
- peer discover/create/inspect/pause/end;
- assignment create/adopt/inspect/steer/cancel;
- schedule create/inspect/update/pause/cancel.

Discovery returns installed, authenticated, authorized, reachable adapters and
targets, including explicit unavailable reasons. Missing optional binaries lead
to Settings → Packages, not invented success or automatic installation.

Creation binds an adapter, target, channel, participant identity, and execution
policy. Assignment creation is idempotent and captures the exact context and
project/checkpoint. Adoption references an existing authorized work/execution id;
it does not restart the work or infer ownership from a title. Observe-only versus
steer/cancel authority must be separately represented.

External prompt/cancel/permission behavior uses the existing ACP service and SDK
contracts. A channel peer is not automatically a mesh device peer; adapters and
device identities remain distinct. Internal and external executors publish the
same normalized assignment events without forcing identical runtime internals.

## Autonomous owner continuation

Autonomy is event-driven reasoning under delegated authority, not a model polling
forever. Terminal peer events, scheduled occurrences, authorized human messages,
and selected progress/stall events feed a durable owner inbox.

Each event is persisted before acknowledgment. An owner intake lease/fencing
boundary serializes consumption and checks current grants. It resumes the
canonical `run_agent_turn` path with referenced context and receipts. The event
is consumed only when the resulting decisions/commands are durably recorded.
Replay must dedupe commands, not merely dedupe rendered messages.

If the owner session is busy, retain and retry intake instead of dropping the
event or attempting another active interactive turn. Reuse host-resume/cohort
mechanisms where possible, extending their scope beyond one parent spawn cohort.
Prevent reflexive peer-message loops: only addressed assignments and subscribed
events wake agents; enforce causal-depth, review-round, cost, and elapsed-time
limits. On exhaustion, report a blocked outcome with evidence and a user choice.

The Codex/reviewer cycle carries explicit permission to review and perform bounded
fixes in the designated work context. Publishing, merging, deployment, unrelated
changes, and new access grants are not implied. Notifications summarize real
state, not an unverified intermediary promise.

## Scheduling contract

One agent-facing scheduling interface supports a relative delay, an absolute
timestamp, or recurrence. Store an absolute UTC due time for one-shot work and an
explicit timezone for wall-clock recurrence. Return the resolved time to the user.
Reuse Stasis jobs, recurring definitions, canonical scheduled agent turns, and
delivery bindings; do not build a parallel timer service.

One-shot means one logical occurrence, not one execution attempt. Atomically
materialize/claim the occurrence and mark its definition exhausted so scheduler
ticks cannot create another occurrence. Failure retries belong to that same
occurrence. A terminal failure remains inspectable; it must not quietly recur.
`max_attempts` stays a retry budget, not a schedule firing count.

Persist schedule id, generation, owner, channel/session, instructions/context refs,
grant reference, due/recurrence specification, occurrence limit, retry policy,
missed-run policy, delivery policy, and state. Pause/cancel/update races use a
generation boundary. Define behavior for already-dispatched work separately from
future firings. Delayed schedules run once after recovery unless explicitly
expired; missed recurrence coalesces by default rather than flooding the owner.

Use completion subscriptions for known agent work; a scheduled check is a watchdog
or intentional check-in, not the main completion-detection mechanism.

## Recovery, security, and resources

- Persist external execution bindings and terminal receipts. Replace/extend the
  process-local wait boundary or reconcile it from durable assignment records.
- After restart, query adapter capability to resume/reconcile. If unsupported,
  mark execution interrupted/unknown and ask before repeating consequential work.
  A persistent record does not mean the external process itself survived.
- Recheck destination execution policy and membership on every resumed action.
  Use existing permission/secret request surfaces; keep keys out of transcripts.
- Durable outbound commands have idempotency keys and receipts. For adapters that
  cannot dedupe an uncertain side effect, reconcile or request user intervention;
  do not promise globally exactly-once external execution.
- Keep identity-qualified placement and work bindings across workshops. Cross-host
  context transfer is explicit; returned code uses existing Forge checkpoint and
  reconciliation authority. Reviews must refer to a concrete code revision.
- Limits cover active peers, concurrency, tokens/cost, wall time, wake frequency,
  retry attempts, review rounds, and artifact retention. No new compute purchase
  or provisioning is authorized by Assistant ownership alone.
- Audit who assigned, adopted, approved, executed, changed state, and notified.
  Context deletion and membership revocation propagate to indexed references.

## Delivery and Live re-entry

Route outcomes through existing delivery/outbox infrastructure. Separate peer
progress from owner conclusions and user-action requests. Dedupe updates and avoid
notifying on every tool call. The phone must not host the ownership coordinator
as a prerequisite for remote work to finish.

A future Live invitation carries channel, topic, assignment/result references,
and an authorization-checked deep link. The user explicitly accepts; the app seeds
Live from canonical context and verified outcomes. Do not automatically activate
the microphone or promise to restart Siri. Initial foundation delivery is a normal
notification/channel result; invitation UX follows after ownership is reliable.

## Implementation sequence and gates

### Slice 0 — lock contracts and seams

Trace services beneath ACP handlers, worker ownership, scheduling admission,
shared-room authorization, and delivery. Write contract tests for unavailable
adapters, identity scope, and existing behavior. Decide reuse versus additive
records from actual persistence semantics. Exit: reviewed typed contracts and a
small end-to-end fake-adapter harness; no new agent model loop.

### Slice 1 — channels and external peers

Add logical channel/participant/session bindings and the model-facing external
peer bridge. Support one local ACP peer at a time, then Codex/Cursor/Hermes adapter
parity. Exit: Medousa—not a UI-only caller—discovers, creates, assigns, observes,
and cancels a peer; attributed output and permission requests reach the channel.
No personal context leaks to another member or executor.

### Slice 2 — adoption and durable owner intake

Adopt existing work, normalize receipts, and continue the owner after completion
with busy-session deferral and restart reconciliation. Exit: close the client,
complete peer work, restart/reconnect, and obtain one attributable owner decision
without duplicate assignment or a stuck active turn.

### Slice 3 — scheduled self-waking

Add delay/at/recurrence controls with occurrence limits and generation fencing.
Exit: “check in 30 minutes” fires one logical occurrence, retries safely, exhausts
itself, and remains inspectable. Cancellation, downtime, timezone changes, and
concurrent scheduler ticks pass deterministic-clock tests.

### Slice 4 — bounded review coordination

Compose existing peers and owner events into Codex → reviewer → permitted fixes →
review. Exit: exact revision/evidence binding, independent runtime sessions,
bounded loops, failure/cancel/revocation handling, and no unauthorized publish.

### Slice 5 — coherent presentation and return

Present participants, assignments, permissions, outcomes, and notifications without
dumping orchestration prompts into chat. Add topic-bearing Live re-entry. Exit:
accepting an invitation resumes verified context; ending Live never ends accepted
work; reconnecting never narrates an obsolete outcome as current.

No giant mode UI or broad autonomy toggle before these gates. Capability and
ownership limits should be inspectable before Assistant is presented as ready.

## Verification and rollout

Use fake ACP adapters, deterministic time, persisted-store integration tests, and
delivery spies first. Test duplicate/out-of-order terminals, crashes before/after
claim and dispatch, stale permissions, unavailable workshops, member departure,
private session attachment, deletion, uncertain side effects, busy owner sessions,
parallel assignments, and watchdog/completion races. Real local adapters then
validate protocol and auth behavior; on-device tests validate presentation and
Live, not the durability of server ownership.

Maintain HTTP/SDK parity for new contracts, update generated SDK/OpenAPI artifacts,
and add canonical engine/SDK documentation. As user flows ship, add guides and
index them in `docs/README.md`; this architecture plan is not a shipped guide.
Before PR, run the repository's complete AGENTS.md CI parity suite and strict docs
verification. Ship foundation slices behind explicit capability exposure; preserve
existing chat, Live, room, and automation contracts throughout migration.

## Deferred design choices, with initial defaults

- Storage placement: prefer additive records using existing runtime stores;
  finalize in Slice 0 rather than modifying every transcript schema prematurely.
- Public naming: logical coordination channels versus delivery channels must be
  distinguishable; use existing tagged-action conventions, not a second catalog.
- Initial deployment: local authorized ACP adapters first; remote targets follow
  established admission policy. Cursor Cloud requires its own verified adapter.
- Peer visibility: least-privilege selected context by default; broader context
  requires a scoped grant, never blanket channel-to-vault access.
- Ownership transfer: one authoritative coordinator per assignment initially;
  multi-owner failover requires explicit transfer/fencing, not replicated prompts.
- External execution guarantees: capability-based resume/reconciliation, with
  honest interrupted state where the adapter cannot recover.

## Definition of foundation complete

A user can ask Medousa to coordinate existing or newly delegated agent work in a
channel, end the interaction, and later receive a verified result. Medousa can
wake on completion or a one-shot schedule, coordinate a bounded follow-up across
authorized peers/workshops, survive supported reconnect/restart paths, and ask
for additional authority when required. Every participant, context handoff,
decision, execution, and delivery remains attributable and inspectable.
