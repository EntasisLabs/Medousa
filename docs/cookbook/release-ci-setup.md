# Release CI setup (GitHub Actions)

Workflow: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)

Supports **full-train** releases (`v*` tags) and **targeted** component ships (`workflow_dispatch` checkboxes). Publishes to **Cloudflare R2** and optionally **GitHub Releases**. Untouched packages keep their prior channel URLs/versions via manifest merge.

Per-package stamps live in [`scripts/release/package-versions.toml`](../../scripts/release/package-versions.toml). Bump only the packages you ship.

### Package map (CDN tarballs)

| Package id | Binaries | Notes |
|------------|----------|-------|
| `engine` | `medousa`, `medousa_daemon`, `medousa_cli`, `medousa_tui` | Headless core — no separate `cli` package |
| `adapter-*` | one each | Slim crates under `adapters/` (not root `[[bin]]`s) |
| `mcp-gateway` | `medousa_mcp_gateway` | Slim crate under `adapters/medousa-mcp-gateway` |
| `local-brain` | `medousa_local` | Offline inference |
| `desktop` / `installer` | app bundles | Tauri |

There is **no** `medousa-v*` / `engine-suite` archive. Operators install extras with `medousa pull <name>`.

After dropping the suite, the **next** publish that should clean the channel index is a **full train** (`ship_all` or `v*` tag). Targeted merges keep untouched keys (including any leftover `cli-*` / suite entries from older channels).

---

## Two URLs (don’t mix them up)

| URL | Purpose |
|-----|---------|
| `https://releases.entasislabs.com/medousa` | **Public CDN** — landing page, installer, manifests |
| `https://3b2e3338687e8e0abd4c85280e87fd7a.r2.cloudflarestorage.com` | **S3 API endpoint** — CI upload only, not for browsers |

After upload, files live at:

```
https://releases.entasislabs.com/medousa/stable/release-manifest.json
https://releases.entasislabs.com/medousa/stable/installer-bootstrap.json
```

---

## One-time GitHub configuration

Repo: **EntasisLabs/Medousa**

### Secrets (Environment: `MEDOUSA`)

The **publish** job and **Windows signing** use GitHub Environment **`MEDOUSA`**. Store release secrets there (Settings → Environments → MEDOUSA → Secrets).

| Secret | Required | Notes |
|--------|----------|-------|
| `MEDOUSA_R2_ACCESS_KEY_ID` | **Yes** (for R2 upload) | Cloudflare R2 → Manage R2 API tokens |
| `MEDOUSA_R2_SECRET_ACCESS_KEY` | **Yes** | Same token |

Legacy names `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` also work (repo or environment secrets).

Repository-level secrets work too if you remove `environment: MEDOUSA` from the publish job.

### Variables (optional — workflow has sensible defaults)

| Variable | Default in workflow |
|----------|---------------------|
| `MEDOUSA_RELEASE_BASE_URL` | `https://releases.entasislabs.com/medousa` |
| `MEDOUSA_RELEASE_CHANNEL` | `stable` |
| `MEDOUSA_R2_BUCKET` | `medousa` |
| `MEDOUSA_R2_ENDPOINT` | `https://3b2e3338687e8e0abd4c85280e87fd7a.r2.cloudflarestorage.com` |
| `MEDOUSA_R2_PREFIX` | `medousa/stable` |

You only need to set Variables if you change bucket/domain later.

### macOS signing (Environment: `MEDOUSA`)

The **desktop app** job uses GitHub Environment **`MEDOUSA`** for Apple secrets (Developer ID + notarization). If Mac `.dmg` builds succeed in CI, you’re set.

| Secret (on `MEDOUSA` environment) | Purpose |
|-----------------------------------|---------|
| `APPLE_CERTIFICATE` | Base64 `.p12` — **Developer ID Application** |
| `APPLE_CERTIFICATE_PASSWORD` | Export password |
| `APPLE_PASSWORD` | App-specific password |
| `KEYCHAIN_PASSWORD` | Any random string |

