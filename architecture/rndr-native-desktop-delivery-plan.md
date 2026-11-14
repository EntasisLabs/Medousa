# Native Medousa desktop: delivery plan

Status: implementation plan, not a completion claim.
Date: 2026-11-14. Inspected Medousa `3f513009abcd196e3c03cd39ba7780615b0b0ad2`
and the RNDR dependency checkout at `44f5dc7`.

## Product decision

Build the native replacement in `apps/rndr-home`. Do not start a third shell.
The first deliverable is an application the principal can use for everyday chat
instead of Tauri. Finish chat before expanding unrelated destinations. Then
migrate the rest of the desktop by complete workflows, not read-only previews.
Tauri remains available during migration; its removal is the final cutover,
not a prerequisite for making the native application useful.

The product is **Medousa**. No preview branding, permanent safety banners,
placeholder buttons or destination count as progress. This does not mean hiding
broken chat features: required chat capabilities must be implemented. Out-of-scope
non-chat destinations stay out of the primary navigation until usable.

Native means RNDR rendering/input/lifecycle, no Tauri or embedded webview as the
application shell. Daemon execution, identity, authorization and filesystem
ownership remain unchanged. Optional browser-based content is a separate product
contract, not permission to silently embed a browser to claim native parity.

## What the code actually says

| Finding | Source | Consequence |
|---|---|---|
| `native_input_ready` is unconditionally false outside tests; tests can install `SyntheticEditor`. Controller blocks Send, Restore, New, Recent and Reload. | `apps/rndr-home/src/composer.rs`, `controller.rs` | Current tests can pass with production chat deliberately unusable. Resolve input ownership and replace this permanent gate with actual readiness/state checks. Do not just return true. Session navigation must not depend on composer reset readiness. |
| Shared local bootstrap/paths/process policy already exists. RNDR still rejects remote/paired active workshops. Startup waits for readiness before creating the app. | `crates/medousa-desktop-host`, `apps/rndr-home/src/main.rs`, `startup.rs`, README | Keep shared host policy. Move connection work behind a visible, responsive startup/retry state; add remote selection through the real workshop/auth contracts, not local-token fallback. |
| ChatCore is already integrated and owns history/live reconciliation and causal identities. | `crates/medousa-app-core/src/lib.rs`, `contract.rs`, `apps/rndr-home/src/lib.rs` | Preserve it. Do not write another transcript reducer. |
| Native host projects rich chat items to `(id, role, body)` rows; worker/proposal entries become literal labels and proposal facts passed to placement are empty. | `apps/rndr-home/src/lib.rs::refresh_rows` | Rendering text alone is not chat parity. Consume typed items/segments, tools, media, artifacts, relationships and actionable coordination. |
| SDK transport, generation-filtered replies, active-turn/history bracketing, v3 stream following and terminal reconciliation already exist. | `apps/rndr-home/src/backend.rs`, `controller.rs` | Reuse and complete this path. Preserve no-duplicate-admission and stale-reply protections. Scope ownership per session/workshop rather than one global loading state. |
| Composer exposes unported attachments/stickies; runtime menu lacks external conversation flows; behavior only carries local response depth. | `composer.rs`, `menus.rs`, `state.rs`, README | These are implementation work, not labels to polish away. |
| The shell is a Today button/Recent shortcut rather than the actual session browser; a custom variable-height conversation realization path already exists. | `shell.rs`, `conversation.rs` | Build session navigation; evaluate RNDR variable-height collections against transcript needs before maintaining a second virtualization engine. |
| RNDR provides native Prompt, clipboard/IME integration, semantic controls, menus, theme/composition, collections including variable-height lists, editing and app helpers. | RNDR `rndr_core/src/prompt.rs`, `rndr_ui/src/{composition,collections,menu,lifecycle,controls,editing}.rs`, `rndr_app/src/lib.rs` | Adopt available contracts. RNDR docs also leave native/platform acceptance open; API availability is not proof of correct integration. |
| RNDR dependencies are absolute developer paths and the app has its own workspace/lockfile. | `apps/rndr-home/Cargo.toml` | Keep isolated builds if useful, but pin reproducible dependencies and add the real app to CI/package workflows before distribution. |

These are source observations, not a fresh GUI or live-provider acceptance pass.
Historical reports/test counts under the preview are not current product signoff.

## Architecture to keep

```text
Daemon / medousa-types: durable truth, capabilities, execution, filesystem authority
           ↕ existing SDK and authenticated transports
Native application services: connection/workshop scope, sessions, drafts,
request routing, subscription ownership, persistence and platform operations
           ↕ commands/facts/snapshots
medousa-app-core: canonical renderer-independent application semantics
           ↕ typed view projection and user intents
RNDR: native window, input, layout, components, accessibility and paint
```

