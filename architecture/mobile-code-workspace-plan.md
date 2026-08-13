# Mobile Code workspace

> **Status:** Proposed (locked 2026-08-12)
>
> **Scope:** Native/mobile-width Medousa Code, project files, editor, Terminal,
> Changes, and project-thread handoff
>
> **Related:** [Home Code workbench parity](home-code-vscode-parity-plan.md),
> [Code flow-state roadmap](code-flowstate-roadmap.md),
> [Code surface bridge](code-surface-bridge-plan.md),
> [Coding session terminal](coding-session-terminal.md), and
> [Mobile Code layout psychology](mobile-code-layout-psychology.md)

## Decision

Mobile Code is a dedicated project workspace, not the desktop Undertakings and
Code workbench compressed into a narrow viewport.

The workspace has four in-house rooms and one door out:

`Files` · `Editor` · `Terminal` · `Changes` — and **Thread** as a header door
to the existing project-bound Chat surface.

Files, Editor, Terminal, and Changes render in the mobile Code workspace as
mutually exclusive full-height rooms. Thread is not a fifth switcher job; it
leaves the house and returns to the same Code location. All five share one
Forge undertaking, worktree, human or agent lease context, open-file state,
Terminal sessions, and Review state. How you move through those rooms —
sibling switches vs jumps, chrome budget, landing, Files as a picker, Editor
as presence, Terminal vs Termius — is locked in
[Mobile Code layout psychology](mobile-code-layout-psychology.md).

## Product invariants

1. Filesystem and PTY authority follow the connected workshop daemon. Mobile
   never opens a local picker, reveals a Home-device path, uploads a worktree,
   or starts a local PTY for a remote workshop.
2. Forge retains project custody. Human edits, agent edits, Terminal changes,
   tasks, verification, Changes, and Review operate on the same governed working
   copy.
3. Mobile navigation changes presentation, not the project model. It reuses
   `undertakings`, `codeWorkspace`, `codeWorkbenchState`, LME/Forge APIs, and
   daemon-owned Terminal session ids instead of creating mobile-only copies of
   project data.
4. Project threads stay Chat threads. Mobile Code links to the bound thread; it
   does not embed or fork a second chat implementation.
5. A control is shown as available only when the corresponding Forge or workshop
   capability is usable. Repair and unavailable states use existing Medousa
   language and Settings → Packages paths.
6. The active shell theme owns application chrome. The active code syntax theme
   owns the editor canvas and language tokens. Terminal ANSI colors use a named
   Terminal palette contract; no mobile Code component introduces a parallel
   palette or hard-coded product theme.

## Why the current surface fails on mobile

`MobileCodePanel` currently switches from `LmeCodeExplorer` to the desktop-sized
`UndertakingsPanel`. The selected project then inherits nested desktop toolbars,
panels, menus, editor regions, and a short Terminal dock. In addition, the
project action that opens a Terminal activates a desktop shell tab even though
terminal tabs have no mobile destination.

The underlying stores and daemon contracts are useful. The narrow-screen
composition and navigation model are the parts to replace.

## Information architecture

### Level 1 — projects

The Code destination opens a project list that reuses the current undertaking
catalog, grouping, status, and create/continue behavior.

- Search and filters use mobile top-chrome actions and existing sheet patterns.
- Selecting a project enters its workspace at the last meaningful surface,
  unless attention should take the front door (below).
- Landing, in order: project has attention (dirty working copy, agent just
  finished, review available) → Changes; else an already-open file → Editor;
  else → Files (including a newly provisioned project).
- Project status, executor, attention, and dirty state use existing semantic
  status roles; technical lease/state vocabulary stays out of primary copy.

### Level 2 — project workspace

One project fills the available mobile content rectangle. A compact project
identity row shows the title, human phase, executor/working state, and an
overflow menu. The global `MobileTopChrome` remains the sole owner of the top
safe area.

The project workspace switcher exposes four sibling rooms. Thread is a header
/ `MobileTopChrome` door, not a fifth equal job. Review stays a decision taken
from Changes, not a sixth switcher item.

| Surface | Primary job | Default transition |
|---------|-------------|--------------------|
| Files | Find and open a project file | File tap → Editor (jump) |
| Editor | Read or edit one active buffer | Room switcher stays one tap away |
| Terminal | Use the project PTY | File link → Editor (jump) |
| Changes | Inspect changed files and diffs | Path/line tap → Editor (jump) |

Files, Editor, Terminal, and Changes are mutually exclusive full-height
surfaces. Mobile does not stack the tree beside the editor, dock a 13rem
Terminal below the editor, or place Changes in a desktop side panel. Editor
chrome does not duplicate room switches (Changes, Terminal, Files are not
editor toolbar buttons).

