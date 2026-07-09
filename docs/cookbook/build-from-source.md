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

#### Prerequisites (one-time)

Medousa Windows **binaries and Tauri builds use the MSVC toolchain** (`x86_64-pc-windows-msvc`), not GNU/MinGW.

1. **Rust (MSVC host)** — if `rustup show` lists `stable-x86_64-pc-windows-gnu` as default, switch:
   ```powershell
   rustup default stable-x86_64-pc-windows-msvc
   rustup target add x86_64-pc-windows-msvc
   ```
   GNU Rust without a full MinGW install fails with `error calling dlltool 'dlltool.exe': program not found`.

2. **Visual Studio 2022 Build Tools** — C++ workload (provides `link.exe`):
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
   Without this, MSVC Rust fails with `linker 'link.exe' not found`.

3. **Node.js 20+** — for `apps/medousa-home` (`npm install`, `tauri build`).

4. **WebView2** — usually preinstalled on Windows 10/11; required at runtime for the Tauri shell.

Verify everything:

```powershell
.\scripts\dev\check-windows-build.ps1
# optional: also switch rustup default to MSVC
.\scripts\dev\check-windows-build.ps1 -FixToolchain
```

Build from a normal PowerShell window (loads VS linker + SDK libs automatically):

```powershell
.\scripts\dev\cargo-msvc.ps1 build --bin medousa_daemon
cd apps\medousa-home
npm install
..\..\scripts\dev\cargo-msvc.ps1 build   # Tauri Rust crate (from src-tauri via manifest)
npm run tauri:build                      # full desktop bundle (frontend + sidecar; loads MSVC env on Windows)
```

If you prefer the classic workflow, open **x64 Native Tools Command Prompt for VS 2022** and run `cargo build` there instead.

**Common errors**

| Error | Cause | Fix |
|-------|-------|-----|
| `dlltool.exe: program not found` | GNU/MinGW Rust host | `rustup default stable-x86_64-pc-windows-msvc` |
| `linker link.exe not found` | No VS C++ tools | Install Build Tools (above) |
| `cannot open file msvcrt.lib` | VS env not loaded, or incomplete VS install | Use `cargo-msvc.ps1` or Native Tools prompt |
| `LNK1180 insufficient disk space` | Debug builds are large (~tens of GB) | Free disk space; target dir is `.cache/cargo-target` |

Shell scripts (`.sh`) are kept with LF line endings for Git Bash/WSL. On a bare Windows dev machine, use the PowerShell equivalents via the Node runners — they avoid `$'\r'` parse errors from CRLF in bash scripts.

**Desktop app + sidecar:**

```powershell
cd apps\medousa-home
npm install
npm run tauri:build
```

`prepare:sidecar` and `tauri:build` call `prepare-engine-sidecar-runner.mjs`, which runs `prepare-engine-sidecar.ps1` on Windows.

**Installer app:**

```powershell
# Point the installer at your CDN (required — empty config falls back to GitHub and 404s)
$env:MEDOUSA_RELEASE_BASE_URL = "https://releases.entasislabs.com/medousa"
.\scripts\release\set-installer-config.ps1

cd apps\medousa-installer
npm install
npm run tauri:dev    # or tauri:build
```

For a one-off dev session without baking the URL into the binary, set `MEDOUSA_RELEASE_BASE_URL` in the shell before launching.

`tauri`, `tauri:dev`, and `tauri:build` route through `scripts/dev/tauri-runner.mjs`, which loads the MSVC environment on Windows (same as medousa-home).

After changing release artifacts on R2, regenerate manifests with `scripts/release/republish-manifests.sh` — desktop/installer entries must be per-platform (Windows `.msi`/`.exe`, not `.dmg`).

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
cargo build -p medousa-local-inference --bin medousa_local --features embedded-inference-metal
cargo run -p medousa-local-inference --bin medousa_local --features embedded-inference-metal -- --load-recommended

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
