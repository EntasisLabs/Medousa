# Native Notes parity — implementation and acceptance ledger

Baseline: `3f6c31eb` (after core workflow pass `0bf1a2a4`). This is a migration ledger, **not a declaration of parity**. Implementation and static inspection do not certify rendered or interactive equivalence. Coordinator owns shared shell/style/build integration; Notes owner owns `notes*.rs`.

## Reference implementation and reusable contracts

- Svelte host: `apps/medousa-home/src/lib/components/vault/VaultEditor.svelte` (Live/Build/source/read/split, kind routing, outline, find, versions, proposals, exports, note-chat).
- Editor: `VaultMarkdownEditor.svelte`, `VaultLiveEditor.svelte`, `VaultMarkdownCodeMirror.svelte`, `VaultFormatBar.svelte`, `VaultSelectionFormatBubble.svelte`, `VaultSlashMenu.svelte` in that directory; serializers/actions in `src/lib/utils/vaultMarkdownEdit.ts`, `vaultFormatActions.ts`; retained runtime in `src/lib/vault/noteEditorRuntime.ts` and `vaultEditorController.ts`.
- Library: `VaultLibraryChrome.svelte`, `VaultLibraryBrowseModeBar.svelte`, `VaultLibraryBrowseLists.svelte`, `VaultTree.svelte`, `VaultTreeNode.svelte`, `VaultTreeRecentRows.svelte`, `VaultQuickSwitcher.svelte`, `VaultRootPicker.svelte`; controllers in `src/lib/vault/vaultRailController.ts`, `vaultBrowseController.ts`, `vaultRootsController.ts`, `vaultSearchCatalog.svelte.ts`.
- Existing shared buffer: `crates/medousa-app-core/src/vault_buffer.rs` (`VaultBufferCore`, `ApplyEdit`, `ReconcileFromDisk`, `MarkSaved`, `Reset`); opaque If-Match baseline is daemon authority. SDK operations and native management already serve daemon filesystem authority, not local upload.
- RNDR source: sibling `rndr/crates/rndr_ui/src/editing.rs` (`EditorSurface`), `rndr_core::editing::SourceDocuments`.
- RNDR **existing rich editor**: sibling `rndr/crates/rndr_markdown_ui/src/{lib,document,surface}.rs` (`MarkdownPlugin`, `MarkdownDocuments`, `MarkdownSurface`, `MarkdownCommand`), `rndr/crates/rndr_markdown/src/{session,edit}.rs` (`Session`, retained concrete Markdown bytes, history, composition). Reuse this, not a custom block engine. It is a separate session from SourceDocuments: cross-mode synchronization/history must be tested explicitly.
- RNDR specialized existing UI: `rndr_ui/src/data_editors.md`, `rndr_canvas_ui/src/editor.rs`. Availability is not equivalent to source-faithful Notes serializers; inspect exact adapters before wiring table/kanban/slides/draw/workbook routes.

Paths named without a prefix above are relative to `apps/medousa-home/src/lib/components/vault/` unless qualified.

## Feature matrix

Status legend: **baseline** = existing native workflow, not independently re-certified; **gap** = missing or materially incomplete; **in progress** = current Notes work, not acceptance.

