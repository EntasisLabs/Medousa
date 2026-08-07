# TUI ↔ Home workspace parity

> **Status:** Proposed living plan  
> **Date:** 2026-08-07  
> **Related:** [component-tui.md](component-tui.md), [coding-session-terminal.md](coding-session-terminal.md),
> [code-surface-bridge-plan.md](code-surface-bridge-plan.md), [medousa-anywhere-plan.md](medousa-anywhere-plan.md),
> [ADR-012](../docs/architecture/decisions/adr-012-medousa-anywhere-surfaces.md),
> [tui-performance-target-plan.md](tui-performance-target-plan.md)

## Product thesis

**Home and the TUI are sibling workshop shells over the same daemon.**  
Anywhere ([medousa-anywhere-plan.md](medousa-anywhere-plan.md)) correctly refuses to
reproduce Home inside VS Code / Neovim / Obsidian. That non-goal does **not**
apply to `medousa_tui`: the TUI is already a first-class Medousa workspace
(chat, scripts, observability, settings). The gap is that Home grew a
tmux-shaped multi-surface desk (notes / code / review / chat / terminal panes)
while the TUI stayed a single-primary chat + overlays product.

Goal: give terminal operators the **same work shapes** Home offers — chat,
notes, code, review, and tmux-like tiling/grouping — without porting gen-UI,
canvas layout edit, or a full web browser.

```text
                    +-----------------------------+
                    |      medousa_daemon         |
                    |  interactive / vault / forge|
                    |  code/lsp / shell-sessions  |
                    +--------------+--------------+
                                   |
              +--------------------+--------------------+
              |                    |                    |
       +------v------+      +------v------+      +------v------+
       | Medousa Home|      | medousa_tui |      | Anywhere    |
       | (GUI shell) |      | (TUI shell) |      | (host plugs)|
       | WM + chrome |      | WM + chrome |      | focused win |
       +-------------+      +-------------+      +-------------+
```

Home remains the richest surface. The TUI aims for **workspace parity**, not
pixel or feature completeness.

## What Home already owns that the TUI should reuse (conceptually)

| Home capability | Home anchor | Engine substrate (already shared) | TUI today |
|-----------------|-------------|-------------------------------------|-----------|
| Chat + multi-live streams | `chat.svelte.ts`, `chatStreamPool` (≤4) | `POST /v1/interactive/turn` + SSE, sessions | Strong single-conversation + slash plane; not pane-local |
| Notes / Library | `vault.svelte.ts`, `VaultEditor` | `/v1/vault/*` (+ CLI `medousa vault`) | No Library surface (script editor only) |
| Code buffers | `codeWorkspace`, CodeMirror desk | Forge + `/v1/code/lsp`, coding tools | No Forge desk |
| Review | `ForgeReviewSurface`, `DiffStack` | `/v1/forge/*` review/seal/disposition | No Review surface |
| Terminal tabs | `TerminalPane`, Tauri VT bridge | `medousa-session` `/v1/sessions/shell*` | No attachable PTY panes |
| Tmux-like WM | `shellTabs.svelte.ts`, `shellSplitTree.ts` | **Client-only** layout | Overlay modes, not a split tree |
| Workshop switch | Connection / workshops store | local / portal / paired daemon | `--daemon-url` (+ local runtime fallback) |

**Do not reuse:** Svelte chrome, CodeMirror/Liquid live fences, canvas layout
edit, gen-UI / custom environment views, human browser tabs, companion pet,
Spotlight visual language. Reuse **contracts, caps, and interaction character**.

## Non-goals

