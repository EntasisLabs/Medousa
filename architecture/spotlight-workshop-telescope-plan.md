# Spotlight as a Workshop Telescope

## Outcome

Spotlight is a temporary, keyboard-first view into Medousa's live workshop. It
is not a generic command palette with an optional preview.

The stable frame answers four questions:

1. **Where?** Home/runtime or one of the named workshop desktops.
2. **What?** Tabs, tiled panes, recent work, notes, chats, or actions.
3. **What is it?** A category-owned preview that never collapses into an empty
   rectangle.
4. **What happens next?** Footer hints distinguish focusing existing work from
   opening or running something.

## Product model

- `Home` is the global/runtime scope. It contains suggested actions, runtime
  state, recents, and cross-workshop search.
- Every other scope tab is a real `ShellDesktop`. Desktop tabs are not search
  filters and must reflect the live desktop catalog.
- A desktop scope reads its real `ShellDesktopLayout`: split tree, pane groups,
  ordered tabs, and focused tab.
- Selecting an already-open tab focuses it in place. It does not create a
  duplicate.
- The existing `+`, `!`, and `>` prefixes remain expert accelerators in Home;
  they are no longer placeholder documentation.

## Stable shell

```text
Search this workshop…
Home | Desktop 1 | Desktop 2 | Desktop 3 | Desktop 4
─────────────────────────────────────────────────────
picker / categories       | adaptive preview
─────────────────────────────────────────────────────
keyboard contract         | ⌘K
```

- Workspace scopes use editor-style tabs with an active underline, not filter
  pills.
- The body keeps a Telescope-style picker/preview split at desktop widths.
- At compact widths the preview may collapse below the picker or be entered
  with `Tab`; the picker remains the primary keyboard surface.

## Preview contracts

Every selectable row owns a useful preview grammar:

| Selection | Preview |
| --- | --- |
| Desktop or tiled pane | Truthful miniature of the split tree, panes, and tabs |
| Open note | Rendered markdown using the existing markdown renderer |
| Open/recent chat | Compact transcript surface with recent-chat tabs |
| Code/file | Source excerpt, language/path metadata, or diff when available |
| Work item | Status, current activity, and related open work |
| Runtime/action command | Description, target, shortcut, and expected effect |
| Category/empty result | Category overview or helpful empty state |

Do not stack unrelated preview grammars merely to fill space. A layout map is
shown for a desktop/pane selection; a note selection gets the note renderer.

## Visual contract

- Consume semantic theme roles: `--theme-pane`, `--theme-card`,
  `--theme-border`, `--theme-text-*`, `--theme-selection`, `--theme-focus`, and
  `--theme-shadow`.
- Light mode is a designed translation, not an inverted dark surface.
- Use accent color for the active workspace underline, input focus, and active
  row wash only.
- Keep the panel calm: no gradients, strong glow, or glassmorphism.
- Primary rows are 12–13px, metadata is 11px, and the search prompt is 14–15px.
- Small glyphs communicate object type; they are not decoration.

## Keyboard contract

- `↑` / `↓`: move through picker rows.
- `←` / `→` while the picker owns focus: move through workspace scopes.
- `Tab`: enter an interactive preview when one exists.
- `Shift+Tab`: return to the picker.
- `Enter`: focus an existing object or execute the selected action.
- `Escape`: cancel a prompt step, then close Spotlight.
- Existing pin digits and prefix modes remain available in Home.

## Delivery slices

### Slice 1 — workshop shell

- Replace hard-coded palette colors with semantic theme roles.
- Add Home plus live desktop scope tabs.
- Add desktop-scoped Open Tabs and Tiled Windows groups.
- Focus existing tabs through `shellTabs.revealSearchHit`.
- Render a truthful miniature split layout for pane selections.
- Ensure every Home command has a text fallback preview.
- Add rendered note previews and recent-chat preview tabs.

### Slice 2 — richer recents and object previews

- Add stable recent-object records instead of inferring recency from array
  order.
- Add code/file, Work, browser, and runtime-specific preview presenters.
- Add preview focus traversal and action affordances.

### Slice 3 — compact and accessibility pass

- Define narrow-window behavior.
- Add tab/row ARIA semantics and screen-reader announcements.
- Verify contrast and selection visibility across every shipped theme pair.
- Add interaction tests for scope switching, focusing open tabs, prompt steps,
  and preview fallbacks.

## Acceptance criteria for Slice 1

- Dark and light Spotlight surfaces visibly belong to the active Medousa
  theme.
- Home and all current named desktops appear as scope tabs without mutating
  desktop state.
- A desktop scope lists its real tabs and panes, including inactive desktops.
- Opening a listed tab switches desktop if necessary and focuses the existing
  tab.
- Note, chat, workspace, and plain-command selections all show a non-empty,
  type-appropriate preview.
- Spotlight still supports command prefixes, prompt steps, pins, mouse
  selection, and keyboard execution.
