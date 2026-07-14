# Changelog

All notable changes to Medousa are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

### Changed

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

[0.1.0]: https://github.com/EntasisLabs/Medousa/releases/tag/v0.1.0