| Capability | Svelte source / contract | Native baseline / remaining work |
|---|---|---|
| Retained drafts, explicit save, remote conflicts, unknown write outcome | `vaultEditorController.ts`, `noteSaveQueue.ts`, `VaultConflictBar.svelte` | **baseline** `notes.rs`, shared `VaultBufferCore`; tokened writes, edits during save, explicit latest-baseline acknowledgement; retain no-blind-retry behavior |
| Multi-note tabs, keep-alive navigation, IME guards | `noteEditorRuntimes.svelte.ts`, `VaultEditor.svelte` | **baseline** retained source handles and tabs; **implemented, unverified** shared tab reorder with open-order/identity retention; **gap** strip close disposal, persistent restart recovery, richer panes |
| Create, rename/move, trash/restore | `VaultNewNoteDialog.svelte`, `VaultNoteActionsMenu.svelte`, `VaultTrashPanel.svelte` | **baseline** `notes_management.rs`; clean-buffer/identity gates; **gap** folders, bulk selection/actions, DnD, permanent deletion/trash management |
| Multiple daemon vault roots | `VaultRootPicker.svelte`, `vaultRootsController.ts` | **baseline** add/select roots and root-reset gates; **gap** full preference/remove/configure root management |
| Search | `vaultSearchCatalog.svelte.ts`, `VaultQuickSwitcher.svelte`, `VaultFindBar.svelte` | **baseline** local loaded-note filter + explicit daemon full-text search; **implemented, unverified** retained shared source find/replace; **gap** fuzzy quick switcher, search scopes/filter/sort, rich/read rendered-text search and host primary-F hook |
| Navigation tree / browse / recent / spaces | library components above, `vaultTree.ts`, `vaultSpaces.ts` | **implemented, unverified** shared RNDR `VirtualTree`, nested folder disclosure, root-scoped stable IDs, ancestor reveal on explicit selection, search expansion without changing saved disclosure, keyboard selection snapshots drafts; **implemented, unverified** Folders/Recent browse with session-root visits and modified fallback; **gap** persisted recents/space chips/other browse modes, server-complete catalog and DnD |
| Rich Live editing | `VaultLiveEditor.svelte`, `VaultMarkdownEditor.svelte` | **implemented, unverified** existing RNDR `MarkdownSurface`, retained per-path rich sessions and source history, exact frontmatter/BOM/newline retention, composition-aware mode/note/root switching; coordinator supplied dependency/plugin wiring; rich theme/disposal gaps below |
| Source editing and read mode | `VaultMarkdownCodeMirror.svelte`, `VaultMarkdownPreview.svelte` | **baseline** RNDR EditorSurface + MarkdownView reading; **gap** complete language/editor commands, Live/Build plane equivalence and synchronized split preview |
| Formatting / slash / selection bubble | `VaultFormatBar.svelte`, `VaultSlashMenu.svelte`, `vaultMarkdownEdit.ts` | **implemented, unverified** RNDR `MarkdownCommand` toolbar for bold/italic/strike/code/heading/list/quote/fence/undo/redo; **gap** slash insertion, selection bubble, links/picker, callout/chart/Liquid builders |
| Frontmatter/properties/tags | `VaultLiveProperties.svelte`, `vaultFrontmatter.ts` | **implemented, unverified** frontmatter hidden in read and rich mode while exact source prefix is retained (including BOM/CRLF); **gap** typed metadata/property editor/tag management |
| Note statistics/status | `VaultNoteStatusBar.svelte`, `vaultNoteStats.ts` | **implemented, unverified** body-only word/read/UTF-16 character stats and live status footer; source metadata excluded |
| Links, backlinks, embeds, outline | `VaultNoteLinksPanel.svelte`, `MarkdownHeadingOutline.svelte`, `vaultNoteBridge.ts` | **gap** interactive wikilinks/transclusion and backlinks panel, navigable outline |
| Attachments/image paste/locality | `VaultAttachmentPanel.svelte`, `VaultLinkedFilesMenu.svelte`, `vaultImagePaste.ts`, `vaultAttachments.ts` | **gap** daemon-backed attachments, preview, paste/upload; never substitute local filesystem for remote workshop authority |
| Specialized note kinds | `LedgerTableEditor.svelte`, `KanbanBoardEditor.svelte`, `SlidesDeckEditor.svelte`, `DrawNoteEditor.svelte`, `WorkbookManifestEditor.svelte` | **gap** route source-faithful adapters to shared RNDR specialized UI; source/read alone are not kind parity |
| Versions, proposals/review/diff | `VaultVersionsPanel.svelte`, `VaultProposalBar.svelte`, `VaultProposalDiffPreview.svelte`, `vaultVersions.svelte.ts` | **gap** distinct from save-conflict baseline UI |
| Templates/import/export/print | `vaultTemplates.ts`, `vaultUserTemplates.ts`, `VaultGarageImportWizard.svelte`, `VaultExportPreviewModal.svelte`, `vault{Pdf,Docx,Html}Export.ts` | **gap** templates, import workflow and export-format pipelines/previews |
| Note-chat/context/work handoffs | `VaultNoteChatFab.svelte`, `VaultNoteWorkshop.svelte`, `vaultNoteWorkshop.ts` | **gap** coordinate Chat/Code navigation and daemon context contract; coordinator-controlled cross-surface integration |
| Sticky/popout/multi-pane/mobile | `VaultEditor.svelte`, `vaultSticky.ts`, `routes/popout/vault` | **gap** native host integration and narrow-screen behavior; coordinator controlled |
| Appearance and keyboard/accessibility | `styles/vault-*.postcss`, `vaultHotkeys.ts`, component semantics | **gap** compare actual renderings, keyboard navigation and platform IME/accessibility; source structure is not visual evidence |

