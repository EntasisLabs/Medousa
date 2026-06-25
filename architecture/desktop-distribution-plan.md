# Desktop distribution plan

> **Status:** In progress — desktop matrix in `.github/workflows/release.yml` (2026-06)  
> **Scope:** Mac / Windows / Linux installable bundles via GitHub Releases — **not** iOS/Android (App Store / TestFlight track)

## Current state

| Track | Status | Artifact |
|-------|--------|----------|
| **CLI / Engine** | Shipped | `medousa-vX.Y.Z-<triple>.tar.gz` on tag push (`.github/workflows/release.yml`) |
| **Desktop app (Tauri)** | Local build only | `.dmg` / `.app` on Mac via `npm run build:desktop` |
| **Mobile** | Separate | TestFlight / stores — see `apps/medousa-home/MOBILE-DEV.md` |

The product README links to [GitHub Releases](https://github.com/EntasisLabs/Medousa/releases), but releases today contain **CLI tarballs only**, not the Medousa.app installers most users expect.

### What already works (Mac, manual)

- `prepare-engine-sidecar.sh` bundles slim `medousa_daemon` + `medousa_local` (`embedded-inference-metal` on Apple Silicon)
- Welcome wizard, auto-start engine, offline Gemma path
- `npm run build:desktop` → `src-tauri/target/release/bundle/`

## Target: downloadable desktop bundles (all platforms)

```
Tag v*  →  CI builds per OS  →  GitHub Release assets
           ├─ medousa-v*-.tar.gz     (CLI, existing)
           ├─ Medousa_*_aarch64.dmg   (macOS Apple Silicon)
           ├─ Medousa_*_x64.dmg       (macOS Intel, optional)
           ├─ Medousa_*_x64-setup.exe / .msi  (Windows)
           └─ Medousa_*_amd64.AppImage / .deb (Linux)
```

## Work breakdown

### P0 — Multi-platform app CI (~1–2 weeks)

1. **`release-desktop.yml`** (or extend `release.yml`) — matrix: `macos-14`, `windows-latest`, `ubuntu-latest`
2. **Cross-build sidecars** — extend `prepare-engine-sidecar.sh` with `--target <triple>` for CI (today builds host only)
3. **Attach bundles** to the same GitHub Release as CLI tarballs
4. **README** — split “Download Medousa (app)” vs “Install engine only (CLI)”

### P1 — Trustworthy installs (~1 week after P0)

1. **macOS** — Developer ID sign + notarize (Gatekeeper)
2. **Windows** — Authenticode signing (SmartScreen)
3. **Smoke matrix** — fresh VM: install → wizard → first chat on each OS

### P2 — Polish (Phase F, deferred)

- Tauri auto-updater
- Download landing page (optional; Releases may suffice)
- Linux local inference story (Metal is Mac-only today; Win/Linux use cloud/BYOK unless CPU inference is added)

## Platform notes

| OS | Gap |
|----|-----|
| **macOS** | Closest — manual `.dmg` works; needs notarization CI + Intel sidecar if supporting x64 Mac |
| **Windows** | Tauri `.msi` + Windows sidecar triple; no embedded Metal inference in sidecar script today |
| **Linux** | AppImage/deb straightforward; sidecar cross-compile; no embedded Gemma path yet |

## Distance estimate

| Milestone | Effort |
|-----------|--------|
| Share Mac `.dmg` with testers (manual upload) | **Today** |
| GitHub Releases with Mac + Win + Linux app installers | **~1–2 weeks** |
| Signed / notarized “double-click install” everywhere | **~3–4 weeks** + Apple/Microsoft certs |

## Related

- `scripts/release/publish.sh --ci vX.Y.Z` — tag triggers CLI matrix today
- `architecture/archive/first-run-gap-analysis-2026-06.md` — A1 sidecar bundling (shipped)
- `architecture/archive/first-run-and-lan-pairing-plan.md` — Phase F packaging (deferred)
