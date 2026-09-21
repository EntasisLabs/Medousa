# Assistant ownership foundation

Status: locked phased implementation plan; no runtime capabilities are added by this document.
Date: 2026-09-17; rebaselined against the runtime on 2026-09-21.
Branch: `codex/assistant-ownership-foundation`, based on the validated iOS Live branch.

## Product decision

Medousa is a platform for context continuity. Assistant is the owner of delegated
intent across conversations, agents, workshops, devices, and interaction modes.
It is not a second personality, a replacement runtime, or an always-running model.
Existing agent turns do the reasoning; existing execution authorities do the work.
Closing Live ends audio transport, not ownership of accepted work.

Assistant is a first-class user-selected mode and a strict behavioral superset of
General, not a second runtime. It keeps the same Medousa identity, memory, chat,
and host-orchestrated lane while adding an ownership policy and, as the following
slices land, durable coordination tools. General retains its existing lower-
autonomy contract. Selecting Assistant never creates a grant or bypasses an
approval boundary; it enables Medousa to use authority the principal and target
workshops have already made available.

The existing mode picker is the sole routine explanation surface: one concise
description establishes that Assistant owns work across agents, workshops, and
time. The composer, transcript, Live UI, activity feed, and notifications should
otherwise behave normally. Do not add persistent autonomy banners, warning
chrome, or repeated permission copy. Concrete approvals and blockers remain
visible at the moment they matter.

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

The first product-facing seam is the Assistant mode contract itself. It reuses
General's host lane, context path, identity, and completion scheduler while
selecting an explicit ownership policy. Full and embedded daemons expose the same
mode and Home explains it only in the existing picker. This establishes the
authority boundary without claiming that remote ACP creation or autonomous
chains are already available; those arrive through the gated slices below.

The runtime now enforces that boundary at the model-visible tool registry as
well as in catalog metadata. General and Teacher retain their existing surface,
including read-only active-work inventory and ordinary workshop execution, while
external-peer discovery and immutable proposal creation are visible only to
Assistant. Assistant's policy directs it to inspect work before choosing compute
and distinguishes Medousa workshop execution from Codex/Cursor/Hermes custody.
This is a surface ceiling, not an execution grant: all existing proposal,
operator-approval, destination-policy, and receipt checks remain authoritative.
The signed cross-workshop coordination adapter now turns an exact authorized
inventory row into an immutable proposal on the workshop that owns it. Mobile
Assistant transfers only a bounded digest-checked context grant and intent over
the pinned mesh. The destination rechecks directional policy, agent targeting,
runtime identity, Forge ownership, and source provenance; it materializes a
request-scoped derived session so repeated requests from one phone chat remain
independent. Home projects those proposals back through the originating chat
and routes approve/start to the exact workshop. Existing proposal decisions and
execution admission remain authoritative. A verified terminal can now wake one
bounded Assistant continuation with an exact discover/propose-only tool ceiling.
It may prepare one user-requested follow-up handoff against the same governed
work, but every launch remains behind a separate approval card. On-device
end-to-end validation of this chained loop is still pending.

The General host now registers a `peers` domain for conversation-bound local
agent discovery and immutable proposals. It derives identity, bound Forge work,
and committed context digests from the admitted owner turn; model input cannot
issue grants or launch agents. Home's exact-snapshot approval/start controls
remain the operator boundary. This slice still needs on-device end-to-end
validation and does not add cross-workshop/cloud delegation or autonomous chains.

Discovery and proposals now also admit exact adoption of a visible, unowned ACP
session already running against the same governed Forge work. The running prompt
pump and registry share observer and terminal state, so adoption never resends a
prompt and completion racing observer attachment is retained. Initial adoption
is limited to the live local process registry; reconstructing provider custody
after daemon restart remains a separate adapter-reconciliation problem.

The owner agent now has a read-only active-work inventory tool that answers the
first half of “what are we working on?” without requiring the chat to already be
bound to a project. It lists the authenticated principal's non-terminal Forge
work in the current workshop and joins visible ACP custody to each exact work id,
including whether it is already Medousa-owned or adoptable. The response carries
an explicit coverage marker. The same read contract now federates from the
mobile owner across configured paired workshops through signed mesh envelopes.
Each destination rechecks its directional execution policy and resolves its own
active workshop identity. Offline/erroring workshops remain explicit, and the
aggregate is complete only when every configured workshop answered. Cross-
workshop adoption and delegation from an inventory row remain the next boundary.

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

