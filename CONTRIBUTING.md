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

# Rust tests (same as CI hermetic lib job — runs the suite twice)
./scripts/ci/test-hermetic.sh

# Workspace lib tests
cargo test --workspace --exclude medousa-sdk-iroh --lib

# Home Tauri (Ubuntu PR job; leftover proxy helpers allowlisted — see H12)
cargo clippy --manifest-path apps/medousa-home/src-tauri/Cargo.toml --all-targets -- \
  -D warnings -A dead_code -A clippy::too_many_arguments -A clippy::large_enum_variant \
  -A clippy::ptr_arg -A private_interfaces
cargo test --manifest-path apps/medousa-home/src-tauri/Cargo.toml --lib

# Dependencies (H11)
cargo deny check
./scripts/ci/check-unused-deps.sh
./scripts/ci/check-dependency-budget.sh

# Desktop app — TypeScript + Svelte (must be 0 errors and 0 warnings) plus unit tests
cd apps/medousa-home && npm ci && npm run check && npm test && npm run build

# Docs
bash scripts/verify-docs.sh --strict
```

Optional: `bash scripts/ci/validate-workflows.sh` after editing `.github/workflows/`.
Optional micro-CI perf: `./scripts/ci/check-perf-budgets.sh`.

## Home runtime boundaries

Home startup is a product boundary ([ADR-020](docs/architecture/decisions/adr-020-feature-boundaries-and-lazy-runtime.md), [H09](architecture/hardening/09-home-runtime-boundaries.md)):

- Feature catalog/descriptors (`apps/medousa-home/src/lib/runtime/features/catalog.ts`) import no stores or Svelte implementations.
- Feature stores do not import sibling feature stores; cross-feature work goes through typed ports and shell orchestration.
- Destinations and overlays load with `import()` on user/restored-state intent. Do not use dynamic import to hide a cycle. Chat stays in the AppShell static graph.
- Tailwind compiles no palettes. Boot loads one stored `/themes/<name>.css` sheet. Feature CSS loads with its entry, not from `app.postcss`.
- Do not prefetch dormant features on boot.

From `apps/medousa-home`, CI already requires:

```bash
npm run check              # includes check:runtime-graph (0 first-party SCCs)
npm test                   # full Vitest suite
npm run test:h09           # leak, overlay freeze, SCC direction, CSS inventory, contrast
npm run build && npm run check:bundle-budget
```

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
   - `./scripts/ci/test-hermetic.sh` for the required engine lib suite
   - `cargo test --workspace --exclude medousa-sdk-iroh --lib` for workspace crates
   - `cd apps/medousa-home && npm run check && npm test` for the desktop app (0 errors, 0 warnings)
   - `cargo deny check` and `./scripts/ci/check-dependency-budget.sh` when touching Cargo.toml / lock
   - `cargo test` / targeted package tests for Rust changes
   - `scripts/verify-docs.sh --strict` when you edit `docs/`
   - App smoke for UI: open Settings → Packages, chat, vault as relevant
5. Do **not** commit secrets, `.env` files, or signing keys.

## Reporting bugs

- Security: follow [SECURITY.md](SECURITY.md) — private report only.
- Everything else: GitHub issues with OS, app/engine version, and steps to reproduce.

## Code of conduct

Be respectful. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## AI coding assistants

If you use Cursor or similar agents in this repo, start from [AGENTS.md](AGENTS.md).
