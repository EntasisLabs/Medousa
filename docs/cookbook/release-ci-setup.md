# Release CI setup (GitHub Actions)

Workflow: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)

Builds CLI + desktop + installer on every tag push or manual run, then publishes to **Cloudflare R2** and **GitHub Releases**.

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

### Secrets (Settings → Secrets and variables → Actions → Secrets)

| Secret | Required | Notes |
|--------|----------|-------|
| `R2_ACCESS_KEY_ID` | **Yes** | Cloudflare R2 → Manage R2 API tokens |
| `R2_SECRET_ACCESS_KEY` | **Yes** | Same token |

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

| GitHub **Variables** | From Azure portal |
|---------------------|-------------------|
| `AZURE_CODESIGNING_ENDPOINT` | e.g. `https://eus.codesigning.azure.net/` |
| `AZURE_CODESIGNING_ACCOUNT` | Signing account name |
| `AZURE_CODESIGNING_PROFILE` | Certificate profile name |

| GitHub **Secrets** | From App Registration |
|---------------------|----------------------|
| `AZURE_CLIENT_ID` | Application ID |
| `AZURE_TENANT_ID` | Directory ID |
| `AZURE_SUBSCRIPTION_ID` | Subscription ID |

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

Redeploy landing after first R2 upload.

---

## How to run

### First run (recommended)

1. Add `R2_ACCESS_KEY_ID` + `R2_SECRET_ACCESS_KEY` secrets.
2. Actions → **Release** → **Run workflow**.
3. Leave version blank (uses `Cargo.toml`), keep **Upload R2** and **GitHub Release** checked.
4. Wait ~30–60 min for matrix builds.
5. Verify:

```bash
curl -s https://releases.entasislabs.com/medousa/stable/installer-bootstrap.json | head
```

### Tag release (normal flow)

```bash
git tag v0.1.0
git push origin v0.1.0
```

Same pipeline runs automatically.

---

## What the workflow does

1. **build-cli** — engine, adapters, tarballs (5 targets)
2. **build-desktop** — Medousa app (Mac signed via `MEDOUSA` env, Win/Linux)
3. **build-installer** — writes `installer-config.json` with CDN URL, builds installer
4. **release** — merge artifacts → manifests → **upload R2** → **GitHub Release**

All matrix jobs set **`shell: bash`** at the job level. Windows runners default to PowerShell; release scripts (`.sh`, `find`, `[[ ]]`) require bash (Git Bash on `windows-latest`).

Installers baked in step 3 fetch packages from `https://releases.entasislabs.com/medousa/stable/release-manifest.json`.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Windows `ParserError` / `Missing '(' after 'if'` | Job must use `shell: bash` — merge latest `release.yml` |
| R2 upload fails “secrets missing” | Add `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY` |
| Mac desktop build fails on secrets | Check **Environment** `MEDOUSA`, not just repo secrets |
| `curl` 404 on manifest | Custom domain not wired, or prefix mismatch — check `MEDOUSA_R2_PREFIX` |
| SmartScreen on Windows | Set Azure variables/secrets per [azure-windows-signing.md](azure-windows-signing.md) |
| GitHub Release “tag exists” | Bump version or delete old tag |

See also: [release-to-r2.md](release-to-r2.md) for local/manual releases.
