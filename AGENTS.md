# AGENTS.md — guidance for coding agents

This file is for **AI coding assistants** (Cursor, Codex, etc.) working in the
Medousa monorepo. Humans: see [CONTRIBUTING.md](CONTRIBUTING.md).

## Product model (do not invent a different one)

- **Filesystem / vault authority follows the workshop daemon.** Home is a window
  into the daemon’s disk when remote; local folder pickers / Reveal /
  `convertFileSrc` only when `workshop.kind === "local"` (co-located).
- **Home-first install.** Users download the Medousa app, chat immediately;
  optional binaries come from **Settings → Packages**. Do not turn “link a vault”
  into an upload pipeline. Medousa Installer is advanced/repair, not the happy path.
- **User-facing product name is Medousa** (not “Medousa Home” in copy). Engine =
  `medousa_daemon`. Settings UI label **Connection** (store id may still be
  `basement`).

## Repo map

| Path | Role |
|------|------|
| `apps/medousa-home/` | Tauri + Svelte desktop/mobile shell |
| `apps/medousa-installer/` | Optional full workloads installer |
| `src/` + `crates/` | Engine, SDK, install-support, types |
| `docs/` | Integrator + end-user guides (canonical for behavior docs) |
| `architecture/` | Living plans / ADRs — not user-facing truth |
| `docs/guides/` | Normie how-tos (getting started, packages, …) |

## Docs rules

- Code wins when docs disagree — fix the docs.
- New user-facing flows → add/update `docs/guides/` and index in `docs/README.md`.
- Integrator HTTP/SDK changes → `docs/engine/`, `docs/sdk/`, and
  `scripts/verify-docs.sh` / SDK contract as needed.
- Do not edit `.cursor/plans/*.plan.md` unless the user asks.
- Prefer not to invent markdown the user didn’t ask for outside this OSS/docs work.

## Engineering habits

- Match existing style; minimal diffs; no drive-by refactors.
- Never commit secrets, force-push main, or skip hooks unless asked.
- Windows background spawns: use `CREATE_NO_WINDOW` via `detach_new_session`.
- Optional binaries: install to `{dataDir}/bin`; resolve after sibling of
  `current_exe` (daemon stays bundled sidecar-first).

## Useful entry points

- Packages UI: `apps/medousa-home/src/lib/components/settings/SettingsPackagesSection.svelte`
- Packages backend: `apps/medousa-home/src-tauri/src/packages.rs`
- Co-location: `apps/medousa-home/src/lib/utils/workshopLocality.ts`
- Install shared: `crates/medousa-install-support/`
