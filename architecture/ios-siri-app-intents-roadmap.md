# Siri, App Intents, and Apple Intelligence roadmap

> **Status:** Locked implementation roadmap
>
> **Date:** 2026-09-14
>
> **Target:** iOS 27 first; retain useful App Intents behavior on earlier supported
> iOS releases where the framework permits it
>
> **Product surface:** Medousa iOS, backed by the selected workshop daemon

## Outcome

Make Medousa a first-class participant in Siri, Shortcuts, Spotlight, the Action
button, Apple Intelligence, and Visual Intelligence without creating a second
agent runtime inside the iOS shell.

The first shippable experience is **Ask Medousa**: a user invokes Medousa from
Siri or Shortcuts, supplies a prompt, selects or accepts a workshop, and starts a
normal durable Medousa turn. Later slices teach the system about Medousa notes,
work, sessions, calendar items, and visible content so Siri can resolve natural
references and compose actions across apps.

## Locked product rules

1. **The selected workshop remains authoritative.** App Intents use the same
   active-workshop resolution and transport as Medousa Home. Native Swift never
   reads or mutates a remote workshop's filesystem directly.
2. **One turn engine.** Asking Medousa through Siri starts the existing durable
   interactive or background turn contract. There is no Siri-only agent loop.
3. **Intent execution is typed.** Concrete operations use App Schemas where an
   Apple schema matches. A generic prompt is exposed separately as Ask Medousa;
   it is not disguised as a calendar, message, or document operation.
4. **Privacy is opt-in by content class.** No vault body, transcript, artifact,
   or work evidence enters Apple's semantic index until the user enables that
   class. Titles and display metadata follow the same setting.
5. **Locked-device responses are minimal.** Sensitive results do not appear in
   Siri dialogue, snippets, notifications, or Spotlight while locked. The intent
   may ask the user to unlock and continue in Medousa.
6. **Consequential actions require confirmation.** External sends, destructive
   changes, work cancellation, and mutation of shared/public entities require
   explicit confirmation and, where supported, entity ownership metadata.
7. **App Intents are a native adapter.** Swift maps Apple parameters and results
   to a small Medousa gateway. Business logic stays in daemon operations.
8. **Generated Xcode state is reproducible.** All target membership, deployment
   settings, frameworks, entitlements, and metadata configuration are produced
   by `npm run ios:prepare`; hand-edited generated project state is not source.
9. **Graceful unavailability is a product state.** An unavailable, unpaired, or
   suspended workshop returns a clear result or opens Medousa. It never silently
   routes the request to a different workshop.
10. **No global Siri replacement claim.** Japan-only side-button access remains
    a regional enhancement and is not the core launch story.

## User experiences

### Tier 1 — actions available broadly

- “Ask Medousa to plan my afternoon.”
- “Search Medousa for the launch checklist.”
- “Open the launch checklist in Medousa.”
- Run Ask Medousa from Shortcuts or the Action button.
- Continue a completed or long-running request in the correct Medousa session.

### Tier 2 — Siri and Apple Intelligence context

- “What is due in Medousa today?”
- “Open my third active undertaking.”
- “Summarize this note.”
- “Create work from this conversation.”
- “Send this artifact,” with a system handoff and explicit confirmation.

### Tier 3 — visual discovery

- Find vault notes or artifacts related to a photographed or onscreen object.
- Continue a Visual Intelligence search inside Medousa.
- Open a matching entity directly in the correct workshop and surface.

## Native architecture

```text
Siri / Shortcuts / Spotlight / Action button / Visual Intelligence
                              |
                     Swift App Intents
                              |
                MedousaIntentGateway (native)
                       |              |
             App Group snapshot   foreground route
                       |              |
                Rust/Tauri commands + medousa:// deep links
                              |
                 selected WorkshopTransport
                              |
                    medousa_daemon APIs
```

### Source layout

Add a source-owned native tree separate from generated Xcode files:

```text
apps/medousa-home/src-tauri/ios-app-intents/
├── App/
│   ├── MedousaIntentGateway.swift
│   ├── MedousaIntentDependencies.swift
│   └── MedousaOnscreenContextBridge.swift
├── Entities/
│   ├── WorkshopEntity.swift
│   ├── VaultNoteEntity.swift
│   ├── WorkItemEntity.swift
│   ├── SessionEntity.swift
│   └── CalendarItemEntity.swift
├── Intents/
│   ├── AskMedousaIntent.swift
│   ├── SearchMedousaIntent.swift
│   ├── OpenVaultNoteIntent.swift
│   └── OpenWorkItemIntent.swift
├── Indexing/
│   └── MedousaEntityIndex.swift
└── VisualIntelligence/
    └── MedousaVisualSearchQuery.swift
```

