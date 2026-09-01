# Changelog

All notable changes to Medousa are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.10.0] — 2026-08-31

### Added

- **Embedded Personal on iOS and Android** — mobile builds can run the Medousa
  daemon on-device instead of acting only as a portal. Chat, Notes, Calendar,
  Automations, agents, flows, history, model routing, and workshop state remain
  available without pairing a desktop.
- **Mobile-first setup and navigation** — onboarding can connect a ChatGPT
  account or an API-key provider, the model picker searches providers and model
  ids, bottom sheets expand to a full-height detent, and Home, Calendar,
  Automations, Workshop, Code, Notes, and chat history share one calmer mobile
  interaction model.
- **Long-conversation navigation** — transcripts page older history on demand,
  preserve the reader's scroll anchor, and keep the contextually relevant user
  turn sticky while moving through earlier responses.
- **Durable conversation tools** — prompt stashes preserve unfinished text and
  attachments, committed history forks branch from an earlier turn, transcript
  search is indexed, and ordered conversation coordinates survive reconnects.
- **Hermes agent runtime** — Hermes joins Medousa, Codex, and Cursor as a
  first-class ACP runtime with readiness checks, setup guidance, model routing,
  and governed Coder worktree support.
- **MCP OAuth** — discover, authorize, refresh, revoke, and reconnect OAuth-backed
  MCP servers through the daemon-owned browser flow and credential registry.
- **Daemon-owned work environments** — OCI-backed jobs gain durable checkpoints,
  resumable remote delegation, target selection, parallel reconciliation, and
  federated result transport without moving custody out of Medousa.
- **Generated API contracts** — a declared route inventory now drives the
  OpenAPI schema, Rust and Python types, SDK clients, Tauri bridges, and
  versioned error envelopes from one contract surface.

### Changed

- **Bounded persistence ownership** — feed appends now use independent per-feed
  owners and incremental logs; workspace state uses a typed mutation journal and
  generation checkpoints; and Forge task runs use bounded per-run output/replay
  storage with explicit reconnect gaps and terminal eviction.
- **Bounded provider streaming** — upgraded to `stasis-rs` 0.9.0 and replaced
  the provider/tool-loop unbounded delta channel with awaited bounded admission.
  Slow consumers now backpressure providers, while closed receivers and
  oversized deltas fail visibly instead of being dropped or accumulating.
- **Chronological turn streaming** — native, SDK, TUI, embedded-daemon, and Home
  consumers now preserve interleaved prose, reasoning, tool activity, and
  completion order through a typed reconnectable stream.
- **Scoped runtime execution** — turn workers, tools, cancellation, continuations,
  and delegated waits carry exact workshop and generation ownership instead of
  relying on ambient shared state.
- **Atomic vault, Forge, and Coder storage** — generation-fenced mutations,
  crash-safe logs, compaction, bounded checkpoints, and explicit replay gaps
  replace full-map rewrites and unbounded retained output.
- **Desktop runtime boundaries** — destination code and CSS load on demand,
  lifecycle ownership is centralized, bundle budgets are enforced, and the
  workspace shell keeps inactive views mounted and sized correctly.
- **Code workbench** — projects open on one tap, tasks attach to terminal
  sessions, run output reconnects, tests are repository-root aware, and editor,
  feedback, and review commands share the same governed workspace.

### Fixed

- Mobile history and library sheets no longer lose their expanded detent or
  bounce back while swiping; search fields wait for an explicit tap before
  opening the keyboard.
- Contextual sticky user turns now advance through the whole loaded transcript
  instead of pinning only the literal latest user message.
- Embedded Personal restores workshop identity, credentials, turns, and feature
  state across app lifecycle transitions and recovers cleanly from interrupted
  startup.
- The desktop shell once again fills the viewport after the CSS/runtime split;
  inactive destinations render normally instead of leaving an empty pane.
- Mobile startup uses one splash, and mobile menus, popovers, code text, chat
  history, Notes chips, Calendar sheets, and action rows use consistent touch
  targets, spacing, and typography.

### Security

- Daemon access now fails closed: local and remote clients authenticate with
  workshop credentials, requests carry typed principals, and every protected
  route declares its required capability.
- Session, vault, artifact, media, Git, and Coder storage use typed identifiers
  and confined filesystem authority through final I/O, including Windows path
  semantics and durable deletion.
- Remote browser WebViews are isolated behind bounded native actions, hardened
  content-security policy, lifecycle fencing, and packaged-resource checks.