- Gen-UI, Liquid scene player, environment canvas, layout-edit mode.
- Full web browser as a first-class pane (see [Browser research](#browser-research-deferred)).
- Rebuilding structured note kinds (kanban boards, slides decks, draw) as rich
  TUI widgets — markdown + frontmatter + optional read-only projections first.
- Nesting real `tmux` inside Medousa panes (Home rule: **splits = sessions /
  buffers**, not one PTY multiplexed — [coding-session-terminal.md](coding-session-terminal.md)).
- Making Anywhere plugins feature-complete with Home (unchanged).
- Blocking TUI work on a shared TypeScript client — TUI stays Rust / ratatui
  against daemon HTTP (and Rust SDK where it already helps).

## Design principles

1. **Daemon authority.** Vault FS, Forge worktrees, PTYs, LSP, and turns always
   resolve on the active workshop — same as Home remote rules.
2. **One WM model.** Port Home’s binary split tree + tab kinds + caps into a
   shared Rust module the TUI owns (and Home may later call via docs/contract
   tests). Caps stay aligned: **≤4 panes**, **≤4 desktops**, bounded tabs.
3. **Pane-local work.** Each leaf holds a tab strip; focused leaf owns keyboard
   routing (chat composer vs editor vs review vs PTY).
4. **tmux muscle memory.** Prefix chord (Home uses `Ctrl+;`) then
   `%` / `"` / `hjkl` / `z` / `x`, plus digit desktop switch — document TUI
   mapping next to Home’s catalog so operators can switch shells without
   relearning.
5. **Progressive depth.** Chat multi-pane first → notes → code/review →
   terminal attach. Each phase ships usable alone.
6. **Honest terminal UX.** Prefer keyboard density, status lines, and stacked
   diffs over fake cards or dashboard chrome.

## Target tab kinds (v1)

Mirror Home’s useful subset:

| Kind | Role in TUI | Backing API |
|------|-------------|-------------|
| `chat` | Session transcript + composer; up to 4 live streams | interactive + sessions |
| `notes` | Vault tree / search / markdown buffer | `/v1/vault/*` |
| `code` | Worktree file buffer (syntax + basic edit) | Forge + optional `/v1/code/lsp` |
| `review` | Stacked unified diffs + approve/finish actions | Forge review endpoints |
| `terminal` | Attached shell session (cell grid) | `/v1/sessions/shell*` |
| `ops` (keep) | Existing observability / thinking / grapheme overlays | current TUI |

Script editor (`editor_runtime.rs`) folds into `code` or a `scripts` tab later;
do not leave a parallel orphan editor forever.

## Architecture slices

### Phase 0 — Shared workspace model (foundation)

Extract (or newly define in Rust, mirrored from Home) a pure layout crate/module:

- `SplitNode` = `Group { id }` | `Branch { direction, ratio, a, b }`
- ops: `split_leaf`, `remove_leaf`, `neighbor`, `zoom`, `set_ratio`, leaf order
- `WorkspaceSession`: desktops → split root → groups → tabs
- persist: `tui_workspace_session_v1` under the engine data dir (or
  `GET/PUT` beside `tui_defaults` if we want Home Settings symmetry)
- property / unit tests that encode the same caps and merge-on-close rules as
  `shellSplitTree.ts`

**Acceptance:** layout module tested without ratatui; no UI behavior change yet.

### Phase 1 — Multi-pane shell + multi-chat

Replace “one conversation + modal overlays” as the only spatial model with:

- render split tree into ratatui `Layout` regions
- each leaf: tab bar + content + status line
- focused pane: chat already works; allow second/third chat tabs bound to
  distinct sessions (Home’s stream pool semantics)
- prefix keymap for split / focus / zoom / close / desktop
- keep existing overlays (settings, history, palette) as modal layers above WM

**Reuse from Home:** caps, keymap intent, “four live streams” mental model.  
**Reuse from TUI:** `event_reducer`, daemon-primary turns, slash commands
(scoped to focused chat pane).

**Acceptance:** operator can `Ctrl+;` `%` split, run two sessions side by side,
zoom one pane, restart and restore layout.

### Phase 2 — Notes (Library-lite)

- left rail or picker: vault tree + search (`/v1/vault/notes`, `/search`)
- buffer: markdown edit with dirty flag, `If-Match` / conflict notice
- quick open (Home Quick Switcher analogue)
- “ask about note” = open/focus chat pane with note path as context envelope
- backlinks/tags as list panels, not graph chrome

Generalize patterns from `editor_runtime.rs` (dirty, save path, status) but
**write through vault HTTP**, never bypassing daemon authority on remote
workshops.

**Acceptance:** browse → open → edit → save note on local and paired daemon;
ask-about-note hands context to a chat pane.

### Phase 3 — Code + Review

- bind a Forge undertaking / worktree (list + select)
- `code` tab: file tree under worktree + buffer; syntax highlight via existing
  packs / tree-sitter where cheap; LSP diagnostics as list (full CM parity out
  of scope)
- `review` tab: DiffStack semantics in the terminal — stacked file hunks,
  collapse unmodified, aggregate +/−, provenance line (human / agent /
  terminal)
- dispositions: approve / request changes / finish via Forge HTTP (same as Home)

Optional later: wire coding-domain tools when a code/review pane is focused
(same opt-in story as Home).

**Acceptance:** open work → edit file → see agent/human diff in Review → seal
path works without opening Home.

### Phase 4 — Terminal panes

- tab kind `terminal` attaches to `medousa-session` (create or attach-existing)
- split terminal = **new session** (Home rule), optional Forge `work_id` cwd
- VT: prefer pure-Rust `vte` cell grid (same lineage as Home’s Tauri bridge) so
  TUI does not need Zig / libghostty
- resize frames on pane ratio changes

**Acceptance:** two terminal panes + one chat pane; agent shell tools can target
the same session ids the TUI attached.

### Phase 5 — Workshop connection polish

- TUI workshop picker aligned with Home Connection (local / portal / paired)
- persist workspace session **scoped per workshop id** (Home v4 pattern)
- document handoff: same session id opens in Home chat without fork

## What to lift from Home vs rewrite

| Lift / mirror | Rewrite for terminal |
|---------------|----------------------|
| Split tree algebra + caps | ratatui layout + focus ring |
| Tab kind taxonomy | pane content widgets |
| Chat stream event semantics | existing TUI `TuiEvent` reducer (extend pane id) |
| Vault / Forge / shell HTTP contracts | list/buffer/diff widgets |
| Review provenance model | unified-diff pager UX |
| Keymap *intent* (prefix + verbs) | crossterm bindings + help overlay |

Do **not** share Svelte stores. Prefer a small Rust module under
`src/tui/workspace/` (or `crates/medousa-workspace-shell` if Home later wants
WASM/TS tests against the same golden vectors). Contract tests that both
`shellSplitTree.ts` and the Rust module pass against the same JSON fixtures are
the cheap way to keep shells aligned.

## Browser research (deferred)

Out of critical path. If a web pane is ever wanted in-terminal, evaluate as an
**optional adapter**, not a Home WebView port:

| Approach | Notes |
|----------|-------|
| Host `$BROWSER` / `lynx` / `w3m` in a `terminal` pane | Zero Medousa chrome; good enough for docs |
| [carbonyl](https://github.com/fathyb/carbonyl) / similar Chromium-in-terminal | Heavy dependency; graphics fidelity varies |
| Agent browser host tools | Already a daemon/agent concern — keep browse *actions* there |

Decision gate: only after Phases 1–4 feel like a daily driver.

## Mapping to existing code

| Area | Start here |
|------|------------|
| TUI binary / state | `src/bin/medousa_tui.rs`, `src/bin/medousa_tui/*` |
| TUI shared helpers | `src/tui/*` |
| Script editor patterns | `src/bin/medousa_tui/editor_runtime.rs` |
| Home WM | `apps/medousa-home/src/lib/stores/shellTabs.svelte.ts` |
| Home split algebra | `apps/medousa-home/src/lib/utils/shellSplitTree.ts` |
| Home tab types | `apps/medousa-home/src/lib/types/shellTabs.ts` |
| Vault CLI (API usage samples) | `src/bin/medousa.rs` (`run_vault`) |
| Shell sessions | `architecture/coding-session-terminal.md` |
| Review product rules | `architecture/code-surface-bridge-plan.md` |

## Delivery order (recommended)

1. **P0** workspace model + tests  
2. **P1** multi-pane + multi-chat (highest “feels like Home” payoff)  
3. **P2** notes  
4. **P3** code + review (can parallelize after P1 if staffed)  
5. **P4** terminal attach  
6. **P5** workshop-scoped persistence / connection UX  

Perf work in [tui-performance-target-plan.md](tui-performance-target-plan.md)
stays in force: pane multiply must not reintroduce blocking slash/network paths
or per-frame markdown rebuilds across every leaf.

## Success metric

A headless / SSH operator can run a day of Medousa work — **chat with an agent,
edit a vault note, review a Forge diff, and keep a worktree shell open** — in
one `medousa tui` process with tiling that feels like Home’s desk, without
needing the GUI except for gen-UI, canvas, or rich media.