App Intent types must be compiled as discoverable members of the application or
an appropriate extension so Xcode's App Intents metadata processor sees them.
Do not hide the intent declarations solely inside the static Swift archive built
by `src-tauri/build.rs`. Reuse that archive only for low-level C ABI bridging
where appropriate.

`scripts/ios-prepare.sh` owns the XcodeGen source entry, framework links,
deployment guards, build settings, and optional regional entitlement injection.

### Gateway contract

The native surface stays deliberately small:

```swift
protocol MedousaIntentGateway {
    func availability() async -> MedousaIntentAvailability
    func listWorkshops() async throws -> [WorkshopSummary]
    func ask(_ request: AskRequest) async throws -> AskReceipt
    func search(_ request: SearchRequest) async throws -> [EntitySummary]
    func resolve(_ reference: EntityReference) async throws -> EntitySummary?
    func open(_ reference: EntityReference) async throws
}
```

The corresponding Rust adapter resolves the selected workshop through existing
workshop routing and calls generated daemon operations. Avoid URLSession calls
to loopback and avoid duplicating credentials in Swift.

Because background intents cannot depend on a running webview, native execution
must use one of two explicit paths:

- a background-safe Rust/native bridge when the embedded app process can service
  the request; or
- a foreground continuation into Medousa when iOS lifecycle or transport rules
  prevent reliable background execution.

App Group state may cache nonsensitive display/index metadata and pending
foreground routes. It is not an authority for turns or entities.

## Entity and schema model

| Medousa concept | App Entity | Initial exposure | Notes |
|---|---|---|---|
| Workshop | `WorkshopEntity` | Picker only | Never merge entity namespaces across authorities |
| Vault note | `VaultNoteEntity` | Search/open | Stable ID includes workshop authority + note identity |
| Undertaking/work card | `WorkItemEntity` | Search/open/status | Mutations require current allowed actions |
| Chat session | `SessionEntity` | Open/continue | Transcript indexing is separately opt-in |
| Calendar event/reminder | `CalendarItemEntity` | Query/create/update | Use calendar schemas only where semantics match |
| Artifact | later `ArtifactEntity` | Search/open/transfer | Export only safe, materialized representations |

Entity identifiers must remain stable across launches and must include workshop
authority so two workshops cannot collide. Adopt `SyncableEntity` only after the
underlying Medousa identity is demonstrably stable across devices.

Use Apple's System search-in-app schema for broad Medousa search. Adopt other
schema domains incrementally and completely: if Xcode identifies a required
related schema, implement the coherent set rather than publishing a partial
contract.

## Delivery phases

### S0 — capability spike and build proof

**Goal:** Prove metadata discovery and lifecycle behavior before product wiring.

- Add one no-op development intent directly to the iOS app target.
- Teach `ios:prepare` to reproduce the source membership and framework setup.
- Verify intent metadata in a device archive, not only the simulator.
- Record the minimum OS/compiler guards for iOS 27 APIs.
- Prove invocation when the app is foregrounded, backgrounded, suspended, and
  terminated.

**Exit:** the intent appears in Shortcuts and invokes reproducibly from an
archive produced by the normal build script.

**Evidence (2026-09-14):**

- `MEDOUSA_LIVE_ACTIVITY=1 npm run tauri -- ios build --debug --target
  aarch64-sim --no-sign --ci` completed through the normal Tauri/Xcode path.
- The resulting `Medousa.app` contains
  `Metadata.appintents/extract.actionsdata`; the generated metadata declares
  `MedousaIntegrationProbeIntent` as discoverable and includes both committed
  App Shortcut phrases.
- The exact bundle installed and launched on an iPhone 17 simulator as
  `com.entasislabs.medousa-home`.
- `npm run ios:verify-app-intents -- /path/to/Medousa.app` provides a repeatable
  metadata assertion for simulator bundles and signed device archives.
- Still required for S0 exit: run the verifier against a signed device archive,
  confirm Shortcuts presentation, and record foreground/background/suspended/
  terminated invocation results. iOS 27-only guards also remain pending an
  installed iOS 27 SDK; the current probe is guarded at iOS 16.