- Secrets are centralized in the platform credential store and scoped to the
  workshop that owns them; revocation is enforced across reconnects.

## [0.9.1] — 2026-08-12

### Fixed

- **Windows console spam** — workshop sidecars (`medousa-session`, `medousa-code`), language servers, and Forge script spawns no longer flash visible console windows. Closing those windows no longer kills the host before health succeeds.
- **Terminal / Review 503 on Windows** — shell-session reaps dead children, both sidecars wait longer for health, and timed-out hosts are cleared so the next request can respawn instead of sticky “health timed out” failures.
- **C# language server** — prefers `csharp-ls` when available, otherwise OmniSharp with `-lsp -z` and an attached `.sln` / `.csproj`; agent initialize now sends workspace folders so non-entry `.cs` files parse as project members instead of single-file “program” analysis.
- **Desktop browser width** — the in-app Web surface no longer overflows `100vw` / flex min-content past the shell. Native embed bounds clamp to the window and correct for content zoom so the right edge is not clipped by shell chrome.

## [0.9.0] — 2026-08-11

### Added

- **Conversation modes everywhere** — General and Coder modes, governed project selection, and project creation now follow daemon-owned conversations across Medousa, VS Code, and Neovim.
- **Account-backed agent routes** — connect ChatGPT for Medousa's native runtime or run Codex and Cursor as external agents inside the conversation's governed project worktree.
- **Code workbench depth** — repository search and replace, cross-file navigation and refactors, Problems and Structure views, streamed tasks, private preview URLs, Git changes, blame, restore, and review comments now share one project workspace.
- **Durable concurrent coding** — isolated attempt worktrees, checkpoints, bounded working memory, visible subagent evidence, resource coordination, and restart recovery keep parallel agents governable.
- **Portable Liquid Markdown** — the shared parser and browser renderer let first-party host adapters render Liquid content without importing Medousa's full UI.
- **Vision attachments** — account-backed chat turns can include bounded image attachments from the composer.
- **TUI workspace parity** — notes, links, syntax, scrollback, LAN browsing, note conflict handling, sealing, and terminal color support expanded the keyboard-first client.

### Changed

- The runtime and model controls now distinguish Medousa, Codex, Cursor, API-key providers, and the native ChatGPT account route without conflating their credentials or billing paths.
- First-party tool contracts now use typed runtime boundaries while preserving compatibility schemas and actionable validation errors.
- Mobile chat, navigation, library, pairing, settings, and home surfaces were aligned with the desktop workshop model.
- Companion integrations advance to **0.2.0**. VS Code and Obsidian render portable Liquid Markdown; VS Code and Neovim also share the expanded Coder/project workflow. Browser and Neovim continue to render their existing safe Markdown surfaces.

### Fixed

- Stream ownership and completion are deterministic, preventing duplicate assistant responses and premature turn completion.
- Code tabs, project events, language-server recovery, terminal PTYs, and the Home project selector recover cleanly across navigation and reconnects.
- Mobile turn ownership and recurring schedule behavior no longer drift during background or navigation transitions.

## [0.8.0] — 2026-08-02

### Added

- **Medousa Draw** — vault-native freehand drawing blocks in ordinary notes and full `kind: draw` canvas notes, stored as versioned vector scenes in Markdown.
- **Companion integrations** — initial 0.1.0 release bundles for VS Code, Neovim, Chromium browsers, and Obsidian, all connected to the same workshop sessions and context model.
- **Request-driven local inference** — device telemetry, benchmark calibration, admission envelopes, resource leases, GPU budgeting, and lifecycle eviction under sleep or memory pressure.
- **True multi-agent concurrency** — workshop turns can hand work to multiple agents without serializing unrelated execution.

### Changed

- Notes gained active-heading tracking, calmer outline navigation, improved scrolling, and richer chat handoff context.
- Settings now exposes the companion surfaces as first-class ways into Medousa.
- Release CI builds, tests, checksums, and publishes independently versioned integration bundles alongside the 0.8.0 full train.

### Fixed

- Companion clients release stale workshop connections during handoff and preserve session navigation and restored chat state.
- Neovim streaming handles daemon event payloads without deserialization failures.

## [0.7.2] — 2026-07-31

### Fixed

- **Empty and renamed Git bases** — repository inspection no longer treats an unborn branch name as a usable commit. Empty repositories receive an explicit initial-commit state, while projects whose selected base disappeared receive an actionable branch-selection error.
- **Forge persistence on Windows** — snapshot caches now use unique temporary files and atomic write-through replacement on Windows instead of opening directories as regular files, eliminating `Access is denied (os error 5)` during project creation and recovery.