- `medousa-app-core` is not an HTTP client or all of the application backend.
  It currently provides chat, shell, vault-buffer and undertaking-binding cores.
  SDK performs transport; desktop-host owns reusable host/bootstrap policy.
- Put genuinely shared state rules in app-core, not RNDR entities or Svelte
  components. Keep networking, credentials and platform effects outside it.
  Where Tauri-only TypeScript owns a required rule, extract a focused shared
  contract and keep both hosts consistent; do not port the entire store blindly.
- Keep RNDR entities as view identities, never daemon/session identity. Drafts,
  selections and async requests are scoped by workshop + session. Retained native
  editor state must survive theme changes, resize and ordinary snapshot updates.
- Use RNDR semantic controls and lifecycle ownership. Do not build another input,
  popup, focus, list or async framework in application helpers. Read the actual
  API constraints: variable-height rows recycle their descendants; persistent
  composer/editor state cannot live inside recycled rows.
- Preserve working authorization and uncertain-outcome handling. Ordinary
  authorized actions should work without warning rituals. Errors belong next to
  the failed action with a useful recovery path, not across the entire app.

## Chat completion contract

This is the acceptance backlog, not an optional feature menu. For each row record
implementation location, regression coverage, native result and remaining defect
in this document as work progresses. No separate evidence bureaucracy is needed.

| Workflow | Existing desktop reference | Required native outcome |
|---|---|---|
| Startup and connection | shared desktop-host; Tauri connection/workshop stores and settings | Launch normally, visible loading/error/retry, use selected workshop and correct auth, switch supported local/remote workshops without leaking state or credentials; no CLI required for normal use. |
| Session navigation | `SessionSidebar.svelte`, `SessionRow.svelte`, session controller/search | Create, list/page/search, open, rename/delete with actual confirmation, restore last session; preserve drafts and correct focus across navigation. Implement supported session grouping/relationships, not a Recent-only substitute. |
| Desktop chat workspace | `ChatSessionView.svelte`, appShell, shell tab store | Chat tabs/panes and focused composer use ShellCore identities; background panes do not steal focus; closing/reopening does not lose drafts or terminate daemon work. |
| Compose and send | `ChatComposerBar.svelte`, composer model, attachments/plus/draft controls | Multiline native editing, IME, undo/redo, selection, paste, Enter/Shift+Enter, click Send, draft restore/persistence, attachment upload/drop/image paste, resource references and skills. Correct request options and payload. No accidental or duplicate submission. |
| Model/runtime/behavior | model/runtime/mode pickers, runtime control row, ReasoningOptions, narration and defaults stores | Real catalog/capability states, persistent/default/session semantics matching Tauri; runtime selection opens the correct Medousa/external conversation path, not the wrong endpoint with a changed label. Include response depth, reasoning/style/narration where supported. |
| Transcript | `ChatMessageList.svelte`, `VirtualChatBeat.svelte`, appChat adapters, ToolRunChips/ToolActivitySheet | Ordered live/history segments, readable native Markdown/code, selection/copy, links, tool details/results/errors, media and artifact references. No loss of rich item data in a text tuple. Long history scroll anchoring and follow-latest work. |
| Turn lifecycle | chat store/controllers, ChatCore, stream/history adapters | One admission per intent; stream, stop, reconnect, replay, restart/reattach, terminal history reconciliation and uncertain-outcome recovery. Switching chats does not cancel remote execution or mix updates. |
| Coordination and context | proposal/permission/budget/mode bars, agent/task/worker groups, work dock, sticky board, context indicator | Render actual grouped activity and actionable decisions; approval/denial goes through existing authority APIs and refreshes real state. Expose tools, worker/proposal details, work context, asks and stickies where the desktop chat provides them. No fake worker labels or inert approval cards. |
| External conversations | runtime picker, external conversation controller/transcript, agent session controls | List/select/resume the actual external conversation, stream its actual events, handle permissions and errors, preserve provider/runtime identity across relaunch. |
| Rich chat content | LiquidChatMessage, media parts, artifact presentation/export | Map typed supported content to native components; provide real open/export actions for foreign content. Do not claim arbitrary HTML/JS executes natively. See content decision below. |
| Desktop ergonomics | existing desktop layout/theme/keyboard behaviors | Coherent responsive layout, persistent appearance, usable focus/shortcuts, native clipboard/file dialogs, accessible names/states, close/reopen and sleep/wake behavior. No preview chrome or development diagnostics in normal layout. |

