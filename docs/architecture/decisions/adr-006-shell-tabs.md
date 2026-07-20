# ADR-006: Shell-level tabs (everything is a tab)

## Status

Accepted (0.3 follow-up foundation)

## Context

Desktop Medousa used an exclusive `layout.desktopSurface` mount: switching Chat ↔ Workspace ↔ Peers remounted the center column. Document tabs existed only inside LME and the human browser. Multi-chat and VS Code–style split views need open work to stay alive as first-class tabs in a shared center host.

## Decision

1. Introduce a shell tab store (`shellTabs`) with a discriminated `ShellTab` union:
   - `chat` — one tab per open session
   - `lme` — mirrors an LME document tab
   - `web` — mirrors a human-browser tab
   - `surface` — singleton utility views (Peers, Context, Settings, …)
2. Shape the store as `EditorGroup[]` with a single `main` group today so a later split-view sprint can add groups without rewriting tab identity.
3. Render a shared `ShellTabHost` + `ShellTabStrip` in `WorkshopShell` instead of remounting panels via `navigationEpoch` for routine navigation.
4. Keep Context modes (Recall / Threads / Posture / Map) as in-panel mode bars, not shell tabs.
5. Do not reuse custom-surface widget tiling (`TilingNode`) for shell editor groups.

## Keep-alive

- Chat, web, and LME hosts stay mounted while any matching tab exists (hidden when inactive).
- Cheap singleton surface tabs mount only while active.
- Closing a chat tab does not delete the session.

## Consequences

- Rail / nest / open actions open or focus shell tabs; `desktopSurface` becomes a focus hint for the rail.
- Local LME and browser tab strips are demoted when the shell strip is present.
- Split views / multi-pane editor groups are explicitly deferred to a follow-up sprint.

## Non-goals (follow-up)

- Drag-to-split, group trees, moving tabs between groups
- Multi-chat grid composer UX
- Persisting split layout trees