### Navigation and back behavior

Room switches and jumps are different. The surface switcher changes sibling
rooms and is not a push. Opening a file, a diff hunk, or a Terminal path is a
jump and *is* a push.

Hardware / back-swipe order:

1. A detail inside the active surface closes first (file picker, diff, search,
   overflow sheet, find UI).
2. A jump pops to its origin (Editor opened from Files, a hunk, or `gf`).
3. The project root returns to the project list.
4. The project list returns through normal mobile destination navigation.

A sibling room switch does not invent a fake “return to Files” unless Files was
the actual jump origin. Switching Editor → Terminal in the switcher returns to
Editor, not Files.

`registerMobileBackHandler` owns hardware/back-swipe parity. No mobile action
may create an activated desktop shell tab with no visible mobile destination.

## Surface contracts

### Files

- Full-width searchable project tree/list backed by the daemon source tree.
- Breadcrumbs collapse into a path button that opens an ancestor sheet.
- Rows show filename, compact parent context, language/file icon, dirty/open
  state, and changed-file status where known.
- Recent files and changed files are quick filters, not permanent sidebars.
  Default filter when the project has changes: Changed. Otherwise: Recent.
  The full tree is the fallback when the name is unknown. Files is a picker,
  not a place you live.
- Long press or overflow opens the existing capability-gated file actions.
- Selecting a file calls the shared `codeWorkspace.open` path, updates active
  undertaking selection, and jumps to Editor.

### Editor

- Reuses `CodeMirrorHost`, `codeWorkspace`, digest-fenced saves, durable drafts,
  conflict handling, editor preferences, language services, and the selected
  code syntax theme.
- Shows one editor canvas at a time. Open buffers live in a mobile file-switcher
  sheet ordered from `codeWorkspace.orderedTabsFor(workId)`.
- Compact chrome exposes back/forward location (jumplist), file switcher, save
  state, find, and an overflow menu. It does not duplicate room switches.
  Secondary desktop regions become full-screen surfaces or sheets rather than
  nested panes.
- Problems, outline, references, and language status open as mobile sheets and
  navigate back into the active editor.
- Selection-based agent actions use the same active-code context and project
  thread handoff as desktop.
- Keyboard appearance must not cover the selection, find controls, or save/error
  state. Refit through the existing visual-viewport utilities.

### Terminal

- Full-height xterm surface bound to the active project and worktree. The glass
  *is* the room: do not compete with Termius on hosts, keys, or SFTP; win on the
  same project PTY the agent is on. The `vim` test is the credibility test.
- Reuses daemon-owned `session_id` values and existing undertaking bindings.
- Provides a touch key row with Escape, Tab, Ctrl latch (visible state), arrows,
  Enter, paste, keyboard dismiss, and interrupt. Keys have at least the existing
  44px mobile target and use `haptic` consistently with other mobile controls.
- Font size is readable on a phone; desktop 12px is too small for a full-glass
  surface. Pinch-zoom is later, not an M3 blocker.
- Supports session switching and a new project shell through a mobile sheet.
  The sheet is this project’s shells, never a host inventory.
- File links open the shared Editor surface at the resolved path and line (a
  jump, `gf`).
- The project switcher hides while the software keyboard is active; the key row
  takes its place above the keyboard and respects the bottom safe area.
- Quiet “agent on this session” when a peer is attached — capability, not lease
  vocabulary.
- Temporary transport loss preserves the xterm buffer and disables input until
  the session host reports ready. It never silently queues commands for later
  execution.

### Changes

- Starts with a changed-file list grouped by the existing Changes/Forge model.
- Selecting a file opens a full-width mobile diff with the current DiffStack
  semantics, provenance, comments, restore/revert capability gates, and Review
  handoff.
- Path/line and diagnostic links jump to Editor without losing diff position.
- Review remains the decision surface. Changes is the inner-loop inspection and
  navigation surface; it does not invent a separate mobile commit model.

### Thread

- Lives on the project header / `MobileTopChrome` trailing actions (same
  pattern as Notes’ `noteChat`), not in the four-job surface switcher.
- Opens an existing project-bound Chat session when one exists or creates/binds
  one through the shared undertaking transition.
- Carries the active path, line, selection, diagnostics, and latest verification
  through the existing active-code context.
- Returning to Code restores the project, surface, active buffer, cursor, and
  scroll position.

## State ownership

Add a small presentation store, `mobileCodeWorkspaceState`, scoped by workshop
and `workId`. It owns only:

