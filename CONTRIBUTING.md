# Contributing to Medousa

Thanks for helping. Medousa is a permanent AI workspace — the app (Medousa), the
engine (`medousa_daemon`), SDKs, and docs. Pick the lane that matches your change.

## License

By contributing, you agree that your contributions are dual-licensed under
**MIT OR Apache-2.0**, the same as the rest of the project. See [LICENSE](LICENSE).

## Before you start

1. Read the [product README](README.md) for the user-facing story.
2. Skim [docs/README.md](docs/README.md) for integrator/operator docs.
3. For engine internals and epics, see [architecture/README.md](architecture/README.md)
   and the [roadmap](architecture/ROADMAP.md).
4. Documentation conventions live in [docs/CONTRIBUTING-DOCS.md](docs/CONTRIBUTING-DOCS.md).

## Development quick start

```bash
# Engine
cargo build -p medousa --bin medousa_daemon

# Desktop app (from apps/medousa-home)
npm install
npm run tauri:dev
```

More: [docs/cookbook/build-from-source.md](docs/cookbook/build-from-source.md) ·
[apps/medousa-home/README.md](apps/medousa-home/README.md)

## Lint (matches CI)

Run these from the repo root (`Medousa/`) before opening a PR:

```bash
# Rust — workspace clippy, warnings denied (medousa-sdk-iroh excluded until its SSE feature wiring is stable)
cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings

# Rust tests (same as CI rust job)
cargo test -p medousa --lib

# Desktop app — TypeScript + Svelte (must be 0 errors and 0 warnings)
cd apps/medousa-home && npm ci && npm run check
```

Optional: `bash scripts/ci/validate-workflows.sh` after editing `.github/workflows/`.

## What to work on

| Area | Good first contributions |
|------|--------------------------|
| Docs | User guides under `docs/guides/`, cookbook fixes, indexing orphaned pages |
| App | UI polish in `apps/medousa-home`, Settings copy, accessibility |
| Engine | HTTP routes + matching `docs/engine/` updates, SDK contract rows |
| Tests | Doctor probes, SDK contract checks, focused unit tests |

Avoid drive-by refactors of the turn spine or workshop transport unless an issue
asks for it — those surfaces are sensitive.

## Pull requests

1. Keep PRs focused (one story per PR when you can).
2. Match existing style in the files you touch.
3. Update docs when behavior changes (see the
   [docs release checklist](docs/CONTRIBUTING-DOCS.md#per-release-checklist)).
4. Run what you can locally:
   - `cargo clippy --workspace --all-targets --exclude medousa-sdk-iroh -- -D warnings` for Rust
   - `cd apps/medousa-home && npm run check` for the desktop app (0 errors, 0 warnings)
   - `cargo test` / targeted package tests for Rust changes
   - `scripts/verify-docs.sh` when you edit `docs/`
   - App smoke for UI: open Settings → Packages, chat, vault as relevant
5. Do **not** commit secrets, `.env` files, or signing keys.

## Reporting bugs

- Security: follow [SECURITY.md](SECURITY.md) — private report only.
- Everything else: GitHub issues with OS, app/engine version, and steps to reproduce.

## Code of conduct

Be respectful. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## AI coding assistants

If you use Cursor or similar agents in this repo, start from [AGENTS.md](AGENTS.md).
