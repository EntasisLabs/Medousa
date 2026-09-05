# Runtime-owned worlds

> **Status:** Active — architecture locked; Phases 1–4 implemented; Phase 5 driver foundation in progress
>
> **Date:** 2026-09-04
>
> **Related:** [shared browser workspace](shared-browser-workspace.md),
> [agent browser host](agent-browser-host.md),
> [bots and authorized remote execution](bots-and-authorized-remote-execution-epic.md),
> [turn runtime and lanes](turn-runtime-and-lanes.md), and
> [daemon-owned OCI work environments](daemon-owned-oci-work-environments-epic.md)

## Product promise

Medousa does not give an agent control of a human-owned computer. A workshop
daemon owns a governed world and admits authenticated human, agent, Bot, and
peer intent into it. The human is the most privileged principal, but every
participant uses the same runtime authority, causal trace, state model, and
recovery boundary.

Browser use and computer use are adapters beneath that authority. To a user,
the result should still feel like immediate ordinary computer use:

> Use this browser or computer yourself, let Medousa operate it, interrupt at
> any moment, and always know what happened, why it happened, and what can be
> recovered.

This is an extension of the existing Medousa product model. The selected
workshop owns the filesystem, shell, credentials, execution policy, and now
the interactive world. Home remains a viewport and privileged control surface;
it is not the policy authority.

## Executive decisions

These decisions are locked for this epic:

1. **The daemon owns the world.** A browser, desktop, application, terminal, or
   future interactive surface is a daemon-governed resource, not a raw tool
   handed to an agent or client.
2. **Every participant is a principal.** Humans, foreground agents, workers,
   Bots, system services, and paired peers submit attributable intent. No
   participant becomes the owner of the underlying driver.
3. **The human has priority, not a bypass.** Human input may preempt any
   non-human lease immediately, while still entering the same state and audit
   timeline.
4. **Authority and placement are separate.** Choosing the workshop that owns a
   world does not grant a principal permission to observe or mutate it. The
   destination daemon enforces local policy.
5. **Semantic interaction precedes pixels.** Structured integrations are
   preferred over browser semantics, browser semantics over OS accessibility,
   and accessibility over coordinate-only input. Pixels remain a necessary
   observation and fallback channel.
6. **The common path is pre-authorized.** Policy is compiled into scoped,
   expiring capabilities before interaction. An admitted low-risk action does
   not perform a database, network, or model round trip before dispatch.
7. **State is revisioned.** Actions name the world, resource, expected state
   revision, and control generation. Stale work fails closed or records that
   reconciliation is required.
8. **Effects are traced, not noise.** Durable events preserve intent,
   authority, actions, results, and semantic state transitions. High-rate raw
   input and frames use bounded buffers and referenced artifacts.
9. **Irreversibility is explicit.** Recovery means a domain-specific
   checkpoint or compensating action. Medousa never claims that an external
   message, purchase, deletion, or submission can be universally rolled back.
10. **Shared and isolated worlds remain distinct.** A daemon may govern a
    human-visible existing surface or an isolated agent-first environment, but
    their default identity, persistence, and permission postures do not blur.
11. **Drivers do not decide policy.** Browser extensions, webviews, CDP,
    accessibility adapters, and input sidecars execute bounded permits and
    report observations. They cannot mint authority.
12. **One internal contract serves every model provider.** Provider-specific
    tool schemas translate into the Medousa world contract rather than making
    the runtime inherit one model vendor's computer-use protocol.

## Product vocabulary

| Term | Meaning |
|---|---|
| World | A daemon-governed interactive environment and its resources |
| World authority | The workshop daemon that authenticates, admits, traces, and recovers interaction |
| Principal | A human, agent, Bot, worker, peer, or system service acting in a world |
| Resource | A tab, window, application, display, terminal, document, device, or other target inside a world |
| Intent | An attributable request to observe or change one or more resources |
| Capability grant | Scoped, expiring authority compiled by the daemon for a principal |
| Control lease | A revisioned right to issue interactive mutations to a resource set |
| Observation | A revisioned semantic and/or visual account of world state |
| Permit | A single admitted action or guarded batch bound to a lease and expected revision |
| Effect | The observed outcome of executing a permit |
| Driver | A replaceable adapter that senses a surface and applies admitted actions |
| View attachment | A client connection that renders or controls a daemon-owned world |