This now has conversation-bound discovery/proposal tools and a native operator
approval preview, not yet a validated end-to-end conversational delegation flow.
Explicit restart reconciliation and autonomous follow-up commands remain to be wired. The raw
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
the desktop daemon now composes a startup/retry host (four concurrent intake
workers, 30-second busy retry, five-minute unavailable-approval backoff). A paged
local-authority/runtime inbox prevents blocked receipts starving later work.
This host never relaunches peer processes from old claims. Monitor timeout and
shutdown cancel only the exact owner turn admitted by the intake worker.
After restart, an exact attributed assistant decision already committed to the
canonical owner transcript can reconcile and acknowledge its intake without the
process-local turn ticket. A missing ticket without that durable decision remains
explicitly unresolved and is never rerun blindly.
Receipt/prompt/scan budgets fail closed rather than hiding missing evidence.

The operator-approval backend persists content-addressed assignment proposals
before decisions. Each assignment has one immutable proposal snapshot, including
expiry and the separate owner-continuation choice. Approval and denial are
immutable owner-scoped decisions; denial cannot be reversed into approval.
Approval revalidates current source visibility and compiles exact grants
idempotently, so partial writes can finish without changing the approved request.
Proposal creation itself grants no execution authority. Dispatch reads the
approved stored snapshot rather than mutable client instructions. Home approval
presentation now appears in the owning Home chat on workshops advertising the
operator-proposal capability, with separate approve and start actions. Native
operator HTTP routes and generated SDK operation/schema tables expose the same
immutable workflow; no mutable grant payload is accepted. A bounded indexed
owner-session inbox excludes denied and recorded peer assignments. Model-facing
proposal/delegation tools remain the next slice; this is not yet conversational
agent spawning. Mobile embedded-daemon composition is not claimed here.

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

## Locked implementation sequence and gates

This sequence is rebaselined against the 2026-09-21 runtime audit. It supersedes
the older five-slice ordering. The architecture above remains the target; these
phases define the smallest dependency-safe path from the current runtime to it.
A phase may be developed behind an unadvertised capability, but it does not count
as complete until its exit gate passes. Later phases must not invent parallel
ownership, scheduling, delivery, or voice systems to bypass an earlier gate.

### Current baseline

The repository already has real execution primitives: canonical interactive and
scheduled agent turns, parallel and bound workers, durable Stasis jobs, workflows,
recurring schedules, ACP sessions, governed Forge work, paired-workshop mesh
routing, delivery bindings, and Home/Live presentation. Assistant mode also has
initial proposal, approval, adoption, receipt, and bounded continuation seams.

Those primitives do not yet form one durable assistant coordinator. Ownership is
spread across subsystem-specific records, and orchestration is still primarily
assembled inside an active model turn. The immediate routing fix adds an explicit
`needs_synthesis` handback and avoids daemon-default fallback, but host synthesis
still derives its route from resolved worker execution rather than a durably
preserved parent `final_response` contract. That defect is Phase 0, because every
later owner wakeup depends on trustworthy return routing.

### Phase 0 — preserve the parent continuation route

**Outcome:** delegated work always returns to the parent assistant through the
route selected for final user-facing synthesis, independent of the worker model.

- Introduce a versioned continuation-route value carrying at least stage role,
  route profile, provider/model intent where pinned, and safe fallback policy.
- Capture it when the parent turn accepts/delegates work; persist it with the
  worker/assignment continuation rather than reconstructing it from daemon
  defaults or the worker's resolved route.
- Make bound-workshop synthesis, parallel-worker host resume, restart reconcile,
  and delivery retry consume the same contract.
- Preserve compatibility for old records by using an explicit legacy fallback;
  never silently treat the worker route as the desired final-response route.
- Keep `needs_synthesis` structural. Prose, worker success, and transport delivery
  are not substitutes for a terminal host decision.

**Likely seams:** `daemon_interactive_turn.rs`, `turn_worker/{run,store,status,
registry,host_resume}.rs`, `turn_api.rs`, and the stage-routing types used by the
canonical turn path.

**Exit gate:** tests use deliberately different host and worker models and prove
that direct completion, parallel cohort completion, bound work, busy-session
deferral, daemon restart, and retry each produce one final answer on the preserved
parent route. Legacy records have deterministic fallback behavior. No path falls
back to a tiny/default model merely because worker execution completed there.

