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
npm run tauri:build
```

Artifacts: `apps/medousa-home/src-tauri/target/release/bundle/` (or `target/<triple>/release/bundle/` when cross-compiling)

### Windows (native PowerShell)

Shell scripts (`.sh`) are kept with LF line endings for Git Bash/WSL. On a bare Windows dev machine, use the PowerShell equivalents via the Node runners — they avoid `$'\r'` parse errors from CRLF in bash scripts.

**Desktop app + sidecar:**

```powershell
cd apps\medousa-home
npm install
npm run tauri:build
```

`prepare:sidecar` and `tauri:build` call `prepare-engine-sidecar-runner.mjs`, which runs `prepare-engine-sidecar.ps1` on Windows.

**Release binaries (engine/adapters):**

```powershell
cd Medousa   # repo root
node scripts/release/release-runner.mjs build.ps1 build.sh -- --target x86_64-pc-windows-msvc
node scripts/release/release-runner.mjs package-all-components.ps1 package-all-components.sh -- --target x86_64-pc-windows-msvc
```

**Full desktop installer bundle:**

```powershell
cd apps\medousa-home
npm run build:full
```

Requires `tar` (built into Windows 10+) for `.tar.gz` packaging scripts.

If Git still checks out `.sh` files with CRLF, re-normalize once: `git add --renormalize .` then recommit `.gitattributes`.

Design notes: [architecture/archive/medousa-home-tauri-design.md](../../architecture/archive/medousa-home-tauri-design.md)

---

## Engine + offline brain (Cargo)

The daemon is a slim catalog/scheduler process. **Offline Gemma inference runs in `medousa_local`** (built separately).

| Platform | `medousa_local` feature | Backend |
|----------|-------------------------|---------|
| Apple Silicon / macOS | `embedded-inference-metal` | Metal |
| Linux / Windows + NVIDIA | `embedded-inference-cuda` | CUDA |
| Any (fallback) | `embedded-inference` | CPU |

```bash
cd Medousa   # repo root with Cargo.toml

# Build offline brain sidecar
cargo build -p medousa --bin medousa_local --features embedded-inference-metal
cargo run -p medousa --bin medousa_local --features embedded-inference-metal -- --load-recommended

# Daemon (no embedded inference)
cargo build -p medousa --bin medousa_daemon
cargo run -p medousa --bin medousa_daemon
```

Start both from CLI:

```bash
medousa start daemon --inference   # spawns medousa_local + medousa_daemon
```

Runtime overrides:

- `MEDOUSA_LOCAL_ENGINE_CPU=1` — force CPU even when Metal/CUDA is available

Release builds (`scripts/release/build.sh`) include **iroh-transport** by default and build **medousa_local** via `--with-local-brain` (default on). Opt out of Iroh with `--without-iroh`. At runtime the Iroh gateway is on when compiled in (opt out with `MEDOUSA_IROH=0`).

Or install full CLI set:

```bash
./scripts/install.sh --from-source
```

---

## iPhone dev

Clone on Mac, engine on LAN, install to device:

[apps/medousa-home/MOBILE-DEV.md](../../apps/medousa-home/MOBILE-DEV.md) · [Mobile & LAN cookbook](../cookbook/mobile-and-lan.md)

---

## App repo layout

| Path | Purpose |
|------|---------|
| `apps/medousa-home/` | Medousa desktop + mobile shell |
| `crates/medousa-sdk/` | Rust SDK (`MedousaClient`) |
| `crates/medousa-types/` | Shared HTTP DTOs |
| `src/bin/medousa_daemon.rs` | Medousa Engine |
| `src/bin/medousa.rs` | CLI launcher |
| `architecture/` | Plans & component docs |
| `docs/` | Integrator & operator documentation |