## Ownership classes

The runtime records how completely it can account for a world:

| Class | Meaning | Guarantee |
|---|---|---|
| Owned | The daemon created and controls the browser or OS session | All admitted interaction is mediated; strongest fencing and recovery |
| Managed | The daemon controls an instrumented surface such as the Medousa webview | Strong accounting inside the surface; the surrounding OS may still mutate independently |
| Attached | The daemon is connected to a pre-existing human browser or desktop | Best-effort observation and reconciliation; no claim of complete mediation |

The product must display this distinction anywhere it materially changes
identity, persistence, recovery, or security.

## Target architecture

~~~text
Human / Agent / Bot / Worker / Peer
                 |
                 | attributable intent
                 v
       Workshop daemon world authority
       +-------------------------------+
       | identity + capability grants  |
       | control and resource fencing  |
       | revisioned state mirror       |
       | action admission              |
       | causal event journal          |
       | recovery and compensation     |
       +-------------------------------+
                 |
                 | bounded permit
                 v
       Driver colocated with the world
       +---------------+---------------+
       | browser       | desktop       |
       | DOM / AX      | OS AX / UIA   |
       | CDP / BiDi    | input driver  |
       | pixels        | pixels        |
       +---------------+---------------+
                 |
                 | observation + effect
                 v
       State mirror / artifacts / views
~~~

There are four runtime planes:

1. **Authority plane:** identity, policy compilation, capabilities, approval,
   lease ownership, and revocation.
2. **Interaction plane:** the local hot path from an admitted intent to the
   resource driver.
3. **Observation plane:** cached semantic state, incremental deltas, frames,
   focus, navigation, console, network, and driver health.
4. **Durability plane:** causal events, bounded raw telemetry, checkpoints,
   artifacts, and recovery records.

The planes share ids and revisions, but durability and remote presentation do
not block ordinary local input.

## Locked world contract

The exact Rust layout may evolve, but the following semantic records are
required.

~~~text
WorldSession {
  world_id
  authority_id
  ownership: owned | managed | attached
  surface: browser | desktop | application | terminal | composite
  revision
  control_generation
  active_control_lease?
  created_at_ms
  updated_at_ms
}

WorldPrincipal {
  principal_id
  kind: human | agent | bot | worker | peer | system
}

CapabilityGrant {
  grant_id
  world_id
  issued_by
  subject
  capabilities[]
  resource_scope
  issued_at_ms
  expires_at_ms?
  revoked_at_ms?
}

ControlLease {
  principal
  generation
  acquired_at_ms
  expires_at_ms?
}

WorldActionIntent {
  intent_id
  trace_id
  principal
  resource_id
  expected_revision
  expected_control_generation?
  required_capability
  effect_class
  idempotency_key
  summary
}

WorldActionPermit {
  world_id
  intent_id
  trace_id
  principal
  resource_id
  admitted_revision
  control_generation?
  effect_class
}
~~~

Rules:

- Observation-only intent does not require a control lease, but does require
  an observation grant.
- Mutating intent requires both the matching capability and current control
  generation.
- The daemon records admission before dispatch and records success, failure,
  cancellation, or indeterminate completion afterward.
- A late result is never silently discarded. If control or state changed after
  dispatch, the effect is recorded as requiring reconciliation.
- Idempotency keys are scoped to a world and prevent an acknowledged external
  effect from being repeated accidentally.
- Driver-provided page, application, and accessibility text is untrusted data.
  It cannot expand policy or mint capabilities.
- Secrets are referenced through the daemon broker. They do not enter model
  observations, action traces, screenshots, or driver payloads unless an
  explicit reveal capability exists.

## Control and concurrency

A world authority is a single logical writer even when its implementation uses
multiple tasks or processes. Resource-level actors may execute concurrently
when their resource sets do not overlap.

Control precedence is:

1. authenticated human administrator;
2. an explicitly delegated human controller;
3. the currently leased agent, Bot, or worker;
4. background automation without an interactive lease.

Human input increments the control generation before it is dispatched. Any
queued non-human permit carrying the previous generation becomes stale and is
cancelled at the next action boundary. A driver must not continue an action
batch after its permit is fenced.

Parallel agents receive separate resources or an explicit scheduler decision;
they do not implicitly share one cursor, focused element, or active tab.

## Observation contract