### Phase 1 — one Assistant assignment ledger over existing executors

**Outcome:** Assistant can answer “what do I own, who is doing it, where is it
running, what authority allows it, and what happens next?” without scraping four
unrelated stores inside a model turn.

- Define one versioned `AssistantAssignment` projection with stable assignment,
  owner, principal, source session/channel, execution binding, workshop, work id,
  grant, budget, lifecycle, receipt, delivery, and continuation references.
- Project existing internal turn workers, ACP/external sessions, workflows/jobs,
  and scheduled occurrences into that contract. Do not rewrite their execution
  engines or duplicate their native state.
- Define lifecycle transitions and terminal precedence centrally. Unknown,
  interrupted, unavailable, blocked, and awaiting-approval remain distinct from
  failed or completed.
- Make commands idempotent and causally attributable. Persist command claims
  before effects and reconcile uncertain effects instead of blind relaunch.
- Provide bounded list/get/event APIs suitable for both owner reasoning and UI.

**Likely seams:** existing coordination records/store, `turn_worker/store.rs`,
runtime job/workflow APIs, recurring records, ACP custody registry, and Forge work
bindings. Native subsystem ids remain visible as execution references.

**Exit gate:** one query returns mixed internal, external, scheduled, and workflow
assignments with exact provenance. Reopen/restart preserves identity and status;
duplicate and out-of-order events cannot regress a terminal. Existing subsystem
APIs and execution behavior remain compatible.

### Phase 2 — general durable owner inbox and serialized continuation

**Outcome:** accepted work can wake its owning Assistant later, even after the
originating interactive turn, client, or daemon process is gone.

- Generalize the current peer-receipt intake into an owner-event inbox accepting
  assignment terminals, approvals, addressed human messages, schedule occurrences,
  selected stalls, and delivery failures.
- Persist each event before acknowledgment and serialize intake by owner session
  with a lease/fencing boundary. Busy sessions defer; they do not drop the event
  or start a competing turn.
- Resume the canonical agent-turn path with bounded context and exact receipts.
  An event is consumed only after resulting decisions/commands and terminal
  delivery are durably correlated.
- Add causal-depth, wake-frequency, elapsed-time, cost, retry, and review-round
  limits. Exhaustion produces an inspectable blocked state and a user decision.
- Reconcile committed decisions after crashes. Never rerun an uncertain started
  continuation solely because a process-local ticket disappeared.

**Dependency:** Phase 0 continuation routes and Phase 1 assignment identity.

**Exit gate:** close the client, complete work, restart the daemon, and receive one
attributable owner decision with no duplicate command or transcript turn. Busy,
crash-before-claim, crash-after-claim, timeout, replay, and poison-event tests are
deterministic and do not starve unrelated assignments.

### Phase 3 — normalized execution adapters and placement policy

**Outcome:** Assistant chooses among inline work, internal workers, workflows,
local ACP agents, and authorized remote workshops using explicit policy rather
than prompt folklore.

- Define a placement request from task needs: capabilities, context locality,
  governed work, trust boundary, latency, cost, persistence, interactivity, and
  requested/forbidden executor constraints.
- Return ranked eligible targets with reasons and explicit unavailable causes.
  Selection never grants authority; destination admission, operator approvals,
  Forge leases, secret handling, and peer execution policy remain authoritative.
- Normalize start/adopt/observe/steer/cancel around assignment commands while
  preserving adapter-specific custody and receipts.
- Require exact work/revision/context bindings for code review and fix loops.
  Offline targets stay offline; do not silently substitute another host, agent,
  provider, or model contrary to the user's constraint.
- Start with existing local/internal paths and authorized paired workshops. New
  cloud adapters and automatic compute provisioning are separate capabilities.

**Dependency:** Phases 1–2. Placement may read inventory earlier, but autonomous
placement cannot ship before durable ownership and return intake exist.

**Exit gate:** scenario tests choose the expected target from mixed inventories,
explain rejection/ineligibility, survive target disappearance, adopt exact running
work without resending its prompt, and return through the same owner. No test can
turn discovery, pairing, channel membership, or model text into an execution grant.

### Phase 4 — bounded plans, schedules, and multi-assignment coordination

**Outcome:** Assistant can decompose a user goal into durable assignments and
bounded follow-ups while remaining the single accountable owner.