## Implementation sequence and non-negotiable invariants

1. Audit/reuse shared contracts; implement discrete behavior chunks with regression tests before visual matching. Keep exact tracked-file staging boundaries.
2. Integrate RNDR rich editing and command toolbar; preserve Markdown source/frontmatter. Never parse/print a whole note just to save. Rich/source mode switches must capture the outgoing writer before showing the incoming one, and refuse while either owns active composition.
3. Improve library interaction/status, then close the remaining matrix rows with concrete shared API requests rather than equating CRUD with full parity.
4. No cargo until coordinator serializes the existing native target with `-j2`. Static format/diff checks are allowed. No installs, new worktrees, targets, broad builds or deletion of evidence.
5. Every mutation is explicit and single-flight. Unknown outcomes require observation, not blind replay. Root identity remains part of buffer authority and root switching cannot discard drafts/conflicts.

## Visual and interaction acceptance plan (not yet executed)

Capture **both** Svelte/Tauri and native at identical content, window bounds, scale and appearance: dark/light at 1440×900 and narrow 900×850, plus long/scrolled note, read/source/Live, empty state, tree search, tabs with dirty state, conflict and management panels. Compare typography, page width/margins, sidebar density/selection, chrome heights, borders/surfaces, formatting controls, footer, scroll and focus. Keep side-by-side artifacts and review differences; existing native screenshots alone are not a parity comparison.

Exercise: create/open/type/undo/redo; switch note/surface/mode and return; Unicode/IME during switch; source/frontmatter exact-byte round-trip; edit during save; external daemon update/conflict acknowledgement; unknown write outcome; stale pagination/search response; duplicate paths across roots; dirty root switch/close; keyboard tab/focus, narrow overflow; rich formatting and source reflection. Coordinator runs targeted `notes::tests` and native render fixtures only after build admission.

## Shared capability blockers and next integration gate

- Coordinator owns `ui.rs`/`native_style.rs` and RNDR crates. Native dependencies and the rich plugin are now wired; no Notes-owned edits to those files.
- `rndr_markdown_ui/src/surface.rs::style_for_kind` builds a literal `TextStyle::default()` and the public `MarkdownSurfaceProps` has no semantic foreground/theme prop, unlike native source/reading styles. A shared semantic theme-color API / update hook is needed so mounted retained rich documents respond to dark/light changes. Do not solve this with a Notes-local renderer or duplicate shared theme palette.
- `rndr_markdown_ui::MarkdownDocuments` exposes open/get/get_mut but no close/dispose. Native move/trash/root switching must remove the retained rich session as well as despawn its surface; coordinator must add the shared lifecycle API, then Notes management can release sessions. Current entity teardown is present, retained-session disposal remains a real gap.
- RNDR rich surface has retained byte/history and composition support but not the complete Svelte Live block ecosystem: specialized note adapters, media/embed/link interaction, slash/selection affordances still require shared adapters and coordinated owner scope.
- Cargo, actual interaction and rendering are coordinator-serialized. Notes owner has requested targeted `notes::tests` (includes nested management tests), then existing ignored `notes_visual_acceptance` plus new rich-mode rendered cases. No build success or visual acceptance is claimed here.

## Verification ledger

- Initial inspection: `notes.rs`, `notes_management.rs`, existing `notes_tests.rs` and management tests; Svelte editor/library component inventory; shared `VaultBufferCore`, RNDR rich/source APIs above.
- Static checks: Notes-owned Rust files formatted with `rustfmt --edition 2024`; scoped `git diff --check` passes. Added retained rich/source/frontmatter/undo/composition/edit-during-save/statistics tests and shared folder-tree search/keyboard draft-retention regressions.
- Builds/tests/render comparison: **not run by Notes owner** (coordinator serialization required). No rendered parity claim. Do not report full parity until all matrix rows and rendered comparisons have acceptance evidence.