An observation envelope may contain:

- world, resource, revision, timestamp, ownership class, and driver identity;
- active application, window, tab, URL, title, focus, and navigation state;
- a bounded accessibility or DOM tree with stable opaque element references;
- viewport size, device scale, coordinate frame, and screenshot artifact id;
- console, network, filesystem, process, or accessibility delta cursors;
- confidence, truncation, stale-state, and untrusted-content markers.

The daemon maintains a state mirror from driver events. An ordinary observation
reads the mirror and returns deltas after a known revision. Full trees,
screenshots, video, and logs are out-of-band artifacts fetched only when the
consumer needs them.

Stable element references are scoped to a world resource and observation
generation. CSS selectors and raw coordinates remain advanced fallbacks, not
the primary model-facing identity.

## Action and effect contract

Drivers accept a permit and one action or guarded batch. A batch executes in
order and stops on the first failed precondition or action. Supported semantic
operations grow by adapter, but the common contract includes navigation,
focus, click/activate, text input, key input, scrolling, selection, dragging,
waiting, and observation.

Each action result contains:

- whether the driver accepted and executed the permit;
- pre- and post-action revisions;
- postcondition results;
- changed element/resource references;
- an optional post-action observation or screenshot artifact;
- effect class and approval receipt where applicable;
- confirmed, failed, cancelled, indeterminate, or needs-reconciliation state.

Risk is derived from the resolved target, action semantics, credential use,
origin/application boundary, and expected effect. It is not inferred from
model-authored selector text.

## Fast-path invariants

Governance must not make interaction feel governed.

- Policy is compiled into an in-memory capability before the first action.
- The authority check is synchronous, bounded, and contains no network,
  database, filesystem, or model call.
- The driver runs beside the world. A remote origin sends intent once; the
  destination daemon executes locally and streams observations back.
- Human keyboard and pointer input dispatch within one local presentation
  frame after a valid lease check.
- Human takeover requires no model participation and invalidates old work
  before dispatching the human event.
- Cached semantic observations avoid a fresh screenshot or full-tree walk.
- Action batches perform local guarded waits instead of model round trips.
- Large frames, trees, recordings, and logs are referenced, not copied through
  every event or turn context.
- Low-risk journal writes may group-commit. Irreversible effects require a
  durable admission barrier before execution.

Target budgets are measured at the destination daemon:

| Path | Initial target |
|---|---|
| Capability and lease admission | p95 below 1 ms |
| Cached semantic observation | p95 below 10 ms |
| Local human input dispatch | within one 60 Hz frame |
| Local control takeover | within one 60 Hz frame |
| Added remote control latency | network RTT plus encoding/dispatch |

These are architectural budgets, not claims about target application response
time or model inference.

## Permission dimensions

World permission is independent across:

- observe semantic state;
- observe pixels or recordings;
- low-risk interaction;
- local mutation;
- external submission or communication;
- irreversible effects;
- credential use and secret-field filling;
- clipboard read and write;
- upload, download, and filesystem transfer;
- console, DOM, style, performance, and network debugging;
- allowed application, window, resource, and origin scopes;
- shared-profile versus isolated-profile identity;
- agent routing and remote peer execution.

Peer execution policy remains the coarse destination admission boundary. A
world capability is the narrower, short-lived authority for one admitted
principal and world. Neither substitutes for the other.

## Shared and isolated experiences

### Shared world

- Uses a visible human-facing browser or desktop.
- Human identity and takeover are primary.
- The daemon groups and labels resources participating in the task.
- Sensitive effects may pause at the exact resolved action.
- Existing personal surfaces are `attached` unless the daemon can prove full
  mediation.

### Isolated world

- Is created and owned by the selected workshop daemon.
- May use an ephemeral or user-approved persistent profile.
- Runs without stealing the user's cursor or focus.
- Supports watch, pause, and privileged human takeover.
- Is the default for Bots, schedules, untrusted sites, and long-running work.

Moving between experiences means creating or attaching a different world and
transferring explicit intent or durable state. It never silently changes which
identity is acting.

## Seven implementation phases

Each phase is a reviewable merge unit with an observable exit test. A phase may
contain several atomic commits, but clients never receive a control that the
destination daemon does not enforce.

### Phase 1 — World authority kernel