- Represent plan steps and dependencies as assignment relations, not hidden chain
  of thought or a second workflow engine.
- Reuse Stasis, workflows, and recurring scheduling. One-shot schedules create one
  logical occurrence; retries remain attempts of that occurrence.
- Support event-triggered follow-up, delay/at/recurrence, pause/cancel, generation
  fencing, missed-run policy, and explicit watchdogs.
- Enforce plan-wide authority, budget, deadline, concurrency, and causal-depth
  ceilings. Expanding scope or obtaining new access requires the user.
- First reference flow: implement → exact-revision review → permitted fixes →
  bounded re-review. Publishing, merging, deployment, and unrelated cleanup are
  never implied.

**Dependency:** durable assignments, owner intake, and placement from Phases 1–3.

**Exit gate:** the reference review loop survives disconnect/restart, uses separate
executor sessions, binds every review to an exact revision, stops at configured
limits, and reports blocked/failed/cancelled states honestly. A “check in 30
minutes” test fires once, retries safely, and remains inspectable after exhaustion.

### Phase 5 — contact policy and unified delivery decisions

**Outcome:** Assistant contacts the user only when policy says the user—not another
agent—needs to know or decide something, through the best currently authorized
channel.

- Add a user-controlled contact policy covering urgency, quiet hours, preferred
  channels/devices, escalation, batching, reminders, expiry, and per-assignment
  overrides. Defaults are conservative and inspectable.
- Separate durable owner conclusions, approval/decision requests, progress, and
  raw peer events. Tool chatter and intermediary promises are not notifications.
- Put policy selection above existing channel/outbox/push transports; do not build
  another delivery engine. Persist intent, selected route, attempts, receipts,
  dedupe key, and fallback/escalation decisions.
- Respect channel capability and presence without assuming reachability. Failed or
  unavailable delivery stays inspectable and can re-enter owner intake when useful.
- Give users pause/snooze/mute/cancel controls without implicitly cancelling work.

**Dependency:** Phase 2 owner decisions and Phase 1 assignment state. Placement is
not required for basic contact, but multi-agent product rollout should include it.

**Exit gate:** deterministic policy tests cover quiet hours, urgent approval,
batched completions, duplicate events, offline devices, fallback channels, snooze,
and revoked endpoints. Delivery spies observe at most one logical notification per
policy decision, and every message links to current assignment evidence.

### Phase 6 — explicit Live invitation and coherent return UX

**Outcome:** Assistant can offer a topic-bearing Live conversation when voice is
the appropriate escalation, while the user remains in control of microphone and
session creation.

- Model a Live invitation as a contact-policy action carrying topic, owner session,
  assignment/result references, expiry, and an authorization-checked deep link.
- The user must accept. Acceptance creates/resumes Live from canonical context and
  current verified outcomes; sending an invitation never activates a microphone.
- Decline, expiry, duplicate delivery, reconnect, and already-resolved work have
  explicit behavior. Ending Live ends audio transport, not accepted assignments.
- Present participants, assignment state, decisions, and outcomes without dumping
  orchestration prompts or pretending an external agent spoke as the user.
- Voice calling, PSTN/SIP, wake-word activation, and automatic audio capture are
  separate consent and transport projects, not hidden inside this phase.

**Dependency:** Phase 5 contact policy and the durable owner/session references from
Phases 1–2.

**Exit gate:** an Assistant decision can issue one invitation; acceptance opens the
right topic and evidence on another device; decline/expiry do not loop; reconnect
never narrates stale results as current; work continues after Live closes.

### Rollout order and product gates

1. Ship Phase 0 as a correctness fix with no new autonomy claim.
2. Expose the Phase 1 ledger initially as read-only diagnostics and UI projection.
3. Enable Phase 2 wakeups only for explicit accepted assignments and conservative
   budgets; retain a kill switch and per-owner pause.
4. Add placement targets incrementally, proving each adapter independently.
5. Enable bounded plans only after restart and uncertain-effect tests pass.
6. Default contact policy to completion and decision-needed events; opt into more.
7. Ship Live invitations last, as a contact modality rather than ownership logic.

The user-facing promise is complete only when Phases 0–5 pass: one conversation,
one accountable Assistant, authorized compute chosen intentionally, durable work
tracking, and restrained proactive return. Phase 6 adds voice re-entry to that
reliable ownership loop; it does not make an unreliable loop autonomous.

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