## Follow-up implementation (2026-11-16; after review of 9455d091)

This is additional **uncompiled functional work**, not assignment satisfaction or visual acceptance.

- **In-note find/replace:** Notes now mounts the existing `rndr_ui::source_tools::FindBar` against each retained source document. The existing host `ui::update` dispatcher owns `apply_find_bars`; Notes does not register another dispatcher. Find captures the current rich writer before explicitly entering source mode (no custom rich search renderer); retains query, replacement, source undo and document identity per note; hides bars for inactive/read/rich modes; focuses the real query control. All writer transitions reject active composition. Find searches full Markdown source, including frontmatter. Direct read/rich rendered-text search is still a gap. `notes::shortcut(world, "find") -> bool` is the owned seam for host primary-F routing; toolbar works without that shared hook.
- **Library browse/recents:** Folders and Recent controls reuse the same shared virtualized tree. Recents follow `vaultBrowseController.ts` visit-order then modified-date fallback, limit **40**; filter is case-insensitive title/path and path-deduplicated. Visits are local session/root-scoped (no new persistence system); root changes clear visits/filter/tree IDs, not the monotonic ID allocator. The existing catalog/refresh/pagination remains authority. Persisted cross-session recents, deep browse and complete search controls remain gaps; explicit daemon full-text search is baseline.
- **Tabs:** open-order is retained independently of catalog order, and shared `TabStripEventKind::Reordered` / `reorder_tabs` applies reorders without changing selection, source/rich identities, drafts or history. Reorder is composition-gated. Existing guarded Close management remains, but strip-close stays disabled until the coordinator's shared Markdown disposal contract is safe; no Notes-local registry workaround.
- **Safety:** duplicate path rows are deduplicated, distinct paths with equal titles remain distinct, Selected+Activated for an unloaded note queues one load, foreign tree events and old-root IDs cannot open a different root's same-path note. Added regressions exercise these behaviors and source replace/undo/frontmatter/hash retention.
- **Render preparation:** ignored `notes_modes_appearances_widths_render_fixture` in owned `notes_tests.rs` prepares 12 synthetic cases (read/source/rich × 1440/900 widths × light/dark). Uses actual host app plus shared exporter; Unicode/frontmatter/mixed blocks/checklist/table/quote/code. Generates only on explicit test execution. **Not run**; no PNGs generated by this follow-up; actual corresponding Svelte/native pixel comparison remains mandatory. Existing screenshots are not evidence for these sources.

### Concrete coordinator dependencies (unchanged ownership)

1. Primary-F: shared `apps/medousa-native/src/ui.rs::update` should route Notes-only primary-F to `notes::shortcut(world, "find")`, following existing Code event-consumption and active-surface checks. No Notes edits to that host-owned file.
2. Theme: RNDR `crates/rndr_markdown_ui/src/surface.rs:1065` (`Palette::default`) and `:1350` (`style_for_kind`), with `project.rs::Palette`, still need semantic live palette support. Host owns this; source/read theme cannot establish rich light/dark acceptance. Exact prior API details above remain applicable.
3. Lifecycle: RNDR `crates/rndr_markdown_ui/src/document.rs:11::MarkdownDocuments` needs safe removal including associated source document, anchors, history/composition and mounted-surface lifetime. Current Notes `notes_management.rs::forget` is the existing integration point. Host must supply exact disposal API before tab-close lifecycle improvements; no custom Notes session registry.
4. Verification: cargo remains host-serialized; Notes has **not** invoked build/test. Coordinator should use existing target, offline `-j2`, then targeted `notes::tests` (includes management) and explicitly ignored fixtures. Stale Chat failure logs do not describe this source revision.

Remaining full parity gaps still include specialized properties/adapters (including calendar/agent/creative integrations), slash/block/selection tooling, links/backlinks and navigable outline, media upload/embed/resize and Liquid interaction, full rich shortcuts, autosave/conflict polish, broader/persisted browse/search and responsive/visual matching. None are silently marked complete by the additional find/recents/reorder implementation.