Optional vars: `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`.

**Note:** The **Medousa Installer** `.dmg` is built without notarization in CI today. Desktop app is signed; installer bundle is unsigned on Mac until we add that step.

### Windows signing (Azure Artifact Signing)

When your certificate profile is ready, follow **[azure-windows-signing.md](azure-windows-signing.md)**.

Quick checklist:

| GitHub **Variables** (on `MEDOUSA` environment or repo) | From Azure portal |
|---------------------|-------------------|
| `MEDOUSA_AZURE_CODESIGNING_ENDPOINT` | e.g. `https://eus.codesigning.azure.net/` |
| `MEDOUSA_AZURE_CODESIGNING_ACCOUNT` | Signing account name |
| `MEDOUSA_AZURE_CODESIGNING_PROFILE` | Certificate profile name |

Legacy names `AZURE_CODESIGNING_*` also work.

| GitHub **Secrets** (on `MEDOUSA` environment) | From App Registration |
|---------------------|----------------------|
| `MEDOUSA_AZURE_CLIENT_ID` | Application ID |
| `MEDOUSA_AZURE_TENANT_ID` | Directory ID |
| `MEDOUSA_AZURE_SUBSCRIPTION_ID` | Subscription ID |

Legacy names `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` also work.

Use a **federated credential** on the app registration for GitHub OIDC (recommended — no client secret). See [azure-windows-signing.md](azure-windows-signing.md).

---

## Cloudflare R2 checklist

1. Bucket **`medousa`** exists.
2. Custom domain **`releases.entasislabs.com`** connected to bucket (path `/medousa/...` matches upload prefix).
3. **CORS** allows GET/HEAD (for landing page bootstrap fetch):

```json
[
  {
    "AllowedOrigins": ["https://medousa.app", "https://entasislabs.com"],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["*"],
    "MaxAgeSeconds": 3600
  }
]
```

4. R2 API token with **Object Read & Write** → paste into GitHub Secrets above.

---

## Landing page

In **medousa-landing** repo, set at build time:

```bash
VITE_MEDOUSA_RELEASE_BASE_URL=https://releases.entasislabs.com/medousa
VITE_MEDOUSA_RELEASE_CHANNEL=stable
```

Redeploy landing after first R2 upload. **Get Medousa** should use `platforms.<os>.url` from bootstrap. On Windows that is the signed desktop `Medousa_*_x64-setup.exe` (`artifactKind: desktop`). Optional footer link: `platforms.windows-x64.installerUrl` for Medousa Installer (add-ons).

---

## How to run

### Full train (`v*` tag) — ship everything

1. Set every entry in `scripts/release/package-versions.toml` to `X.Y.Z`.
2. Align root `Cargo.toml`, WhatsApp crate, Home/Installer `package.json` + tauri conf to `X.Y.Z`.
3. Tag and push:

```bash
git tag v0.4.2
git push origin v0.4.2
```

CI asserts all package stamps equal the tag, builds the full matrix, and **replaces** channel indexes (`--full-train`).

### Targeted ship (workflow_dispatch)

1. Bump only the packages you changed in `package-versions.toml` (and matching app/crate version if needed).
2. Actions → **Release** → **Run workflow**.
3. Check the `ship_*` boxes you need (e.g. `ship_desktop` only). Leave `ship_all` off.
4. Keep **Upload R2** on. GitHub Release is optional for partial ships.
5. Publish **merges** into the existing channel `release-manifest.json` — adapters you did not rebuild keep their old version/URL.

| Goal | Checkboxes | Bump |
|------|------------|------|
| Home polish | `ship_desktop` (+ `ship_engine` if daemon API changed) | `desktop` (+ `engine`) |
| Adapter fix | `ship_adapters` | that adapter id |
| Engine / CLI / TUI | `ship_engine` | `engine` |
| MCP only | `ship_mcp` | `mcp-gateway` |
| Offline brain | `ship_local_brain` | `local-brain` |
| Everything | `ship_all` or push a `v*` tag | all ids |

