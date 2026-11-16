# Native Code parity audit

Baseline: `3f6c31eb` (`0bf1a2a4` workflow baseline), attached `medousa/runtime-ui-fixes`.
This is an implementation ledger, **not a parity declaration**. Static inspection is not runtime or visual verification. Compilation is serialized by the coordinator; this worker does not run Cargo without authorization.

## Sources and reusable contracts

Reference paths below are relative to `apps/medousa-home/src/lib/` unless stated otherwise.

- Surface composition: `components/code/CodeEditorShell.svelte`, `CodeEditorWorkspace.svelte`, `CodeEditorChrome.svelte`, `CodeEditorDialogs.svelte`.
- Navigation: `components/lme/explorers/LmeCodeExplorer.svelte`, `CodeRepositoryTree.svelte`, `CodeRailSwitcher.svelte`; `code/codeWorkbenchState.svelte.ts`, `codeQuickOpenController.svelte.ts`.
- Editor and commands: `components/code/CodeMirrorHost.svelte`, `CodeEditorContextMenu.svelte`, `CodeFindBar.svelte`, `CodeCommandPicker.svelte`; `code/codeLanguageNavigation.ts`, `codeWorkspaceEdit.ts`, `codeSaveController.svelte.ts`.
- Feedback: `components/code/CodeChangesPanel.svelte`, `CodeProblemsPanel.svelte`, `CodeTestsExplorer.svelte`, `CodeTasksOutput.svelte`, `CodeTaskTerminal.svelte`; `code/codeChangesController.svelte.ts`, `codeTasksController.svelte.ts`.
- Projects/terminal: `components/code/CodeProjectCreationFlow.svelte`, `components/work/CodeTerminalDock.svelte`, `components/terminal/DesktopTerminal.svelte`, `TerminalPane.svelte`.
- Native adapter: `apps/medousa-native/src/code.rs`, `code_workbench.rs`, tests in `code_tests.rs`, `code_workbench_tests.rs`.
- Canonical native text: sibling RNDR `crates/rndr_core/src/editing/`, `crates/rndr_ui/src/editing.rs`, `source_tools.rs`, `tabs.rs`, `menu.rs`, `diff.rs`, `split.rs`. These own editing/history/selection, literal find/replace, tab gestures, menus and diff presentation. Do not replace with native-local editor history or a custom input dispatcher.
- `crates/medousa-app-core/src/lib.rs` and modules: shared session/history/vault buffer/undertaking binding exist; there is no Code workbench or VT terminal implementation here. Native reuses SDK operation identities and the daemon's Forge source/task/language contracts rather than inventing project execution authority.

## Feature matrix

