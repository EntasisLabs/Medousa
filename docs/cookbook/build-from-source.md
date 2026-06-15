# Build from source

## Medousa app (Tauri)

Engine should be running (or will be started by the app):

```bash
medousa start daemon --inference   # recommended for offline dev

cd apps/medousa-home
npm install
npm run tauri dev
```

Release binary:

```bash
cd apps/medousa-home
npm run tauri build
```

Artifacts: `apps/medousa-home/src-tauri/target/release/`

Design notes: [architecture/medousa-home-tauri-design.md](../../architecture/medousa-home-tauri-design.md)

---

## Engine only (Cargo)

Pick the inference backend for your machine:

| Platform | Feature | Backend |
|----------|---------|---------|
| Apple Silicon / macOS | `embedded-inference-metal` | Metal |
| Linux / Windows + NVIDIA (CUDA toolkit required at build) | `embedded-inference-cuda` | CUDA |
| Any (fallback) | `embedded-inference` | CPU |

```bash
cd Medousa   # repo root with Cargo.toml

# Apple Silicon / macOS
cargo build -p medousa --bin medousa_daemon --features embedded-inference-metal
cargo run -p medousa --bin medousa_daemon --features embedded-inference-metal -- --local-engine

# Linux / Windows CPU (works everywhere, slower)
cargo build -p medousa --bin medousa_daemon --features embedded-inference
cargo run -p medousa --bin medousa_daemon --features embedded-inference -- --local-engine

# Linux / Windows + NVIDIA (requires CUDA toolkit + driver at build time)
cargo build -p medousa --bin medousa_daemon --features embedded-inference-cuda
cargo run -p medousa --bin medousa_daemon --features embedded-inference-cuda -- --local-engine
```

Runtime overrides:

- `MEDOUSA_LOCAL_ENGINE_CPU=1` — force CPU even when Metal/CUDA is available
- `MEDOUSA_LOCAL_ENGINE_CUDA=1` — prefer CUDA when the binary was built with `embedded-inference-cuda`

Desktop app sidecar builds pick features automatically via `scripts/prepare-engine-sidecar.sh` (`MEDOUSA_EMBEDDED_INFERENCE=auto|metal|cuda|cpu`).

Or install full CLI set:

```bash
./scripts/install.sh --from-source
```

---

## iPhone dev

Clone on Mac, engine on LAN, install to device:

[apps/medousa-home/MOBILE-DEV.md](../../apps/medousa-home/MOBILE-DEV.md)

---

## App repo layout

| Path | Purpose |
|------|---------|
| `apps/medousa-home/` | Medousa desktop + mobile shell |
| `src/bin/medousa_daemon.rs` | Medousa Engine |
| `src/bin/medousa.rs` | CLI launcher |
| `architecture/` | Plans & component docs |
