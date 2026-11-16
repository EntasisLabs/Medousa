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
| Work dock / peer handoff | `ChatWorkDock.svelte`, `AgentTaskDetail.svelte` | `collaboration.rs`, `collaboration_peers.rs` | Worker exchange, peer history/reading and governed actions exist; **gaps:** dock grouping/counts, finished dismiss/restore preferences and compact task tray parity |
| Sticky/context usage | `ChatStickyBoard.svelte`, context panels | `context.rs` | Revision-fenced operations and usage projections present; visual/evidence audit pending |
| Narration / voice | `ChatNarrationToggle.svelte`, `ChatVoiceRecorder.svelte` | no native module identified | **Gap:** playback/recording integration; needs shared native audio capability decision, not imitation UI |
| Provider conversations | `ExternalConversationTranscript.svelte`, external controller | `runtimes.rs`, `provider_timeline.rs` | Native runtime attach/read/send/stop present; verify route-specific busy/uncertain behavior |

Paths in the Svelte column are relative to `apps/medousa-home/src/lib`; native paths relative to `apps/medousa-native`. Shared canonical contracts: `crates/medousa-app-core/src/{chat.rs,contract.rs,shell.rs}`. RNDR menu/drag/completion/tab/split controls live in the existing adjacent `rndr/crates/rndr_ui/src` tree; no parallel widgets/state machines should be introduced.

## Ownership / verification constraints

Chat owner may edit owned Chat modules and this matrix. `ui.rs`, `backend.rs`, `auxiliary.rs`, app-core and RNDR require coordinator agreement. No shared files edited by this worker. Existing owned `conversations.rs::tab_events` provides the event hook, avoiding the initially requested `ui.rs` ownership. Remaining shared changes require coordinator decisions: main host input admission for paste/drop, composer completion placement in ui.rs, drawing/audio integration in auxiliary.rs/main.rs, canonical family discovery via backend/app-core, and RNDR Liquid/artifact presentation capability. Work-dock grouping needs a shared typed grouping projection rather than parsing display strings into a parallel workflow model. No cargo/build/render until coordinator serializes the existing native target with `-j2`; no dependency installs, alternate profiles or worktrees.

## Rendered acceptance plan

After functional regressions and coordinator compile slot: use existing RNDR Exporter synthetic fixtures at wide/narrow sizes in both appearances; inspect pixels, not merely export success. Cover populated and empty history, pinned/search/collapsed states, row menu and destructive confirmation; retained multi-pane draft; long/streaming tool timeline and return-to-latest; work dock pending/finished; image tray and composer wrapping. Compare to actual Svelte component anatomy and existing verification evidence. Existing screenshots predate this change and are not evidence of this patch. Live daemon mutation scenarios need governed test sessions; never delete real user chats for verification.

## Current outcome

Implemented the bounded history/tab/timeline functionality above, retaining ChatCore/ShellCore and RNDR widgets. **Chat parity remains incomplete.** Concrete blockers: coordinator has not granted the serialized cargo slot or the shared capability changes listed above. No cargo command, dependency install, alternate build, or application restart has been run by this worker.

Verification performed: rustfmt (edition 2021) on the four owned edited Rust modules; `git diff --check` on those files. New regression coverage in `chat_sidebar.rs` includes scoped pin isolation/non-navigation, atomic preference round-trip and corrupt-file errors, row-menu captured identity, rename/delete cancel and stale ownership fences; existing recency/search/history-append coverage retained. `conversations.rs` tests cover malformed/self/stable reorder; `timeline_tests.rs` covers rail controls and target availability. These tests are **added, not executed** until the coordinator grants the compiler slot.

Added ignored GPU acceptance harness `chat_sidebar::tests::chat_interaction_visual_acceptance`: exports pinned/sidebar menu/rename dialog in dark wide and light narrow states to `verification/native-chat-parity-{pinned,menu,rename}-{dark,light-narrow}.png`. Harness is **not executed**; no new rendered pixels are claimed. After coordinator compilation, run targeted tests on the existing native target with `-j2`, then the ignored harness and inspect each image via the image tool. Existing unrelated evidence/drafts, Code/Notes/shared file changes and untracked target files are preserved.