Content decision: RNDR native Markdown and typed UI can cover substantial chat
content, but arbitrary stored HTML/JS artifacts are not automatically RNDR views.
Port typed Liquid presentations to native components and define an explicit
external-browser/open-export route for legacy HTML where faithful native rendering
is impossible. Preserve sandbox/authority boundaries and tell the user what opens
externally. If exact embedded HTML interaction is required, that conflicts with a
strict no-webview target and must be resolved explicitly, not hidden by a parity
claim. Browser/draw surfaces later have the same dependency question.

## Execution order: six coherent implementation batches

One lead owns architecture, integration and the runnable product. Parallel workers
may own isolated controls/transcript services with explicit contracts; they must
not simultaneously rewrite ChatCore, global state or the controller. Review each
integrated batch in the application. A failed acceptance case is fixed in that
batch, not renamed into a future preview milestone.

### 1. Working native chat foundation

Work in startup/main, state, backend/controller, composer and shell. Keep current
shared host and ChatCore. Create a responsive connection surface and real session
list/open/new flow. Implement per-session draft and request/subscription ownership.
Resolve Prompt submission/reset against the installed RNDR code. Current Prompt
emits Submitted after folding its key batch; explicitly test Enter followed by
other input, IME commit, queued clicks, and reset/undo state. Replace the permanent
false gate only when the real editor path works; remove synthetic test-only
permission as proof. Do not couple New/Open/Reload to editor-reset readiness.

If this needs a framework fix, produce the exact reproduction and RNDR change
contract, coordinate the authorized RNDR fix, then consume it. A cross-repository
prerequisite is real work, not an excuse for an indefinite disabled application.

Exit: launch normally, find/open an existing chat, create a chat, type/send using
both Enter and mouse, stream/stop/reload, switch away/back, close/relaunch without
losing draft/history or duplicating a turn. Tests exercise the production path.
This is the first integrated build, not the final chat parity claim.

### 2. Complete composition and controls

Finish the composer parity rows: attachments/upload/paste/drop/resource references,
skills, draft persistence, model/runtime/mode and behavior controls. Use real
catalogs and settings semantics. Add the minimum usable Connection/provider setup
needed for chat now; do not defer essential setup behind a future Settings phase.
Implement workshop switching and its stream/cache invalidation with the existing
locality/auth policies. Add microphone/transcription integration from the reference
when supported; drawing attachment creation depends on the native drawing surface
and must remain an explicit outstanding parity item, not an inert button.

Exit: the same prompt/options/attachments reach the same backend contract as
Tauri. Catalog/setup failures preserve drafts and offer usable recovery. External
runtime choices are only exposed when their actual conversation path works.

### 3. Full transcript and turn recovery

Replace the lossy row projection with typed item/segment view models. Reuse
ChatCore ordering/relationships and RNDR Markdown/host slots/collections as
appropriate. Implement tools, media, links/code copy, typed rich UI and artifact
open/export. Choose the transcript realization approach based on stable scroll
anchors, selection, variable heights and performance; do not mechanically replace
working code with a uniform-height list. Complete reconnect, reattach, terminal
reconciliation and stale-workshop/session rejection.

Exit: representative real chat histories, including tools, media, workers and
interrupted turns, retain content/order and remain usable while streaming and
scrolling. Reload/relaunch never resubmits an uncertain turn.

### 4. Complete chat workspace and coordination

Wire ShellCore tabs/panes, focus, last-open state; action cards and worker/task
panels; context/stickies/asks; external conversation selection and controls. Complete
any remaining chat parity rows from the contract, including essential context
openers. A deep link to an unported full work/review surface must provide the
necessary chat decision/detail locally or remain a named blocker to that workflow.
Do not mark chat complete while required approvals still force Tauri use.

Exit: ordinary chat, tool use, delegated work and external conversations can be
conducted in RNDR without going back to Tauri for chat controls or approvals.

### 5. Daily-use cutover build

Remove superseded preview fixtures/routes from normal navigation, not necessarily
from isolated developer test tools. Finish visual/keyboard/accessibility polish
and lifecycle defects. Pin RNDR reproducibly instead of absolute user paths; add
app-specific build/test/lint CI and native packaging using existing Medousa
identity/assets and bundled daemon/install policy. Deliver a runnable macOS app
bundle, not a Cargo instruction. Keep existing Tauri distribution until replacement
release acceptance is complete. Add a short user-facing launch/setup guide then.

Exit: the principal completes the acceptance script below in the packaged native
build. This is **chat daily-use ready**, not whole-desktop migration complete or
cross-platform release certification.

### 6. Remaining desktop, then full replacement