### S1 — Ask Medousa MVP

**Goal:** Ship the highest-value action without waiting on semantic indexing.

- `AskMedousaIntent` with prompt and optional workshop parameters.
- App Shortcut phrases, Shortcuts presentation, Spotlight discovery, and Action
  button compatibility.
- Start a normal durable turn with `home-ios-siri` surface attribution.
- Return a bounded summary when safe; otherwise return a receipt and deep-link
  into the session.
- Explicit busy, offline, unavailable, authentication, and unlock-required
  states.
- Cancellation propagates to the intent wait, not automatically to durable work.

**Exit:** a TestFlight build can start and resume a real Medousa turn from Siri
and Shortcuts without the webview already running.

**Progress (2026-09-14):**

- Added an iOS 18-guarded `AskMedousaIntent` with required free-text input and
  the supported “Ask Medousa” / “Talk to Medousa” invocation phrases. Siri asks
  for the request through the parameter dialog; Apple does not permit a raw
  `String` parameter inside an App Shortcut phrase.
- The native intent writes a bounded request into the shared App Group and opens
  `medousa://ask?request=…` with an opaque UUID receipt. Rust atomically consumes
  the matching receipt within ten minutes, so an unrelated app cannot forge an
  auto-run URL or recover the prompt from the URL.
- After receipt validation, the trusted shell admits a normal interactive turn
  through the existing selected-workshop path with `home-ios-siri` attribution,
  registers it in the current chat, and attaches the standard durable stream.
- Siri gives a bounded “Starting your request” acknowledgment. The foreground
  gateway distinguishes busy, expired, authentication, offline, unavailable-
  workshop, and unknown failures; if admission fails after receipt consumption,
  it preserves the prompt in the composer for a safe manual retry.
- Canonical mobile plist ownership now guarantees `medousa://` registration in
  intermediate Xcode archives as well as Tauri's final bundle.
- The normal simulator build and metadata verifier pass with both intents; the
  final iOS executable exports the Swift receipt-consumer C ABI used by Rust.
  Remaining S1 work is direct background execution, an optional workshop
  picker, bounded Siri result dialogue, explicit error states, and cancellation
  behavior.

### S2 — search, entities, and deep links

**Goal:** Let the system find and open Medousa content.

- Implement workshop, note, work-item, and session entities and queries.
- Add an authority-scoped daemon search operation if existing operations cannot
  provide one bounded, typed query.
- Implement System search-in-app, entity open intents, and missing deep links.
- Add user settings per indexable content class plus “Remove Medousa content
  from system search.”
- Maintain the semantic index incrementally from daemon change events; reconcile
  on foreground and settings changes.

**Exit:** Spotlight and Siri resolve duplicate names correctly across workshops,
revoked content disappears, and taps land on the exact entity.

### S3 — onscreen awareness

**Goal:** Make “this,” “that,” and ordinal references useful in Medousa.

- Frontend publishes a bounded current-context envelope on navigation/selection.
- Native bridge sets `NSUserActivity` for a single primary entity.
- Add native view/entity annotations where native surfaces exist.
- For webview lists, expose only selected/focused entities initially; do not
  attempt fragile DOM-coordinate mirroring in the first release.
- Clear native context on workshop switches, lock, logout, and sensitive panes.

**Exit:** Siri can resolve the selected note or work item without leaking stale
context from another workshop.

### S4 — typed actions and cross-app transfer

**Goal:** Support reliable operations beyond a generic agent prompt.

- Adopt applicable calendar, document/system, and other App Schema domains.
- Add create/update/query intents backed by typed daemon operations.
- Donate relevant successful interactions without donating prompt contents.
- Add `Transferable`/`IntentValueRepresentation` for explicitly exportable
  artifacts and entities.
- Add confirmations, ownership checks, and shared-workshop policy enforcement.

**Exit:** schema actions pass isolated App Intents tests and end-to-end Siri tests
for permission, confirmation, cancellation, and cross-app handoff.

### S5 — Visual Intelligence

**Goal:** Return relevant Medousa content for visual searches.

- Receive `SemanticContentDescriptor` in one intent value query.
- Prefer labels/OCR and existing semantic search before adding an image embedding
  store; add image similarity only with a measured relevance win.
- Return a small, ranked result set with safe thumbnails.
- Implement continue-search-in-Medousa and exact-result open paths.
- Never persist captured pixels unless the user explicitly continues into a
  Medousa turn that attaches the image.