- selected project and active mobile surface;
- jump origin (Files, Changes hunk, or Terminal `gf`) vs sibling room switches;
- per-surface navigation/detail state;
- last Files directory/filter;
- active Changes file/detail;
- the visible Terminal session id and Ctrl-latch/input-bar UI state;
- the return target used by Thread handoff.

It does **not** own source contents, drafts, file digests, project projections,
leases, terminal bytes, diffs, chat messages, or task results. Those remain in
their existing authoritative stores/services.

Project restoration combines:

- `undertakings` for project and executor context;
- `codeWorkspace` for buffers, active file, recent/closed files, and persisted
  workshop state;
- `codeWorkbenchState` for shared navigation and contextual-region posture;
- `mobileCodeWorkspaceState` for narrow-screen presentation;
- Terminal attachment state for the live connection only.

Workshop switching clears live presentation and attachments through the same
reset boundary as the existing Code stores. Persisted mobile state is keyed by
workshop identity so project ids from two workshops cannot collide.

## Visual and interaction contract

Composition, chrome budget, landing, Files-as-picker, Editor presence, jump vs
room switch, and Termius-class Terminal feel are specified in
[Mobile Code layout psychology](mobile-code-layout-psychology.md). This section
keeps the token, primitive, and motion rules.

### Existing layout primitives to reuse

- `MobileShell` remains the 100dvh owner.
- `MobileTopChrome` remains the sole top-safe-area owner.
- `mobile-chrome-icon`, `mobile-icon-btn`, `mobile-sheet`,
  `mobile-sheet-header`, `mobile-you-scroll`, and the current mobile sheet
  gesture/back behavior are reused rather than restyled per surface.
- Bottom placement uses `--mobile-bottom-chrome-height`,
  `--mobile-keyboard-inset`, and `env(safe-area-inset-bottom)` through the
  existing viewport helpers.
- Primary mobile controls retain a minimum 44px hit target. Lucide supplies
  icons; controls keep accessible labels, visible focus treatment, and reduced
  motion support.

### Theme rules

Shell and workspace chrome consume semantic roles:

- surfaces: `bg-surface-*` or `--theme-canvas`, `--theme-pane`,
  `--theme-card`, and `--theme-chrome`;
- borders: `border-surface-*` or `--theme-border` with the existing alpha
  hierarchy;
- text: `text-content-*` or `--theme-text*`;
- focus, selection, and actions: `--theme-focus`, `--theme-selection`,
  `--theme-action`, and `--theme-link`;
- success, warning, and error: the existing semantic status roles.

No component branches on a named theme id. No new literal shell colors or
one-off radii are introduced when an existing semantic token or radius variable
fits. Light themes, familiar editor themes, workshop themes, and Medousa mark
themes must all remain legible.

CodeMirror continues to consume `codeSyntaxThemePreference`; mobile does not
translate syntax colors into shell theme roles.

Terminal receives a `terminalThemeFor(...)` adapter that maps the active shell
and/or code syntax palette into xterm roles (`background`, `foreground`, cursor,
selection, and ANSI 16). Terminal chrome itself uses shell semantic tokens.
The existing hard-coded `#0c0a09`/violet xterm palette is removed as part of the
Terminal slice. ANSI contrast is verified in both light and dark appearance.

### Motion and touch

- Surface switches use the short sibling transition language; jumps use the
  existing push/pop stack. Both honor `prefers-reduced-motion`.
- Scroll areas use `mobile-you-scroll`/momentum scrolling and preserve position
  per surface.
- Haptics are limited to navigational selection, destructive confirmation, and
  Terminal modifier/interrupt feedback; output arrival never triggers haptics.
- Hover-only affordances always have tap/long-press or overflow equivalents.

## Terminal transport completion

The dedicated Terminal surface is not considered usable until mobile transport
uses the authenticated workshop route rather than constructing a raw WebSocket
from `daemon_url`.

Required contract:

1. Output is available as a replayable, sequenced stream through the existing
   LAN/Iroh workshop transport.
2. Input is ordered and batched over the same authenticated route; resize and
   signal remain explicit control operations.
3. Attach reports a ready watermark before input is enabled.
4. Reconnect resumes with `after_sequence` and reports retained-history gaps.
5. Foreground resume and Wi-Fi/cellular handoff invalidate route caches and
   reattach without resetting the xterm buffer.
6. Shared-mode bearer authorization is covered by integration tests.

Desktop may retain direct WebSocket attach as a low-latency LAN fast path. Both
paths share protocol frame types and session semantics.

## Component boundary

Target composition (names may change without changing ownership):