After chat is daily-use ready, migrate complete groups in this dependency order:

1. Settings/Connection/Packages and workshop/profile management beyond chat needs;
   retain home-first install and shared host policies.
2. Files/Library/Notes: daemon-backed browse/search/open/edit/save/conflict handling,
   using VaultBufferCore. Filesystem operations follow workshop locality.
3. Code/work/reviews/terminal: UndertakingBindingCore, real project bindings,
   diffs, review decisions and execution controls; native terminal/platform support
   is a dependency to resolve, not a static transcript substitute.
4. Other content/product surfaces (calendar, peers/messaging, automations/runtime,
   artifacts/create/draw/map/browser) in dependency-aligned complete workflows.
   Audit actual Tauri route and canvas/custom-surface registration; this list is
   ordering guidance, not a claim that every route was exhaustively inspected.
5. Full route/workflow inventory closed, supported-platform native validation,
   release packaging/signing/updater/install acceptance, then switch the default
   desktop entry point and remove redundant Tauri-only application logic.

Each group gets its own small behavior matrix based on its existing implementation
when work begins. Do not create 18 empty destinations again. New surfaces must
reuse the native shell/navigation/service conventions established by chat.

## Acceptance: what must actually be demonstrated

Use a dedicated test workshop/session for mutations and authorized live/provider
calls; replay fixtures support diagnosis but do not substitute for a live pass.
Do not restart or alter the principal's daemon just to test the plan.

1. Launch packaged app with normal configuration, connect, browse/search/open an
   old session; handle missing daemon/auth/provider with a useful visible state.
2. New chat → choose model/options → send multiline text with Enter and with
   Send. Check IME, paste/undo and rapid queued input. Verify exact admitted text
   and exactly one admission, not merely a changed label.
3. Attach/paste/drop a file/image, add a reference/skill and verify the real
   request and rendered result. Check upload failure/removal and draft recovery.
4. Stream a tool-using turn; read/copy prose/code, inspect tool output, open media
   and an artifact. Perform a real supported approval/denial and inspect a worker.
5. Stop; interrupt transport; switch chats/workshops during delayed replies;
   reopen during an active turn; reconcile an unknown admission. No duplicate
   send, mixed workshop content, stale focus takeover or lost draft.
6. Open two chat tabs/panes, switch focused composer, close/reopen, relaunch;
   verify session, draft, settings, view and scroll ownership.
7. Use an external-runtime conversation end to end. Confirm actual provider and
   conversation continuity, permission handling and error recovery.
8. Inspect narrow/normal/wide layouts, light/dark/high-contrast and text scaling;
   test keyboard traversal, clipboard and representative accessibility actions.
   Exercise resize, minimize/restore and sleep/wake in the native window.
9. Run the real app against a substantial history and a sustained stream; record
   input responsiveness, scroll stability, CPU/RSS and any visible jank. Set
   numerical budgets from a measured baseline; do not invent passing budgets.
10. Principal can use this build for daily chat; all required rows above are
    demonstrated or explicitly rejected as an agreed product change. No hidden
    fallback to Tauri and no assertion that unit tests equal native acceptance.

For each batch: relevant app-core/host tests, RNDR application tests on the real
path, format/lint/build, diff review and native workflow check. At integration,
run the repository CI checks applicable to changed crates/adapters; preserve
Tauri compatibility while it remains shipped. Platform-specific input, a11y and
packaging need native results on each supported platform before release claims.

## Execution discipline and reporting

- Work from this backlog until a usable outcome is delivered. Do not stop at
  plumbing extraction or declare success from a test count.
- Keep one runnable integrated build and one current defect list. Update this
  document with completed behavior and concrete blockers; avoid proliferating
  historical seal/report folders as a substitute for product work.
- Report: workflows now usable, workflows still broken, next integration target.
  A blocker names the failing API/behavior, reproduction and owner needed to fix
  it. Do not turn general uncertainty into globally disabled controls.
- No speculative calendar estimate: the prompt/input and rich-content contracts
  are the immediate technical uncertainties. Finish batch 1, measure throughput,
  then forecast the remaining named backlog without weakening the finish line.

## Current delivery state

- [x] Source-backed diagnosis and implementation order established.
- [ ] Batch 1: production-path chat foundation.
- [ ] Batch 2: composition and controls.
- [ ] Batch 3: typed transcript and recovery.
- [ ] Batch 4: workspace and coordination parity.
- [ ] Batch 5: packaged daily-use chat cutover.
- [ ] Batch 6: remaining desktop and replacement release.

This planning change does not alter application behavior. No test suite, GUI,
provider call, daemon restart, release build or integration was performed as part
of writing this plan.
