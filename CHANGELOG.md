# Changelog

All notable changes to Medousa are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Settings → **Packages** — install optional binaries (offline brain, adapters, CLI, MCP gateway) from the app without opening Medousa Installer first
- Shared tarball install path in `medousa-install-support` used by Home and Installer
- Home resolves optional binaries from `{dataDir}/bin` after the app-bundle sibling
- Dual MIT / Apache-2.0 licensing and community docs (`CONTRIBUTING`, `SECURITY`, `CODE_OF_CONDUCT`, `AGENTS.md`)
- End-user guides under `docs/guides/` (getting started, packages, workshop, phone, memory, channels)
- Remote file authority — vault filesystem affordances gated to co-located workshops; daemon-served vault file previews when remote
- Windows daemon spawn hides console window (`CREATE_NO_WINDOW` + release `windows_subsystem`)

### Changed

- Product path is **Home-first**: download the app, chat, then add packages from Settings; Installer remains an advanced/repair escape hatch
- Connection → Extras and welcome-wizard offline CTAs open Settings → Packages instead of launching the Installer by default

## [0.1.0] — TBD

Initial public documentation baseline. Tagged release notes will land here when
the first public `v*` ships with desktop + engine artifacts.
