# AGENTS.md

## Cursor Cloud specific instructions

This repo is **Medousa**: a Rust workspace at the root (the **engine**) plus a Tauri + SvelteKit
desktop/mobile app under `apps/medousa-home/` (the **client**). The engine is the product; the app
is a thin client that talks to it.

### Services

| Service | Command | Port | Notes |
|---------|---------|------|-------|
| Engine (`medousa_daemon`) | `cargo run -p medousa --bin medousa_daemon` | `127.0.0.1:7419` | Core HTTP API, durable jobs, vault, memory. Required. |
| Desktop app | `cd apps/medousa-home && npm run tauri dev` | Vite `1420` | Connected client (engine + UI). Needs a display + webkit2gtk. |
| App UI preview only | `cd apps/medousa-home && npm run dev` | `1420` | Browser-only; see caveat below. |

Standard build/run commands live in `docs/cookbook/build-from-source.md` and `package.json` scripts.

### Non-obvious gotchas

- **Rust edition 2024 needs Rust ≥ 1.85.** The base image may ship an older `rustc`; the update
  script installs/uses the `stable` toolchain. Build the engine with **default features** for dev —
  the `embedded-inference` feature pulls in `mistralrs` (huge, optionally GPU) and is only needed for
  the local Gemma "private brain".
- **System libraries are required and are NOT installed by the update script** (they live in the VM
  snapshot): engine needs `pkg-config` + `libssl-dev`; the Tauri desktop app additionally needs
  `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libsoup-3.0-dev patchelf`.
- **The engine starts without any LLM key.** Health, vault CRUD, search, workspace, manuscripts, and
  memory all work offline. But **chat / agent turns require an LLM provider** — set a cloud key
  (`MEDOUSA_<PROVIDER>_API_KEY`, e.g. `MEDOUSA_OPENAI_API_KEY`) or run Ollama / embedded Gemma.
  Without one, sending a chat message returns a "stream event failed" error; everything else works.
- **Default daemon backend is in-memory** (data lost on restart). For persistence pass
  `--backend surreal-kv:/path/to/runtime.surrealkv`. SurrealKV is embedded — no DB server needed.
- **`npm run dev` (browser) cannot reach the engine.** The frontend is Tauri-gated: every engine
  call is behind `isTauri()` and there is no HTTP fallback in the browser. Use `npm run dev` only for
  pure UI work. To exercise the app end-to-end use `npm run tauri dev`, which spawns and manages its
  own engine sidecar (in-memory). Stop any standalone daemon on `:7419` first to avoid a port clash.
- **Tauri dev needs the engine sidecar binary** at
  `apps/medousa-home/src-tauri/binaries/medousa_daemon-<target-triple>`. The official
  `npm run prepare:sidecar` does a heavy `--release` + `embedded-inference` build; for dev you can
  instead reuse the debug engine:
  `cp target/debug/medousa_daemon "apps/medousa-home/src-tauri/binaries/medousa_daemon-$(rustc -vV | sed -n 's/^host: //p')"`.
- **End-to-end smoke test:** `bash scripts/smoke-home-api.sh` against a running daemon (hits health,
  vault CRUD, search, workspace snapshot, manuscripts, capabilities).

### Lint / test

- Engine tests: `cargo test --lib` (uses in-memory `kv-mem` Surreal; no external services).
- Engine lint: `cargo fmt --check` and `cargo clippy` (requires the `rustfmt`/`clippy` components).
  Both currently report **pre-existing** diffs/warnings on a clean checkout — not env problems.
- App check: `cd apps/medousa-home && npm run check` (svelte-check) reports a few **pre-existing**
  type errors on a clean checkout.
