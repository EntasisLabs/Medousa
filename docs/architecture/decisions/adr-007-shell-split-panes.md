# ADR-007: Shell split panes + stream pool

## Status

Accepted

## Context

[ADR-006](adr-006-shell-tabs.md) introduced shell tabs with a single `EditorGroup`. Multi-chat and TMUX/VS Code–style tiling need a binary split tree, hover-revealed per-pane tabs, keyboard-first pane ops, and a path to multiple live chat streams without rewriting pane hosts.

## Decision

1. **Binary split tree** (`SplitNode`: leaf group | row/column branch + ratio) over `EditorGroup` leaves. Soft cap **4 panes** in v1.
2. **Tabs are chrome:** per-pane strip hidden until hover, focus, or `Ctrl+; w`.
3. **Keyboard:**
   - `Ctrl+;` prefix → pane ops (`%`/`"` split, hjkl focus, `z` zoom, `x` close, …)
   - `Ctrl+B` → toggle left master rail (VS Code/Cursor); never pane ops
4. **`ChatStreamPool`:** slots keyed by `sessionId` with `acquire` / `release` / `setMaxLive`. **`maxLiveStreams = MAX_SHELL_PANES` (4)** — every chat pane can be live; pool LRU-evicts when a 5th session acquires. Demoted sessions stop owned SSE but keep cached transcripts (`ChatPaneIdle` only when `!isLive`).
5. **Compose:** only the focused pane’s chat accepts send/input; background live panes still stream and update transcripts (`ChatSessionView` with `interactive={focused}`).
6. **Workspace (LME):** every pane with a Workspace/note tab mounts `LmePanel` (focused pane is interactive; background panes render read-only). Note bodies are **path-keyed buffers** on `vault` (`contentFor` / `warmBuffer` / stash on `openNote`) so two panes can show different notes without stealing. **Web:** still one shared browser host (focus steals) — dual webviews deferred.
7. Do **not** reuse custom-surface `TilingNode` for shell panes.

## Consequences

- Persistence key `medousa-home-shell-tabs-v2` stores tabs, groups, splitRoot, activeGroupId, zoom.
- **Restart restore:** shell chrome hydrates from that key; open chat panes are re-acquired into `ChatStreamPool` (active first, up to `maxLiveStreams`); background sessions warm history via `warmBackgroundSession`. Main window size/position/maximized persist via `tauri-plugin-window-state` (desktop, label `main` only).
- Spotlight exposes pane commands under Advanced.
- Session-scoped chat runtimes (`chat.svelte.ts` + `chatSessionRuntime.ts`) keep messages/drafts across focus swaps; stream events route by owned `turnId` → `sessionId`.
- Path-keyed note buffers (`noteBuffer.ts` + `vault.svelte.ts`) keep demoted notes visible in background panes; shell passes `lmeTabId` → `VaultEditor path`.
- Follow-ups: same note in two panes, dual webviews, remappable keys, soft LRU for cached runtimes.

## Code anchors

- `apps/medousa-home/src/lib/stores/shellTabs.svelte.ts`
- `apps/medousa-home/src/lib/stores/chatStreamPool.svelte.ts`
- `apps/medousa-home/src/lib/utils/shellSplitTree.ts`
- `apps/medousa-home/src/lib/utils/shellPaneHotkeys.ts`
- `apps/medousa-home/src/lib/components/shell/ShellTabHost.svelte`