**Exit:** camera and screenshot searches return fast, relevant results and no
captured image data remains after a query-only interaction.

### S6 — conversational launch and regional enhancements

**Goal:** Launch directly into Medousa voice where Apple permits it.

- Implement the Assistant activate schema and immediate audio-session startup.
- Request and conditionally inject the Side Button Access entitlement.
- Gate UI and documentation to eligible devices/accounts/regions.
- Keep the ordinary Action button/App Shortcut path available globally.

**Exit:** eligible Japan-region devices launch directly into a ready conversation
and ineligible builds contain no misleading settings.

## Security and privacy acceptance matrix

| Situation | Required behavior |
|---|---|
| Device locked | No private body text or transcript in dialogue/snippets |
| Workshop unavailable | Report the named workshop unavailable; never reroute |
| Active workshop changes | In-flight request remains bound to captured authority |
| Entity deleted or access revoked | Query returns no entity; remove indexed record |
| Shared/public mutation | Confirm and enforce daemon authorization |
| Generic prompt requests external side effect | Normal Medousa tool approval still applies |
| Visual search query | Process ephemerally; do not retain pixels by default |
| User disables indexing | Delete donated/indexed Medousa entities and reconcile |

## Verification

Each phase adds tests at the narrowest boundary and then exercises Apple system
surfaces in order:

1. Swift unit tests for entities, queries, gateway mapping, and redaction.
2. Rust tests for workshop capture, authority binding, and daemon error mapping.
3. App Intents Testing for parameters, results, confirmations, and cancellation.
4. Shortcuts for intent shape and background lifecycle.
5. Spotlight for indexing, revocation, duplicate names, and deep links.
6. Siri for natural language, onscreen references, and locked-device behavior.
7. TestFlight/device archive validation on the oldest supported OS and current
   iOS 27 hardware, including cold launch and network loss.

The existing Home checks and strict docs verification remain required. Native
intent metadata inspection becomes part of the iOS release checklist after S0.

## Metrics and rollout

Collect privacy-preserving counters only:

- invocation surface and intent kind;
- success, foreground-continuation, cancellation, and typed failure class;
- time to receipt and time to first completed result;
- entity query latency and zero-result rate;
- deep-link landing success.

Do not log prompts, Siri transcripts, entity titles, captured pixels, result
bodies, or semantic index contents as analytics.

Roll out behind a build/runtime feature flag through internal builds, TestFlight,
and then general availability. Indexing remains an explicit user setting even
after the overall feature graduates.

## Explicit non-goals

- Replacing Siri globally or claiming default-assistant status.
- Running a second LLM/agent implementation inside an App Intent.
- Uploading a vault to Apple or treating pairing as content synchronization.
- Indexing all content automatically because an API is available.
- Keeping an intent alive indefinitely to stream full agent output through Siri.
- Mirroring arbitrary webview DOM into native view annotations in the MVP.
- Adopting a schema whose semantics Medousa does not actually satisfy.

## Decisions still requiring measured proof

These do not block S0, but must be answered before their dependent phase:

- Maximum reliable background execution window for a durable turn receipt.
- Whether App Intent execution can use the embedded daemon bridge while the app
  is terminated on every supported iOS configuration.
- Which entity bodies are eligible for Apple's semantic index and what product
  copy produces informed consent.
- Whether image similarity materially outperforms labels/OCR plus existing
  Medousa semantic retrieval.
- Which iOS 27 schemas are available in the shipping SDK and applicable to the
  final Medousa domain model.

## Apple references

- [Apple Intelligence for developers](https://developer.apple.com/apple-intelligence/)
- [App Intents](https://developer.apple.com/documentation/appintents)
- [Build intelligent Siri experiences with App Schemas — WWDC26](https://developer.apple.com/videos/play/wwdc2026/240/)
- [Explore advanced App Intents features — WWDC26](https://developer.apple.com/videos/play/wwdc2026/343/)
- [Providing contextual cues to Apple Intelligence and Siri](https://developer.apple.com/documentation/appintents/providing-contextual-cues-to-apple-intelligence-and-siri)
- [Integrating your app with Visual Intelligence](https://developer.apple.com/documentation/visualintelligence/integrating-your-app-with-visual-intelligence)
- [Launching a conversational app from the iPhone side button](https://developer.apple.com/documentation/appintents/launching-your-voice-based-conversational-app-from-the-side-button-of-iphone)