```text
MobileCodePanel
├── MobileCodeProjects
└── MobileCodeWorkspace
    ├── MobileCodeProjectHeader
    ├── MobileCodeSurfaceSwitcher
    ├── MobileProjectFiles
    ├── MobileCodeEditor
    │   └── CodeMirrorHost
    ├── MobileProjectTerminal
    │   └── TerminalPane (mobile presentation contract)
    └── MobileProjectChanges
        └── shared diff/review components
```

`MobileCodeProjectHeader` / `MobileTopChrome` own the Thread door. The
four-job `MobileCodeSurfaceSwitcher` does not.

`UndertakingsPanel` and `CodeSourceEditor` remain desktop compositions. Shared
behavior should be extracted into stores, controllers, and small reusable
components; the mobile workspace must not accumulate viewport conditionals that
make either shell harder to reason about.

## Delivery slices

### M1 — workspace shell and Files

- Add mobile presentation state and project navigation stack.
- Replace selected-project `UndertakingsPanel` composition in `MobileCodePanel`.
- Ship project header, four-job surface switcher, Files list/tree, Changed/Recent
  default filters, and file open as a jump.
- Extend `MobileTopChrome` resolution/actions for Code project modes, including
  Thread as a header door.

**Acceptance:** On a paired phone, open Code → select a project → browse nested
files → open a file → back to the same Files position. No desktop shell tab or
side panel is required.

### M2 — focused Editor

- Compose mobile Editor around shared buffers and `CodeMirrorHost`.
- Add file-switcher, find/save, conflict, navigation, and language-status sheets.
- Restore active file, cursor, scroll, dirty draft, and syntax theme.

**Acceptance:** Edit and save two files, switch between them, background/resume,
and recover the same buffers without losing drafts or changing project custody.

### M3 — mobile Terminal surface

- Route every mobile Terminal action to the visible project Terminal surface.
- Add full-height layout, key row, session sheet, file-link handoff, keyboard
  geometry, and theme adapter.
- Remove the short mobile Terminal dock path.

**Acceptance:** Run commands, history, completion, Ctrl-C, paste, `vim`, and a
long-running process on iOS and Android; follow an output file link into Editor
and return to the same Terminal buffer.

### M4 — resilient authenticated transport

- Add replayable output and ordered input/control over LAN/Iroh.
- Add ready/sequence/gap handling, bounded reconnect, resume, and route handoff.
- Keep desktop WebSocket fast path compatible.

**Acceptance:** Continue one shell session through app background/resume and a
LAN → Iroh → LAN route change with no duplicated output, silent command replay,
or Shared-mode authorization failure.

### M5 — Changes and Thread continuity

- Add changed-file list and mobile diff detail.
- Complete Editor/Changes location handoffs and Review actions.
- Complete project Thread open/return state restoration.

**Acceptance:** Inspect a changed file, jump to its source, ask the bound project
thread about the active selection, return to Code, verify, and open Review with
project/file/Terminal state intact.

### M6 — polish, accessibility, and docs

- Verify every shell theme, code syntax theme, appearance mode, Dynamic Type/text
  zoom boundary, VoiceOver/TalkBack labels, focus order, reduced motion, safe
  areas, landscape, and hardware keyboard behavior.
- Add mobile Code tests and device smoke journeys.
- Update `docs/guides/` and `docs/README.md` only when the user-facing flow ships.

## Test matrix

| Area | Required evidence |
|------|-------------------|
| Navigation | Project list, each surface, hardware back, Thread return, restoration |
| Files | Remote tree, filters, changed/open state, deep path, errors |
| Editor | Open/switch/save/conflict, dirty resume, location links, light/dark syntax themes |
| Terminal | Input keys, IME/paste, resize, `vim`, interrupt, long output, session switch |
| Transport | LAN, Iroh, route handoff, bearer auth, sequence replay/gap, host restart |
| Changes | File list, diff, provenance, comments, revert gates, Editor jump |
| Themes | All shell themes × light/dark plus supported syntax and Terminal palettes |
| Mobile | iPhone safe areas, iPad narrow/full width, Android back/IME, landscape, hardware keyboard |

Unit and component tests cover state transitions, token/theme adapters, and
input encoding. Integration tests cover daemon/session protocol behavior. At
least one real iOS and one Android smoke journey are release evidence; viewport
emulation alone is insufficient for PTY keyboard and lifecycle behavior.

## Completion definition

Mobile Code is complete when an engineer can select a remote project, browse
files, edit and save, use a resilient project Terminal, inspect changes, and
continue the project thread without entering a desktop-compressed composition
or losing project context. Every acceptance journey must pass through the
workshop authority boundary and remain coherent across supported themes,
appearance modes, safe areas, keyboard states, and reconnects.

