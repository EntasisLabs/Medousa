# Native Chat parity

Baseline: `3f6c31eb` (2026-11-16). This is a feature audit, not a declaration of parity. Native uses the same app-core ChatCore/ShellCore; presentation preferences are device-local, never fabricated daemon facts.

## Matrix

| Feature | Svelte/Tauri reference | Native reference | Baseline → current implementation / remaining work |
|---|---|---|---|
| Canonical history, stream reconciliation | `lib/chat/chatSessionRuntime.ts`, `lib/chat/sessionController.ts` | `src/chat.rs::hydrate/apply`, `src/backend.rs::history` | Implemented using ChatCore; verify reconnect and background-turn tests |
| History search / pagination / recency | `components/chat/SessionSidebar.svelte`, `utils/sessionHistoryGroups.ts` | `src/chat_sidebar.rs`, `ui.rs::apply_current_reply` | Recency groups, counts, search expansion and stale-query fencing present |
| Pinned history | `chat/sessionController.ts::loadPinnedIds/togglePin`, `SessionSidebar.svelte:225` | `chat_sidebar.rs::{set_pin,pin_store,sync}` | Baseline notice-only action replaced at the owned action hook by authority-scoped device-local pins, separate Pinned section, asynchronous atomic persistence and explicit read/write error reporting. Test execution pending |
| Row actions / context menu | `components/chat/SessionRow.svelte`, `ChatForkMenu.svelte` | `ui.rs` session actions | Implemented identity-bound RNDR Menu on secondary press/keyboard context gesture, scoped row pin/fork, RNDR Dialog rename/delete with cancel, generation/authority fences and no composer navigation. Test execution pending |
| Related-session families / origin markers | `SessionSidebar.svelte::sessionFamilies`, `SessionChannelMarks.svelte` | `chat_sidebar.rs` | **Gap:** flat history, no canonical relation discovery in sidebar; metadata marks absent |
| Tabs, split panes, reorder / cross-pane move | shell tab surfaces and app-core ShellCore | `conversations.rs::tab_events` | RNDR TabStrip/SplitSash retained. Added RNDR drag reorder and Alt+Left/Right keyboard reorder, stable sorting across history refresh, malformed/self reorder safety tests. History is chronological, not user-draggable in SessionRow reference. Cross-pane movement needs rendered verification |
| Draft isolation / IME / close without cancel | `ChatSessionView.svelte`, chat runtime | `conversations.rs`, `ui.rs` SourceDocuments | Retained per-conversation document and route ownership present; compile/regression gate required |
| Composer model, mode, depth, runtime | `ChatComposerBar.svelte`, `ChatRuntimeControlRow.svelte` | `ui.rs`, `runtimes.rs` | Native selectors, text editing and stop/send exist; exact rendered layout needs comparison |
| Attachments | `ChatComposerAttachments.svelte`, `ChatAttachmentChips.svelte`, image paste utilities | `attachments.rs`, `media_preview.rs` | Picker/upload/chips/images present; **gaps:** image clipboard/file-drop admission, voice recording and drawing dialog |
| Slash / mention completion | composer completion components | `ui.rs` | **Gap:** no corresponding host completion wiring identified; RNDR completion primitives exist |
| Timeline / markdown / tools / images | `ChatMessageList.svelte`, `VirtualChatBeat.svelte`, `ChatTaskGroup.svelte` | `timeline.rs`, `tool_activity.rs`, `media_preview.rs` | Virtualized retained rows, canonical presentation, disclosure/copy/tool grouping present |
| Scroll following / navigation | `ChatScrollChrome.svelte`, `utils/chatTurnNavigation.ts` | `timeline.rs::sync` | Native following and scrollbar retained; implemented explicit Return latest and Previous/Next user turn controls on the rail, disabled when no applicable target, measured post-layout scrolling and latest-follow restoration. Tests added; rendered verification pending |
| Artifact / Liquid cards | `ChatArtifactStrip.svelte`, `LiquidChatMessage.svelte` | `timeline.rs::artifact_part` | **Gap:** non-image artifacts explicitly reference-only; no full Liquid interaction renderer in this host |
| Work dock / peer handoff | `ChatWorkDock.svelte`, `AgentTaskDetail.svelte` | `collaboration.rs`, `collaboration_peers.rs` | Worker exchange, peer history/reading and governed actions exist. Added window-local earlier terminal-Done worker dismiss/restore and Show/Hide dismissed controls, keyed by authority/session and captured work ID; active/blocked rows resurface. **Gaps:** full typed dock grouping/counts, persistent workshop-scoped history preferences, peer/approval dismissal and compact task tray parity. Source coverage added, not executed |
| Sticky/context usage | `ChatStickyBoard.svelte`, context panels | `context.rs` | Revision-fenced operations and usage projections present; visual/evidence audit pending |
| Narration / voice | `ChatNarrationToggle.svelte`, `ChatVoiceRecorder.svelte` | no native module identified | **Gap:** playback/recording integration; needs shared native audio capability decision, not imitation UI |
| Provider conversations | `ExternalConversationTranscript.svelte`, external controller | `runtimes.rs`, `provider_timeline.rs` | Native runtime attach/read/send/stop present; verify route-specific busy/uncertain behavior |

