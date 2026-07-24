# Changelog

All notable changes to Medousa are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[0.5.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.5.0
[0.4.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.4.1
[0.4.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.4.0
[0.3.2]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.2
[0.3.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.1
[0.3.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.0
[0.2.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.2.0
[0.1.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.1.0