## [0.7.1] — 2026-07-31

### Fixed

- **Windows project access** — repository discovery now recognizes mounted Windows drive roots, so projects under paths such as `C:\\...` can be opened outside the user profile without weakening canonical path containment.
- **Windows console flashing** — captured Git, project-check, and repository-provider subprocesses no longer create visible console windows. Interactive PTY terminals remain unchanged.
- **Vault Versions on Windows** — version history, restore, diff, selective saves, worktrees, and portable-Git extraction now share the hidden subprocess and canonical Vault path boundaries; duplicate status probes are coalesced across Home surfaces.

## [0.7.0] — 2026-07-31

### Added

- **Forge work lifecycle** — durable undertakings, governed worktrees, lease-fenced executor attempts, sealed evidence, human review, recovery, and explicit dispositions
- **Coding room** — repository discovery, fast file tree and editor, project intelligence through Detamu, source navigation, checks, tests, review, recovery, and optional provider handoff
- **Shared terminal** — workshop-owned PTY sessions with full VT rendering, resize, multiline input, TUI support, splits, and reconnectable shell tabs
- **Replaceable coding agents** — Cursor and Codex ACP executors bind to Forge work while Medousa retains custody, evidence, and session history
- **Workspace restoration** — desktops, panes, tabs, active documents, code drafts, and terminal sessions return after relaunch; snapshots are isolated per workshop

### Changed

- Code, terminal, and review now open as first-class workspace tabs inside the Medousa shell
- Code loading is interactive-first: files become usable before optional indexing and language intelligence start
- Code actions moved into a calm view action bar; line, language, save, and health information moved into the shared status bar
- Review was redesigned around changed files, evidence, risk, and a focused approval action instead of an infrastructure-heavy form
- OpenAI tool calls automatically use the Responses API when model reasoning and function tools require it

### Fixed

- Prevented language-server request storms and made Rust Analyzer opt-in for large workspaces, avoiding multi-gigabyte idle growth and OOM crashes
- Removed reactive event/effect loops that repeatedly reopened files, refreshed the bottom dock, froze scrolling, and exhausted WebView memory
- Fixed daemon CORS handling for the Home development origin and stopped retry loops from amplifying recoverable failures
- Made code-understanding readiness an explicit state instead of surfacing expected indexing delays as repeated 502 errors
- Fixed duplicate editor decorations, response-body decoding errors, stale service detection, and Home/CLI service restart ownership
- Deduplicated concurrent loose-file opens so one user action performs one disk read