**Outcome:** A reusable, provider-neutral kernel admits actions using world
revision, scoped capabilities, control generation, and idempotency without I/O
on the hot path.

**Proof status (2026-09-04):** Implemented locally in `medousa-world`. Thirteen
focused state-transition tests cover preemption, stale state, grants, resource
scope, id and idempotency collisions, late results, and bounded events. A
20,000-iteration local benchmark measured admission plus completion at
approximately 2.1 microseconds p50, 3.2 microseconds p95, and 6.1 microseconds
p99 on the development Mac. These numbers establish kernel overhead only; they
are not browser or network latency claims.

Implementation:

- Add a small `medousa-world` crate containing the locked domain records.
- Implement deterministic capability, lease, revision, admission, completion,
  failure, preemption, and bounded journal behavior.
- Make time and ids explicit inputs where determinism matters.
- Record late successful effects as requiring reconciliation rather than
  pretending they did not occur.
- Test human preemption, stale revisions, expired grants, resource scope,
  external-effect authority, idempotency, and journal bounds.
- Add a microbenchmark before declaring the latency budget met.

Acceptance:

- A human preempts an agent in one synchronous state transition.
- A stale or revoked principal cannot receive a mutation permit.
- A late driver result remains attributable and advances world state with a
  reconciliation marker.
- Replaying an acknowledged idempotency key does not issue a second permit.
- The kernel contains no async runtime, HTTP client, database, or model code.

Suggested commit boundary:

- feat(world): add the runtime authority kernel

### Phase 2 — Govern the existing browser action path

**Outcome:** The current desktop BrowserHost action crosses a daemon-owned
world admission and returns correlated world provenance.

**Proof status (2026-09-04):** Implemented locally for the existing desktop
BrowserHost path. The daemon resolves a concrete tab group and tab, admits the
turn through the world kernel, sends the exact group route a short-lived permit
plus the URL observed at admission, and records confirmed, failed, or
indeterminate completion. BrowserHost rejects a permit if its tab, page, or
expiry no longer matches. The prior unordered `current` lookup now uses an
explicit last-touched group identity.

This is a vertical proof, not the production cutover: authority and events are
still in memory, the loopback driver accepts legacy payloads during migration,
permit authenticity is not yet enforced across a process trust boundary, and
mobile remains on the client-executed path. Those constraints must not be
mistaken for completion of Phases 3–4.

Implementation:

- Give the daemon a world-authority service and bind one managed browser world
  to each concrete BrowserHost tab group.
- Resolve `current` deterministically during compatibility; stop sending
  actions to an unordered global map entry.
- Bind the acting turn principal, concrete tab resource, world revision, and
  control generation before calling the driver.
- Authenticate the daemon-to-driver permit on a dedicated governed route;
  compatibility payloads must not be able to impersonate daemon admission.
- Complete or fail the permit from the driver result and include world
  provenance in the tool result.
- Preserve the current BrowserHost control and URL checks as driver-side
  defense in depth.
- Keep mobile client-executed action on its current path until Phase 4 rather
  than pretending polling is a world transport.

Acceptance:

- Two tab groups cannot cause an action to reach an arbitrary `current` group.
- Every successful desktop browser action has an admitted and completed world
  event with the turn trace.
- A human control change during an action fences the old permit or marks a late
  success for reconciliation.
- Missing, forged, expired, or state-mismatched permits cannot reach the
  governed driver path.
- Existing browser UI and legacy action payloads remain compatible.

Suggested commit boundary:

- feat(browser): route desktop actions through world authority

### Phase 3 — Incremental browser observation and guarded batches

**Outcome:** Browser use is semantic-first, event-driven, and fast enough that
multiple ordinary actions do not require repeated full snapshots or model
round trips.

Current local slice:

- BrowserHost captures a bounded accessibility/DOM projection, keeps opaque
  refs stable only within one document, and emits full observations or deltas
  from a 32-revision journal. Page text and attributes remain explicitly
  untrusted data, sensitive values never cross the boundary, and document
  replacement invalidates old refs.
- The daemon admits one guarded batch of at most 16 prevalidated actions. The
  colocated host executes in order, budgets local waits to five seconds total,
  rechecks control/tab/URL/permit/observation fences before every step, stops on
  the first failure, and returns an automatic post-batch observation.
- Human control changes cancel pending action replies and fence the next step.
  Applied prefixes are reported as partial/indeterminate rather than falsely
  claiming rollback.
