# Changelog

All notable changes to Medousa are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

### Changed

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

[0.3.2]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.2
[0.3.1]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.1
[0.3.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.3.0
[0.2.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.2.0
[0.1.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.1.0