| Reference behavior | Native baseline / implementation status | Remaining acceptance / dependency |
| --- | --- | --- |
| Repository/project/thread hierarchy and disclosure | Baseline groups daemon git repo identity, work-scopes unknown identities, retains editor selection | Project search, pin/archive/rename/remove menus, switchers, persistent preferences; compare actual explorer rail |
| File tree folders, selected file, dirty badge | Baseline bounded daemon tree and disclosure | Tree context menus, create/rename/delete and external refresh workflows; no local disk assumptions |
| Quick open | Added local path filtering over bounded daemon tree; safe open reuses retained documents | Keyboard routing via coordinator; reference supports richer live picker/navigation |
| Source document editing / undo / selection / scroll | RNDR SourceDocuments + EditorSurface baseline | Verify keyboard, Unicode, IME, clipboard and independent document histories interactively |
| Open tabs / overflow / drag order / close | Added host-owned open order, RNDR reorder, draft-safe close; closing hides but retains document/draft/history for reopen | Verify mouse, middle-click, keyboard and project switches; no cross-group split claim |
| Find / replace | Added per-document canonical RNDR FindBar, retained fields and readonly policy | Shared UI must run apply_find_bars before draining action intents; test literal Unicode/replacement/hidden scope |
| Digest-versioned saves / ownership | Baseline source identity/digest + Forge human lease generation, heartbeat, dirty and uncertain-save protection | Preserve all guards; never use successful compilation as evidence of race safety |
| Conflict comparison / keep draft | Baseline bounded comparison and explicit acknowledge baseline, separate Save | Rich three-way/diff UX and use-project-version confirmation remain gaps |
| Problems / outline / hover / definition / references | Baseline saved-file daemon observations; added code.request references; stale digest/project replies rejected | Navigation capability availability, declaration/type/implementation, inline diagnostics, LSP logs/repair |
| Rename / format / organize imports / workspace edits | Not complete | Reuse daemon code.request + atomic Forge workspace operations; preview, URI/digest/lease preconditions and dirty-buffer protection needed. Svelte codeWorkspaceEdit.ts planner has no app-core equivalent; request shared extraction rather than silently skip atomicity |
| Search saved project contents | Baseline literal bounded daemon search and open-hit routing | Regex/case/include/exclude/replace preview workflow and retained results |
| Changes / diff / history | Daemon read endpoints; added RNDR virtual DiffView hunks/markers/line numbers, bounded history JSON | Validate actual hunk marker/blank-line rows; side-by-side and compare tabs not baseline parity |
| Detected tasks / run history / output / cancel | Baseline daemon task contracts and lease-gated start, unknown outcome protected | Test targets/test explorer, streamed incremental output, stop state and VT rendering |
| Terminal dock / sessions / resize / input / popout | **Not implemented**; task text output is not terminal parity | Tauri uses xterm. No VT terminal in app-core or RNDR discovered by module/source audit. Coordinator notified: reusable terminal capability/ownership required; do not invent a Prompt masquerading as terminal |
| Project setup/import/create/thread lifecycle/review/agent handoff | Existing projects and begin/continue editing only | CodeProjectCreationFlow and explorer menus have substantial workflows beyond listing. Shared navigation/backend integration must be coordinated |
| Context/world panel / language and editor preferences | Not complete | Shared app-core and native host surface ownership; RNDR gutter/scrollbar primitives available |
| Visual density/resizing/themes/narrow layouts | Existing native_style + RNDR tokens, not yet matched against current Svelte | Coordinator-owned shared styling/visual harness; no screenshot parity claim without pixel inspection |

## Verification and visual plan

1. Owned regressions: tab insertion/reorder/foreign IDs, close current/last/dirty/uncertain/pending-save tab, reopen same DocumentId and undo state, project switch, composition refusal; canonical find mounting, query retention and hidden scope; safe quick-open paths.
2. Coordinator serial Cargo lane: existing native target only, `-j2`; run Code and workbench unit tests, then combined native test suite. No installs, alternate targets/profiles or workspace-wide builds.
3. Coordinator visual harness: Code populated tree/open tabs and overflow; long paths; dirty/conflicted/readonly/loading/empty states; search/quick-open/find/results; dark/light at wide and narrow viewport, zoom and keyboard focus. Inspect actual images, not receipts alone.
4. Interactive: drag reorder, middle-click close/reopen dirty document, find Unicode and replace undo, IME across project/tab change, source conflict/version race, stale lease, unconfirmed save and task start recovery. Test terminal/refactor only once their real dependencies land.

## Ownership / handoff

Worker owns only `apps/medousa-native/src/{code.rs,code_workbench.rs,code_tests.rs,code_workbench_tests.rs}` and this requested matrix. Shared UI/backend/style/visual harness, app-core and RNDR require coordinator-granted narrow ownership. Shared find dispatcher hook and terminal capability decision requested promptly. Scoped local commits only; preserve unrelated staged files, target contents and verification evidence.

## Current chunk evidence (2026-11-16)

Implemented, not yet runtime accepted: retained draft-safe RNDR tab close/reorder, retained RNDR FindBar, bounded-tree quick open, saved-digest references navigation, RNDR DiffView with daemon hunk kinds and line numbers. `code::shortcut(world, "find" | "quick-open")` is the narrow keyboard integration seam; it rejects composition and missing target identity. Existing lease/version/save protections remain in their original adapter.

Validation performed by worker: owned-file `rustfmt --edition 2021` and `git diff --check` pass. Added/updated unit regressions for retained tab state/order, hidden find scope, safe quick open, canonical diff mounting/blank rows, references saved digest/dirty-file refusal and scoped shortcuts. **Tests have not been compiled or executed**: coordinator owns the serialized Cargo lane. No new native/Svelte screenshots have been generated or inspected for this chunk.

Pending exact shared requests: run canonical `rndr_ui::source_tools::apply_find_bars(world)` in shared UI before consuming its local intents; wire surface-scoped find/quick-open shortcuts; allocate shared workspace-edit planning/preview/atomic apply ownership for refactors; coordinate reusable terminal component and project lifecycle/persistence integration. These are not silently marked done or replaced by native-local editor/terminal models.