- Pixel observation is an explicit capability separate from semantic
  observation. On macOS, the colocated BrowserHost can capture the current
  WKWebView viewport only when it matches an admitted semantic document and
  revision. It scans known sensitive controls before and after capture,
  rejects moving/truncated redaction state, redacts at native resolution,
  bounds dimensions and bytes, then persists a content-addressed PNG outside
  the model transcript. The tool returns only a revisioned artifact receipt.
- Vision-capable model routes can hydrate that receipt transiently through a
  narrow runtime port. The daemon requires an exact session-bound artifact id,
  revalidates tool provenance, MIME, byte count, digest, and PNG dimensions,
  and marks the pixels as untrusted external content. Base64 never enters tool
  receipts or durable checkpoints, and each tool round replaces the prior
  pixel attachment so long turns stay memory-bounded.
- Windows/Linux/mobile screenshot drivers, client hydration of binary
  artifacts, pushed console/network deltas, hardened isolated-world DOM
  inspection, and the latency/pixel harness remain open before Phase 3 is
  considered complete. Mobile and extension action transports remain
  intentionally deferred to Phase 4.

Implementation:

- Replace serialized page HTML as the primary observation with a bounded
  accessibility/DOM projection and stable opaque references.
- Maintain a daemon state mirror from navigation, focus, DOM, accessibility,
  console, and network deltas.
- Add screenshot artifacts with viewport and coordinate metadata.
- Add guarded action batches, local waits, stop-on-first-failure, cancellation,
  and automatic post-action observation.
- Resolve risk from actual element semantics and effect boundaries.
- Add action-to-state and action-to-pixel performance harnesses.

Acceptance:

- A multi-step form interaction can execute locally from one admitted batch.
- Human input cancels the remaining batch before its next action.
- Most follow-up observations are bounded deltas.
- DOM replacement invalidates stale element references explicitly.
- Browser text cannot alter runtime policy.

Suggested commit boundary:

- feat(browser): add revisioned semantic observations
- feat(browser): execute cancellable guarded action batches
- feat(browser): hydrate governed screenshots for vision turns

### Phase 4 — Shared and isolated browser worlds

**Outcome:** A user or runtime may choose a shared real-session browser or an
isolated daemon-owned browser on an authorized workshop.

Current local slice:

- The world contract now distinguishes concrete driver identity, kind,
  transport, ownership, and mechanical capabilities from authority grants.
- Home desktop, Home mobile, and the browser extension advertise stable
  process-local browser drivers. Turn admission carries the selected driver
  through tab groups, world sessions, permits, client queues, and client-side
  browser-session completion.
- Exact driver routing prevents a newer client on the same surface from
  stealing a turn's tool request. Legacy clients remain surface-routed only
  when no driver id was supplied.
- The workshop daemon can now create an owned Chromium world with either an
  ephemeral profile or an explicitly named persistent profile. Its catalog,
  ownership, URL identity, control epoch, and lifecycle survive Home
  disconnects; daemon restart recovers active processes as stopped worlds that
  require an explicit resume.
- The isolated driver implements the same bounded semantic observation,
  opaque-ref action, guarded batch, redacted screenshot, and world-provenance
  contracts as the shared BrowserHost. It talks to a loopback-only random CDP
  endpoint in a named isolated execution world and never disables Chromium's
  sandbox.
- Authenticated profile-scoped HTTP controls expose create/list/get, navigate,
  observe, screenshot, pause/resume, human takeover/return, view
  attach/detach, stop, and cleanup. Detaching a view does not stop execution;
  persistent identity is never selected implicitly.
- The authority kernel revalidates every permit at the driver boundary,
  serializes mutating permits per resource, and permanently fences admitted
  work after takeover even when control is returned immediately. Expired
  pending permits are reaped with an audit event.
- Home now presents one Browser product surface with two explicit sources:
  **Workshop** is a daemon-owned world and **Device** is the existing native
  WebView with this device's cookies and passkeys. Source selection is scoped
  to the active workshop, an agent-attached world may surface automatically,
  and failure never silently moves identity between the two sources.
- Desktop and mobile can attach to a workshop world without owning its
  lifecycle. The view keeps one bounded redacted frame in memory, polls faster
  after interaction and slower while idle or hidden, and detaches without
  stopping the remote browser.