Paths in the Svelte column are relative to `apps/medousa-home/src/lib`; native paths relative to `apps/medousa-native`. Shared canonical contracts: `crates/medousa-app-core/src/{chat.rs,contract.rs,shell.rs}`. RNDR menu/drag/completion/tab/split controls live in the existing adjacent `rndr/crates/rndr_ui/src` tree; no parallel widgets/state machines should be introduced.

## Ownership / verification constraints

Chat owner may edit owned Chat modules and this matrix. `ui.rs`, `backend.rs`, `auxiliary.rs`, app-core and RNDR require coordinator agreement. No shared files edited by this worker. Existing owned `conversations.rs::tab_events` provides the event hook, avoiding the initially requested `ui.rs` ownership. Remaining shared changes require coordinator decisions: main host input admission for paste/drop, composer completion placement in ui.rs, drawing/audio integration in auxiliary.rs/main.rs, canonical family discovery via backend/app-core, and RNDR Liquid/artifact presentation capability. Work-dock grouping needs a shared typed grouping projection rather than parsing display strings into a parallel workflow model. No cargo/build/render until coordinator serializes the existing native target with `-j2`; no dependency installs, alternate profiles or worktrees.

## Rendered acceptance plan

After functional regressions and coordinator compile slot: use existing RNDR Exporter synthetic fixtures at wide/narrow sizes in both appearances; inspect pixels, not merely export success. Cover populated and empty history, pinned/search/collapsed states, row menu and destructive confirmation; retained multi-pane draft; long/streaming tool timeline and return-to-latest; work dock pending/finished; image tray and composer wrapping. Compare to actual Svelte component anatomy and existing verification evidence. Existing screenshots predate this change and are not evidence of this patch. Live daemon mutation scenarios need governed test sessions; never delete real user chats for verification.

## Sender-requested corrections (follow-up to eb919e9f)

- Narrow correction committed as `ac2c729c`: `chat_sidebar.rs:184` now uses `..Layout::row()`, not nonexistent `Layout::default()`. The old E0599 log is not evidence that the corrected source compiles.
- eb919e9f did **not** genuinely disable Previous/Next: it merely no-opped on missing targets. Follow-up sets RNDR `ButtonProps.enabled` / `ControlDisabled` from target availability. Navigation resolves stable turn IDs against measured row heights after layout (including spacing/virtual spacer), retries estimated offscreen destinations, and keeps a per-pane navigation anchor through streaming/layout corrections until deliberate scroll or Latest releases it. Source regressions cover boundary disabling, pane isolation, measured corrections, streaming, manual-scroll release and Latest. They are **unexecuted**; stability is not yet a verified runtime claim.
- Earlier terminal-Done worker dismissal is presentation-only, never cancel/forget/delete. Stale authority/session clicks are rejected; no Command or daemon fact is manufactured. Current preferences intentionally last only this process/window. This is a conservative supported subset of Svelte `workDockHistory.ts`, not full persistent/history parity or a claim of canonical attention grouping.
- Tests/render remain held by host process `proc_c2c6dc54377f4a5b89e76eb1618ab397` and its durable callback. No Cargo, new target/profile/install, PTY, restart or live deletion was performed. Formatting/static inspection alone cannot establish compile or pixel parity.

