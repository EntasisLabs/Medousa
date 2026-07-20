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
4. **`ChatStreamPool`:** slots keyed by `sessionId` with `acquire` / `release` / `setMaxLive`. **v1 `maxLiveStreams = 1`** — focused chat pane owns the live SSE; other chat panes show a cached/idle view. Raising max later should not require host rewrites.
5. **LME / web:** one shared host each (focus steals). Dual editors deferred.
6. Do **not** reuse custom-surface `TilingNode` for shell panes.

## Consequences

- Persistence key `medousa-home-shell-tabs-v2` stores tabs, groups, splitRoot, activeGroupId, zoom.
- Spotlight exposes pane commands under Advanced.
- Follow-ups: `maxLiveStreams > 1`, drag tab to split, dual LME/web, remappable keys.

## Code anchors

- `apps/medousa-home/src/lib/stores/shellTabs.svelte.ts`
- `apps/medousa-home/src/lib/stores/chatStreamPool.svelte.ts`
- `apps/medousa-home/src/lib/utils/shellSplitTree.ts`
- `apps/medousa-home/src/lib/utils/shellPaneHotkeys.ts`
- `apps/medousa-home/src/lib/components/shell/ShellTabHost.svelte`