- Pointer, wheel, keyboard, and mobile software-keyboard input are mapped from
  the rendered frame into its CSS viewport. Each input names the exact
  document and observation revision shown to the human, is admitted as
  authenticated human intent, and takes control before CDP dispatch so queued
  agent work is fenced immediately.
- Approval-aware extension mutations, richer isolated multi-tab chrome, and
  destination-aware private/local URL routing remain follow-up hardening; none
  may become an implicit identity fallback.

Implementation:

- Turn the browser extension and mobile client queue into concrete,
  instance-addressed world drivers.
- Add approval-aware effects beyond the extension's current read-only ceiling.
- Add a daemon-owned isolated Chromium driver with explicit profile
  persistence and identity selection.
- Group agent resources separately from unrelated personal tabs.
- Support watch, pause, takeover, detach, resume, and cleanup.
- Route private and local-development URLs only to a workshop that can reach
  them; never silently move identity between drivers.

Acceptance:

- The same semantic browser contract drives embedded, extension, and isolated
  implementations.
- Concurrent sessions cannot clobber one shared active tab.
- Selecting a real profile is explicit, revocable, and visible.
- A remote isolated browser keeps executing when Home disconnects.

Suggested commit boundary:

- feat(browser): register concrete governed browser drivers
- feat(browser): add daemon-owned isolated browser worlds
- feat(browser): attach governed worlds to the Browser surface

### Phase 5 — Computer-use drivers

**Outcome:** The world contract drives native applications and desktops while
preserving platform security boundaries and background execution semantics.

Current foundation slice:

- The daemon composition owns one explicit world-authority service shared by
  browser and native-computer adapters.
- `medousa-computer-bridge` defines the platform-neutral, bounded preflight and
  semantic-observation protocol, including sensitive-value redaction and exact
  desktop-session identity.
- The native driver broker admits observation through that authority, fences
  it to one registered colocated driver, validates the result, and records a
  confirmed or failed effect.
- An in-memory fake driver proves the governed path. No platform sidecar,
  computer action, doctor surface, or Home control is claimed by this slice.

Implementation:

- Package computer-use drivers as optional sidecars resolved through the
  existing `{dataDir}/bin` installation model.
- Start with one platform proof, then add macOS accessibility/capture, Windows
  UI Automation/capture, and Linux accessibility/desktop portals.
- Prefer accessibility patterns and background dispatch; use foreground input
  and coordinates only when necessary.
- Add capability and permission preflight to `medousa doctor`.
- Model displays, windows, applications, menus, controls, dialogs, and focus as
  stable world resources.
- Fail closed on secure desktops, unavailable capture, permission loss, or
  coordinate uncertainty.

Acceptance:

- The user can watch and interrupt a daemon-owned desktop operation.
- A supported accessibility action does not move the user's physical cursor.
- Permission failure is diagnosed before a turn enters an action loop.
- Browser-only work continues to use the browser adapter.

Suggested commit boundary:

- feat(computer): add the first governed desktop driver
- feat(doctor): preflight computer-use capabilities

### Phase 6 — Federation, Bots, and durable operation

**Outcome:** Worlds run on an explicitly selected workshop and survive client,
agent, and network lifecycle changes.

Implementation:

- Extend authorized execution inventory with world-driver capabilities.
- Let the user select which workshop owns browser or computer execution.
- Let authorized workers and Bots request only opaque eligible world ids.
- Transport signed intent and capability material once; enforce and execute at
  the destination.
- Persist Bot-to-world bindings only when the user selects durable continuity.
- Stream bounded observations and artifacts without moving destination
  authority to Home.
- Resume, revoke, or reconcile after disconnect and daemon restart.

Acceptance:

- Mobile can operate or watch a browser/desktop owned by another workshop.
- A Bot may resume its approved isolated world without inheriting unrelated
  human identity.
- Revocation at the destination prevents the next action boundary.
- An offline origin does not orphan destination-owned work.

Suggested commit boundary:

- feat(world): federate authorized world intent
- feat(bots): bind durable worlds explicitly

### Phase 7 — Recovery, recipes, and production proof

**Outcome:** World operation is explainable, recoverable where possible, and
measurably more reliable than an opaque computer-use loop.

Implementation:

- Add domain-specific checkpoints and compensation plans.
- Keep a bounded raw telemetry ring and promote evidence around failure or
  sensitive effects.