### Exact shared API proposal (host-owned; not implemented here)

Request a shared app-core projection, e.g. `ChatWorkDockProjection { scope: DockScope, groups: Vec<DockGroup> }`, where:

```rust
struct DockScope { workshop_id: String, authority_id: String, session_id: String }
enum DockGroupKind { Attention, Running, Earlier }
enum DockItemKey { Budget(String), Permission(String), Proposal { channel: String, proposal_id: String }, Worker(String) }
struct DockGroup { kind: DockGroupKind, items: Vec<DockItem> }
struct DockItem {
    key: DockItemKey,
    group: DockGroupKind,
    title: String,
    status_line: String, // display only; never infer state by parsing
    detail: String,
    origin_turn_id: Option<String>,
    terminal: bool,
    streaming: bool,
    dismissible: bool, // canonical terminal && !attention, not UI guessing
}
```

The host supplies immutable scope from the actual selected workshop/execution authority and parent session; no hardcoded/default workshop ID. Stable keys come from the existing budget/permission request IDs, the existing `collaboration_peers::key` proposal binding (channel + proposal ID), and `collaboration::work_id` (`WorkCardDetail.work_id`, falling back to `card.id.0`); not row index/title or display text. Keep canonical ordering in the projection rather than adding a new timestamp heuristic. Proposed input seam: parent `ChatSnapshot` + current `Vec<WorkCardDetail>` + existing `Vec<PeerProposalReviewRecord>` + existing approval `Vec<TurnBudgetRequestRecord>` / `Vec<AgentPermissionRequestRecord>`, using current app-core presentation/peer/approval status contracts. Deduplicate shared proposal records rather than displaying peer and approval duplicates. Canonical reference: `ChatWorkDock.svelte::taskGroups`, `workDockHistory.ts::{classifyDockItem,dockItemKey}` and `utils/subagentRows.ts`. Host must map exact status/attention predicates (including pending review/blocked/error) there once, then native renders groups directly. No shared/app-core/RNDR edits are requested implicitly by this proposal.

For persistence, request the host's actual `DockScope` and reuse device-local versioned preferences keyed by JSON tuple `(workshop_id, authority_id, session_id)` matching `workDockHistory.ts`; bounded dismissed-key sets and Show dismissed state are UI preferences only. Until workshop identity and typed dismissibility are supplied, persistent all-kind dismissal and full grouping remain blocked. Canonical family discovery additionally needs host/backend relation inputs; composer input/completion/audio and Liquid/artifact capability seams remain as above.

## Current outcome

Implemented the bounded history/tab/timeline functionality above, retaining ChatCore/ShellCore and RNDR widgets. **Chat parity remains incomplete.** Concrete blockers: coordinator has not granted the serialized cargo slot or the shared capability changes listed above. No cargo command, dependency install, alternate build, or application restart has been run by this worker.

Verification performed: rustfmt (edition 2021) on the four owned edited Rust modules; `git diff --check` on those files. New regression coverage in `chat_sidebar.rs` includes scoped pin isolation/non-navigation, atomic preference round-trip and corrupt-file errors, row-menu captured identity, rename/delete cancel and stale ownership fences; existing recency/search/history-append coverage retained. `conversations.rs` tests cover malformed/self/stable reorder; `timeline_tests.rs` covers rail controls and target availability. These tests are **added, not executed** until the coordinator grants the compiler slot.

Added ignored GPU acceptance harness `chat_sidebar::tests::chat_interaction_visual_acceptance`: exports pinned/sidebar menu/rename dialog in dark wide and light narrow states to `verification/native-chat-parity-{pinned,menu,rename}-{dark,light-narrow}.png`. Harness is **not executed**; no new rendered pixels are claimed. After coordinator compilation, run targeted tests on the existing native target with `-j2`, then the ignored harness and inspect each image via the image tool. Existing unrelated evidence/drafts, Code/Notes/shared file changes and untracked target files are preserved.
