# ADR-010: Virtual shell workspaces

## Status

Accepted

## Context

[ADR-007](adr-007-shell-split-panes.md) persists one shell layout (`tabs` / `groups` / `splitRoot`). Users want Hyprland / Windows–style **virtual desktops**: named, switchable arrangements of tiled panes without cloning vaults, chat sessions, or the workshop connection.

## Decision

1. **Workspace = layout snapshot.** A `ShellDesktop` is `{ id, name, layout }` where `layout` is the former v2 shell blob. Content stores (vault, chat, LME, browser, workshop) stay global.
2. **Persistence v3** (`medousa-home-shell-tabs-v3`): `{ desktops[], activeDesktopId }`. Migrate existing v2 (or v1) → one desktop named `"Main"`.
3. **Caps stay per desktop:** max 4 panes and 16 tabs apply to the active snapshot only.
4. **Same chat on two desktops is allowed** — each desktop holds its own shell-tab ref to the same `sessionId`. The stream pool only lives for the **active** desktop’s panes; switch flushes, hydrates, then release/acquire.
5. **Open / sync scope:** `openChat` / `openLme` / `openWeb` “already open elsewhere” and `syncFromLme*` / `syncFromHumanBrowser` mutate the **active** desktop only (in-memory layout is always the active snapshot).
6. **DOM:** mount only the active desktop’s tree (`{#key activeDesktopId}`); no N×4 keep-alive.
7. **Not** multi-vault compose, OS virtual desktops, or Stasis `stasisd`.

## Consequences

- Spotlight: New / Switch / Rename / Remove workspace.
- Status bar shows the active workspace name (click cycles when more than one exists).
- Mobile shell and sticky Live window remain out of scope for v1.

## Code anchors

- `apps/medousa-home/src/lib/stores/shellTabs.svelte.ts`
- `apps/medousa-home/src/lib/types/shellTabs.ts`
- `apps/medousa-home/src/lib/components/shell/ShellTabHost.svelte`
- `apps/medousa-home/src/lib/commands/registry.ts` (`buildWorkspaceCommands`)
