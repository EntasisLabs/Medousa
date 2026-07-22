# Release to Cloudflare R2

Self-hosted distribution: build locally → sign → publish manifests → upload to R2 → landing page + installer pull from the same CDN.

**CI:** see [release-ci-setup.md](release-ci-setup.md) for GitHub Actions + R2 secrets.

## One-time setup

### 1. Cloudflare R2

1. Create bucket (e.g. `medousa-releases`).
2. Create R2 API token with **Object Read & Write**.
3. Enable public access via **custom domain** (recommended), e.g. `https://releases.medousa.app/medousa`.
4. Add CORS rule (GET/HEAD from `*` or your domains).

Set these in your shell profile or a local `release.env` (do not commit secrets):

```bash
export MEDOUSA_RELEASE_BASE_URL="https://releases.entasislabs.com/medousa"
export MEDOUSA_RELEASE_CHANNEL="stable"

export MEDOUSA_R2_BUCKET="medousa"
export MEDOUSA_R2_ENDPOINT="https://3b2e3338687e8e0abd4c85280e87fd7a.r2.cloudflarestorage.com"
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
```

Public download URL: `https://releases.entasislabs.com/medousa`  
S3 upload endpoint: `https://3b2e3338687e8e0abd4c85280e87fd7a.r2.cloudflarestorage.com` (CI/local sync only)

Object layout after upload:

```
medousa/stable/
  release-manifest.json
  installer-bootstrap.json
  SHA256SUMS
  Medousa_0.1.0_x64-setup.exe          # Windows default download (signed desktop)
  MedousaInstaller_0.1.0_x64-setup.exe # Windows add-ons / customize (optional)
  Medousa Installer_0.1.0_aarch64.dmg    # macOS default download
  medousa-v0.1.0-….tar.gz
  …
```

Public URLs:

- `{MEDOUSA_RELEASE_BASE_URL}/stable/installer-bootstrap.json`
- `{MEDOUSA_RELEASE_BASE_URL}/stable/release-manifest.json`

**Windows express path:** `installer-bootstrap.json` → `platforms.windows-x64.url` is the signed desktop NSIS setup (`Medousa_*_x64-setup.exe`), not the Medousa Installer MSI. The nested installer flow is avoided — one download, one install, then open Medousa. `platforms.windows-x64.installerUrl` points at Medousa Installer for users who want the component picker or add-ons later.

### 2. Landing page

In `medousa-landing/.env.local`:

```bash
VITE_MEDOUSA_RELEASE_BASE_URL=https://releases.entasislabs.com/medousa
VITE_MEDOUSA_RELEASE_CHANNEL=stable
```

Rebuild and deploy the landing site. Download buttons fetch `installer-bootstrap.json` and pick the right artifact for Mac / Windows / Linux. On Windows, use `platforms.windows-x64.url` (desktop setup); offer `installerUrl` only as an “Advanced / add gadgets” link if you expose it.

### 3. Windows signing (Azure Artifact Signing)

Full guide: **[azure-windows-signing.md](azure-windows-signing.md)** — Azure app registration, GitHub secrets/variables, CI (automatic), and local `sign-windows.ps1`.

Sign **before** publishing so manifest SHA256s match signed files.

### 4. macOS (optional)

Sign/notarize desktop + installer DMGs with your Developer ID (see `.github/workflows/release.yml` for the CI pattern). Run before publish so manifest checksums match signed files.

---

## Per-package versions

[`scripts/release/package-versions.toml`](../../scripts/release/package-versions.toml) stamps each package archive independently (`engine-v0.4.2-…`, `adapter-discord-v0.4.1-…`). The channel `release-manifest.json` top-level `version` is the **channel head** (max of package versions / last full train). Home Packages and the Installer offer updates from **per-package** version inequality — shipping desktop alone must not flip WhatsApp to “update available”.

Prefer CI targeted dispatch for partial ships (see [release-ci-setup.md](release-ci-setup.md)). Locally:

```bash
# Build only what you need
./scripts/release/build.sh --components engine,cli --without-local-brain
./scripts/release/package-all-components.sh --packages engine,cli --skip-suite

# Merge into existing channel indexes when publishing a partial set
./scripts/release/publish-self-hosted.sh \
  --staging dist \
  --merge-base /path/to/downloaded/channel \
  --version "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
```

Full train (replace channel indexes): add `--full-train`.

## Release checklist (every version)

Run on each platform, then merge artifacts into one `dist/` folder on the machine that publishes.

| Step | Command |
|------|---------|
| Bump stamps | Edit `scripts/release/package-versions.toml` (only packages you ship) |
| Build selected bins | `./scripts/release/build.sh --components engine,cli,…` |
| Package selected | `./scripts/release/package-all-components.sh --packages …` |
| Build desktop | `cd apps/medousa-home && npm run tauri build` |
| Sign Windows desktop | `.\scripts\release\sign-windows.ps1 dist\final\Medousa_*_x64-setup.exe` (required before publish) |
| Copy all artifacts into | `dist/` |
| Publish manifests | `./scripts/release/publish-local.sh` (or `publish-self-hosted.sh --merge-base …`) |
| Upload to R2 | `./scripts/release/publish-local.sh --upload` |
| Bake R2 URL into installer | `./scripts/release/set-installer-config.sh` |
| Rebuild installer | `cd apps/medousa-installer && npm run tauri:build` |
| Sign installer (Win) | `.\scripts\release\sign-windows.ps1 …` |
| Re-publish + upload | `./scripts/release/publish-local.sh --upload` |

**Order matters:** sign → publish → upload. If you change binaries after publish, re-run publish and upload.

### Quick publish (artifacts already in `dist/`)

```bash
export MEDOUSA_RELEASE_BASE_URL="https://releases.medousa.app/medousa"
./scripts/release/publish-local.sh --upload
```

### Bake CDN URL into the installer

The installer embeds `MEDOUSA_RELEASE_BASE_URL` from `apps/medousa-installer/public/installer-config.json` at build time:

```bash
export MEDOUSA_RELEASE_BASE_URL="https://releases.medousa.app/medousa"
./scripts/release/set-installer-config.sh
cd apps/medousa-installer && npm run tauri:build
```

End users do not need env vars — the installer fetches `release-manifest.json` from your CDN automatically.

---

## Verify

```bash
curl -s "$MEDOUSA_RELEASE_BASE_URL/stable/installer-bootstrap.json" | head
curl -s "$MEDOUSA_RELEASE_BASE_URL/stable/release-manifest.json" | head
```

On a clean VM: download from medousa.app → **Windows:** one desktop setup → launch Medousa → express wizard. **Mac/Linux:** installer → express install → launch Medousa.

---

## Scripts reference

| Script | Purpose |
|--------|---------|
| `set-installer-config.sh` | Write `installer-config.json` from env |
| `publish-self-hosted.sh` | Generate manifests in `dist/final/` |
| `publish-local.sh` | Config + publish (+ optional `--upload`) |
| `upload-r2.sh` | `aws s3 sync` to R2 |
| `sign-windows.ps1` | Azure Artifact Signing for `.exe` / `.msi` |

See also [install-and-self-host.md](install-and-self-host.md) for end-user install paths.