### First run (recommended)

1. Add `R2_ACCESS_KEY_ID` + `R2_SECRET_ACCESS_KEY` secrets.
2. Actions → **Release** → **Run workflow** with `ship_all` (or push a `v*` tag).
3. Wait for matrix builds.
4. Verify:

```bash
curl -s https://releases.entasislabs.com/medousa/stable/installer-bootstrap.json | head
curl -s https://releases.entasislabs.com/medousa/stable/installer-bootstrap.json | jq '.platforms["windows-x64"]'
# Expect artifactKind "desktop" and fileName Medousa_*_x64-setup.exe
curl -s https://releases.entasislabs.com/medousa/stable/release-manifest.json | jq '.packages | keys'
# Expect engine-* keys; no medousa-v* / engine-suite / cli-* after a full train
curl -s https://releases.entasislabs.com/medousa/stable/release-manifest.json \
  | jq '[.packages | keys[] | select(startswith("engine-") or startswith("cli-") or startswith("medousa-") or startswith("engine-suite"))]'
```

### Republish manifests only (no rebuild)

If binaries are already on R2 but `release-manifest.json` or `installer-bootstrap.json`
were wrong or empty, use **Actions → Republish manifests → Run workflow**. It syncs
existing artifacts down from R2, regenerates the JSON files, and uploads only those
two files (~1–2 minutes, no compile).

Or locally (with R2 credentials):

```bash
export MEDOUSA_RELEASE_BASE_URL=https://releases.entasislabs.com/medousa
export MEDOUSA_R2_BUCKET=medousa
export MEDOUSA_R2_ENDPOINT=https://….r2.cloudflarestorage.com
export AWS_ACCESS_KEY_ID=…
export AWS_SECRET_ACCESS_KEY=…
./scripts/release/republish-manifests.sh --from-r2 --upload --version 0.1.0
```

If you still have `dist/final` from the publish job on a runner, skip the download:

```bash
./scripts/release/republish-manifests.sh --staging dist/final --upload --version 0.1.0
```

---

## What the workflow does

1. **prepare** — resolve `ship_*` selection (`v*` / `ship_all` = full train)
2. **build-daemon** — `medousa` + `medousa_daemon` once per OS (when engine/desktop selected)
3. **build-engine** — packages `engine` (launcher + daemon + CLI + TUI); **reuses** prebuilt daemon (no second compile). Never builds the retired `medousa-v*` suite.
4. **build-adapters** / **build-mcp** / **build-local-brain** — independent legs (slim adapter crates; never rebuild engine)
5. **build-desktop** / **build-installer** — only when selected (desktop reuses daemon sidecar)
6. **release** — stage artifacts → generate delta manifests → **merge** into channel (or replace on full train) → **upload R2** → optional **GitHub Release**

Skipped legs do not block publish. Daemon is compiled once per OS and shared by desktop + engine packaging.

All matrix jobs set **`shell: bash`**. Windows runners default to PowerShell; release scripts require bash (Git Bash on `windows-latest`).

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Windows `ParserError` / `Missing '(' after 'if'` | Job must use `shell: bash` — merge latest `release.yml` |
| R2 upload fails “secrets missing” | Add `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` |
| Mac desktop build fails on secrets | Check **Environment** `MEDOUSA`, not just repo secrets |
| `curl` 404 on manifest | Custom domain not wired, or prefix mismatch — check `MEDOUSA_R2_PREFIX` |
| `installer-bootstrap.json` has empty `platforms` | Installer bundles are named `Medousa Installer_*` (Tauri productName) but an old script only matched `MedousaInstaller*` — merge latest release scripts, then run **Republish manifests** workflow (no rebuild) |
| SmartScreen on Windows | Set Azure variables/secrets per [azure-windows-signing.md](azure-windows-signing.md) |
| GitHub Release “tag exists” | Bump version or delete old tag |

See also: [release-to-r2.md](release-to-r2.md) for local/manual releases.