- Add a unified human/agent causal timeline and artifact review surface.
- Derive replayable recipes from successful traces without carrying old
  authority into a new run.
- Build adversarial prompt-injection, stale-state, crash, disconnect,
  concurrent-control, and irreversible-effect evaluations.
- Publish performance and reliability budgets in CI without coupling release
  packaging to unnecessary duplicate compilation.

Acceptance:

- A crash between admission and completion resolves to a typed indeterminate
  or reconciled state.
- The user can answer who did what, under which authority, and why.
- A replayed recipe receives fresh admission and fails safely when the world
  differs.
- Common browser and computer workflows meet the latency budgets above.

Suggested commit boundary:

- feat(world): persist causal traces and recovery checkpoints
- test(world): add adversarial and latency evaluation suites

## Migration rules

- Existing BrowserHost endpoints remain compatible while Phase 2 adds world
  provenance. New fields are additive.
- `BrowserControl` remains a presentation compatibility field until all clients
  render daemon world leases directly.
- The current mobile browser-session polling path is not generalized; it is
  replaced by instance-addressed driver requests in Phase 4.
- Existing peer grants and execution policy are reused. World capabilities do
  not create a second pairing or authentication system.
- Existing browser snapshot tools remain available until the semantic
  observation contract reaches parity.
- No raw credential, cookie database, profile directory, or remote desktop
  token enters portable world state.
- A driver may report an externally observed mutation, but only the daemon may
  issue an action permit or capability grant.

## Observability requirements

Every durable world action records:

- world, authority, ownership class, driver, resource, and state revision;
- principal, turn/Bot/worker/peer correlation, intent, and trace ids;
- grant and control generation used for admission;
- requested semantic action and classified effect without secret values;
- admission, dispatch, driver, observation, and durability latency;
- terminal status and whether reconciliation or recovery is required;
- approval receipt, checkpoint, compensation, and artifact references where
  relevant.

Metrics distinguish runtime overhead from model, network, driver, and target
application latency.

## Verification strategy

Each phase adds tests at its ownership boundary:

- pure kernel state-transition and property tests;
- browser adapter contract tests and stale-control races;
- driver harness pages for DOM replacement, canvas, iframes, dialogs,
  downloads, authentication, and malicious page instructions;
- native platform permission and focus tests;
- remote workshop disconnect, restart, revocation, and latency tests;
- end-to-end human takeover and late-effect reconciliation tests.

The normal repository CI remains required. Platform-only computer-use tests
run in dedicated jobs with explicit capability preflight rather than making
every release job compile and launch every driver.

## Non-goals

- Giving a model unrestricted shell, CDP, accessibility, or input injection.
- Claiming full mediation of a pre-existing personal desktop.
- Streaming every raw frame or input event into the model context or durable
  database.
- Inventing a second auth, pairing, scheduler, Bot, or turn runtime.
- Treating screenshots as the only source of state.
- Silently copying a user's real browser profile to another workshop.
- Pretending all external effects can be rolled back.
- Making Home the authority because it renders the world.

## Initial code anchors

- `crates/medousa-browser-bridge/src/model.rs`
- `crates/medousa-browser-bridge/src/manager.rs`
- `apps/medousa-home/src-tauri/src/browser_host.rs`
- `apps/medousa-home/src-tauri/src/human_browser.rs`
- `apps/medousa-home/src/lib/browserBridge.ts`
- `src/browser_act_tools.rs`
- `src/browser_host_client.rs`
- `src/client_tools.rs`
- `src/peer_execution_policy.rs`
- `src/agent_runtime/execution_context.rs`
- `src/work_environment_federation.rs`

## Epic exit criteria

The epic is complete when:

- the destination daemon is the sole authority that grants and admits browser
  and desktop mutations;
- humans, agents, Bots, workers, and peers appear in one causal world timeline;
- human takeover is immediate and reliably fences old work;
- shared and isolated worlds use one semantic contract without sharing unsafe
  defaults;
- browser and desktop drivers are replaceable capabilities, not policy owners;
- world placement and permission compose with existing remote execution;
- late, failed, and irreversible effects have truthful recovery states;
- common workflows meet measured interaction latency budgets; and
- losing a client, model loop, driver, daemon, or network link does not erase
  acknowledged world state or authority provenance.