- **Windows script save** — grapheme script path containment no longer fails on first save when the body file does not exist yet (`\\?\` / case / separator mismatches)
- **Script rename** — add Rename action in the scripts library and script workbench toolbar (notch tabs no longer host script rename)
- **Browser + notch** — when the shell-tab notch expands, hide the native browser embed (same overlay stack as Spotlight / rail popovers) so the fused drawer isn’t buried under the webview

## [0.6.1] — 2026-07-27

### Fixed

- **Windows idle CPU** — stop pre-creating hidden Home popout WebViews; sync WebView2 `IsVisible` on show/hide so Manager / Network / GPU processes stay quiet when popouts are closed
- Popout windows only attach workshop observers while visible; status pulse and peer polls no longer tick forever while idle / hidden

### Notes

- **Desktop-only cut** — bumps `desktop` to **0.6.1**; engine stays **0.6.0**. Ship with Actions → **Release** → `ship_desktop` only and leave **`reuse_r2_daemon`** on (default) so CI pulls `medousa_daemon` from the published `engine-v0.6.0-…` artifacts and skips the daemon compile matrix.

## [0.6.0] — 2026-07-26

### Added

- **Shared mode** — profiles as workshop seats (no login); pairing carries `profile_id`; dual session catalogs for personal vs shared
- **Peer mesh** — capability-scoped personal↔team messaging, inbox/outbox, receipts, grants, intros, and daemon introducer wiring (ADR-011)
- **Calendar reminders** — reminder composer, alarm helpers, and richer calendar actions / `.ics` flows
- **In-app updates** — check / download desktop shell updates from Settings → Workshop → App
- Standalone toolbar + pop-out window support; zoom / content-scale capabilities for mobile and desktop
- Vault **outline** navigation; Operator’s Guide expanded (product voice + Liquid examples)
- Context map absorbs actions and session metrics; improved physics / rendering / side rail
- Interactive canvas layout editing for custom surfaces / environments
- Work surface: reworked Ask flow; side rail filtered views
- Spotlight pass inspired by Harpoon / Telescope (relevance + discovery)
- ACP/MCP finish: remaining handlers, stream events, permission / pump polish

### Changed

- **Mobile shell rework** — quieter Home (greeting hero, workshop/status/peers meta row), destinations menu (custom views before More; Settings pinned last), settings pager with arrows through all sections, platform-aware host copy (no hardcoded “This Mac”)
- Mobile Notes, Automations / Scripts, and chrome interactions (top chrome actions, sheets, back-to-Home from More)
- Settings IA reorganized into Preferences / Medousa Agent / Runtime Controls / Sharing + machine group
- Shell tabs: notch orientation, drag-to-split, rename / search workspace tabs
- You / profiles language, keybindings, and wizard path polish
- Chat model picker and presence / routing copy for companion shells

### Fixed

- Timeline scroll rendering issues
- Markdown preview / Operator’s Guide raw rendering edge cases
- Mobile Home Vite loading via redirect; workshop open crash from missing Home date state
- Mobile settings section switching (no longer forced back to Preferences)

### Notes

- **0.6.0 ships `engine` + `desktop` only** — adapters / installer / mcp-gateway / local-brain remain on **0.4.1** channel artifacts (same pattern as 0.5.0). Do not push a full-train `v0.6.0` tag unless every package stamp is raised together.

## [0.5.0] — 2026-07-24

### Added

- **Versions** (optional, off by default): Git-backed vault history via Settings → Versions; Save version / History / Restore; Advanced Git worktrees + diffs
- Platform Git detect/install: Windows portable MinGit into data `bin/`; macOS Xcode CLT hint; Linux package-manager hint
- Liquid **snapshot** timeline (`layout: snapshot`) — horizontal track + synced peek carousel; Live dedicated surface
- Liquid **```feed```** fence — hydrate Stasis last-good result (`GET /v1/feeds/{id}/latest-good`) with datatypes `md|text|json|csv|image`
- Vault **Trash** restore UI (`GET /v1/vault/trash`, `POST /v1/vault/trash/restore`)
- Scripts **CodeEditorShell** extract (Grapheme first; plaintext/md/shell highlight stubs)
- **Virtual shell desktops** (up to 4): named pane-layout snapshots, status-bar strip, Spotlight workspace commands, `Ctrl+; 1–4` to switch
- Status bar: activity pulse, contextual vault/script whisper, automations `enabled/total`, desktop strip
- Rail popovers, shake-to-reveal / `Ctrl/Cmd+Shift+.` summon toolbar, per-surface rail lists & toolbars
- App titlebar: New Tab menu, per-tab back/forward history
- Chat presence empty state (time-of-day room title + centered composer)
- Work surface **Asks** panel
- Vault note property controls and tag addition

### Changed

- Shell persistence → `medousa-home-shell-tabs-v3` (migrates v1/v2 into one “Main” desktop)
- `Ctrl+; 1–4` switches virtual desktops (no longer focuses panes by index)
- Status bar overflow (`…`) removed in favor of first-class desktop strip + Automations control
- Side-rail / navigation IA and vault top bar reworked
- Stasis dependency bumped to **0.8.0**
- Release CI supports targeted per-package ships (`workflow_dispatch` `ship_*` checkboxes); `v*` tags remain full-train. Daemon builds once and is reused by desktop + engine packaging. Channel manifests merge so untouched packages keep prior versions/URLs (`scripts/release/package-versions.toml`). **0.5.0 ships `engine` + `desktop` only** — adapters / installer / mcp / local-brain remain 0.4.1.

### Fixed

- Creating a note with a title/path that already exists no longer overwrites disk or the editor buffer (frontend refuse + `POST /v1/vault/notes` create-only)
- Windows Build split: slash menu anchors immediately (fixed coords) and IME keyCode 229 no longer claims ↑↓/Enter (WebView2 input deadlock)
- Windows focus loss (Greenshot / snipping tools): hard-dismiss slash + context menus; skip clipboard while unfocused; release split sash pointer capture
- Vault expand/collapse bugs around nested folders

## [0.4.1] — 2026-07-22

### Added

- Live heading / list fold (Obsidian-style chevrons; session-local)
- Collapsible GFM tables and `medousa-view` hosts in Live
- Optional Live toggle to hide heading `#` / `##` marks (no layout shift)
- Paper width presets for Live / Preview (narrow → full)
- Width controls for compare, slides, and Live tables (`width:` fence KV where applicable)
- Syntax highlighting for C, C++, C#, Java, PHP, R, Scala (plus common aliases)
- Workbook marker surface (title + sheet list) with View / Raw toggle

### Changed

- External / loose markdown files use absolute-path note buffers + editor UI restore so Live keep-alive / multi-pane no longer mounts blank
- Loose-file leave flush can autosave via absolute write; export Word/PDF available for loose notes
- Local images beside an absolute note resolve next to the file, not the vault root
- Kind pill for sheet / ledger / workbook / slides / board seeds object body and opens table/deck/board/manifest (not empty Live)

### Fixed

- Preview callout icon / title alignment (no longer fights `.markdown-content p` margins)
- Copy CSV on query views no longer opens Configure (click priority)
- `contentSyncKey` path parsing keeps absolute OS paths intact
- Sheet / workbook / slides kind no longer snaps back to Note after buffer restore (frontmatter kind wins)
- Sheet View/Raw toggle available (was ledger-only)

## [0.4.0] — 2026-07-21

### Added

- Liquid `block` fences — typography containers (font, size, align, spacing) with Obsidian-style trailing `^block-id` round-trip
- Redesigned Live selection format bubble — Shape / Voice pages, paragraph + heading menu, Build-style color wheel + hex apply
- Content zoom (`⌘`/`Ctrl` `+` / `-` / `0`) for notes, chats, and scripts
- Markdown footnotes in Live and Preview (definitions + refs)
- Callout visual refresh with shared icon / token styling
- Syntax highlighting for fenced code snippets
- Daemon agents surface + ACP client wiring so Home chat can talk to external agent runtimes
- Workbooks foundation and improved slides player for vault decks

### Changed

- Vault new-note creation flow
- Side rail interactions and vault filtering polish
- Styled-block chrome uses Type / Layout doors instead of dense chip rows

### Fixed

- Live editor no longer jumps scroll on typing or format actions
- Styled blocks update in place without remount / layout jumps that fight the viewport
- Editor race condition and menu serialization under rapid Live interactions

## [0.3.2] — 2026-07-21

### Fixed

- Creating a note no longer wipes / retitles the previously active note (cold-open write-lease handoff)
- Live editor remount / destroy flushes no longer clobber another note during tab switches or paste storms
- Empty / frontmatter-only notes no longer freeze the app when entering Live or side-preview edit
- Opening external markdown files binds an LME tab and renders content (absolute paths no longer vault-normalized away)
- Liquid `compare` fences with duplicate axis or entity labels no longer abort the rest of the preview
- Embed write-through and foreign undo go through the versioned per-path save queue
- Vault editor context-menu cut/copy/paste no longer hangs the shell on Windows (clipboard timeouts + menu portal)
- Slash menu no longer freezes Live/Build on Windows (IME key guards, hard dismiss, deferred serialize, BodyPortal)

### Changed

- Vault open / save coordination uses generation fencing, path-scoped dirty, and quiescent leave-flush before lease transfer

## [0.3.1] — 2026-07-20

### Added

- LME schedule tabs — open a schedule in the workspace with a calm detail editor (Runs / Deliveries / Pause)
- Progressive **New schedule** popover — title, prompt, natural-language when; frequency → time → timezone on demand
- Vault editor right-click context menu (cut / copy / paste / select all and related actions)
- Stronger note buffer / save-queue persistence so open notes survive tab and rail navigation

### Changed

- Quieter LME rails to match a Jobs-cut workbench: agents, flows, schedules, and history
- History: hover-reveal More / Flow, cardless expand, liquid selection dock, inline dock search (same pattern as Files / Scripts / Decks)
- Schedules: human titles and one health line; machine cron / ids under Details; create no longer takes over the rail
- Flows and specialists: calmer titlebars and liquid forms instead of loud workshop chrome
- Shell tab strips — hover reveal with safer hit targets so titlebar actions stay clickable

### Fixed

- Live kanban boards not dragging after surface updates
- Shell tabs hiding incorrectly when the pointer was over the active view
- Note / editor state lost when switching tabs or navigating the side rail
- Editor truncation glitches in LME script and vault surfaces
- Script editor action buttons and tab overlays stealing clicks
- Schedule / split-rail boundary issues so pane chrome and titlebar controls don’t fight

## [0.3.0] — 2026-07-20

### Added

- Shell tabs with split panes — tile Workspace / Chat / Web side by side; multi-live chat transcripts per pane
- Per-pane note buffers so background Workspace panes keep their open note without fighting the focused editor
- Quieter chat composer — `+` menu for Attach / Profile / Agent, optional non-default chips, anchored popovers
- Expandable chat error details (`View details`) when the daemon provides a debug message
- Onboarding model picker shares Settings’ catalog → live → default resolution
- Paste / drop images into vault notes as inline `data:image/…;base64,…` (Live + Build + Preview)
- Slash menu starters for the full Liquid fence catalog (carousel, actions, section, chips, media, cite, compare, plan, timeline, shortlist, decision, brief, …)
- `scripts/install-app.sh` — curlable desktop installer (reads CDN `installer-bootstrap.json`, verifies checksum, opens the right artifact)

### Changed

- Workspace nav icon is now panels (tiling workspace); book icon stays for Notes / vault library affordances
- Profile and Agent manage links from the composer open the correct desktop shell tabs / mobile More destinations

### Fixed

- Live image paste no longer inserts escaped markdown text; clipboard `File` is captured synchronously so data URLs stay valid

## [0.2.0] — 2026-07-15

### Added

- Liquid markdown Blume-aligned embeds: `tabs`, `steps`, `accordion`, `code`, and `tree` (plus stagger enter animations)
- Open a single markdown file without adding a vault root (loose-file mode)
- Obsidian vault support on co-located workshops — detect `.obsidian`, safer scans, no auto workshop tags on external roots
- Dual-pane vault editing with bidirectional scroll sync between source and live preview
- Platform-aware shortcut hints (`⌘` on macOS, `Ctrl` elsewhere)

### Fixed

- Vault Tab key indents markdown instead of moving focus
- Vault YAML frontmatter no longer grows blank lines on every save (TS + Rust)
- Chat session search autofocuses and coalesces in-flight refreshes so typing is not dropped
- Artifact MedousaStore persists across chat embed revisions via stable store scopes + alias rebind
- Calendar `.ics` import generates missing UIDs, maps Outlook/Windows TZIDs, and surfaces import stats

### Changed

- Chat session search placeholder clarifies title/preview search (“Search titles…”)

## [0.1.0] — 2026-07-14

### Added

- **Medousa Home** desktop app (Mac / Windows / Linux) with Chat, Vault/Library, Web, Automations, Capabilities, Peers, Messaging, Context/Identity, Settings, and phone pairing
- Local **engine / daemon** with durable turns, host ↔ workshop lanes, memory & identity, vault, artifacts/presentations, environment canvas
- **Calendar** — personal RFC 5545 `.ics` store (`calendar/personal.ics`), Home Calendar surface, HTTP + SDK API, and `cognition_calendar_*` agent tools
- **Packages** — install optional binaries (offline brain, adapters, CLI, MCP gateway) from Settings without opening the Installer first
- Shared tarball install path in `medousa-install-support` used by Home and Installer
- Home resolves optional binaries from `{dataDir}/bin` after the app-bundle sibling
- End-user guides under `docs/guides/` (getting started, packages, workshop, phone, memory, channels)
- Dual MIT / Apache-2.0 licensing and community docs (`CONTRIBUTING`, `SECURITY`, `CODE_OF_CONDUCT`, `AGENTS.md`)
- Remote file authority — vault filesystem affordances gated to co-located workshops; daemon-served vault file previews when remote
- Windows daemon spawn hides console window (`CREATE_NO_WINDOW` + release `windows_subsystem`)
- Liquid markdown / interactive chat embeds, chart widgets, sandbox shell for Grapheme, and packaging/release CI (R2 + GitHub Releases)

### Changed

- Product path is **Home-first**: download the app, chat, then add packages from Settings; Installer remains an advanced/repair escape hatch
- Connection → Extras and welcome-wizard offline CTAs open Settings → Packages instead of launching the Installer by default

[Unreleased]: https://github.com/EntasisLabs/Medousa/compare/v0.10.0...HEAD
[0.10.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.10.0
[0.9.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.9.1
[0.9.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.9.0
[0.8.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.8.0
[0.6.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.6.0
[0.5.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.5.0
[0.4.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.4.1
[0.4.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.4.0
[0.3.2]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.2
[0.3.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.1
[0.3.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.0
[0.2.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.2.0
[0.1.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.1.0
