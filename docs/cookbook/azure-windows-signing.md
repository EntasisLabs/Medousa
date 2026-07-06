# Windows signing with Azure Artifact Signing

Digitally sign Medousa `.exe` / `.msi` so Windows SmartScreen shows **Entasis Labs** instead of “Unknown publisher”.

Uses **Azure Artifact Signing** (formerly Trusted Signing) — no `.pfx` file; certificates live in Azure HSM.

---

## What you need from Azure Portal

Open your **Artifact Signing** resource and note:

| Field | Example | GitHub |
|-------|---------|--------|
| **Endpoint** | `https://eus.codesigning.azure.net/` | Variable `AZURE_CODESIGNING_ENDPOINT` |
| **Account name** | `entasis-signing` | Variable `AZURE_CODESIGNING_ACCOUNT` |
| **Certificate profile name** | `medousa-public` | Variable `AZURE_CODESIGNING_PROFILE` |

Also confirm **identity validation** on the profile shows **Complete** (not Pending). Signing returns 403 until this finishes.

Endpoint region must match your resource (e.g. `eus`, `wus2`, `weu`).

---

## Part 1 — Azure App Registration (for CI)

GitHub Actions authenticates to Azure as a **service principal**.

### 1. Create App Registration

1. Azure Portal → **Microsoft Entra ID** → **App registrations** → **New registration**
2. Name: `Medousa-GitHub-Signing`
3. Register (single tenant is fine)

Copy:

- **Application (client) ID** → GitHub secret `AZURE_CLIENT_ID`
- **Directory (tenant) ID** → GitHub secret `AZURE_TENANT_ID`
- **Subscription ID** (Portal home → Subscriptions) → GitHub secret `AZURE_SUBSCRIPTION_ID`

### 2. Federated credential (OIDC — recommended)

App registration → **Certificates & secrets** → **Federated credentials** → **Add credential**

- **Federated credential scenario:** GitHub Actions deploying Azure resources
- **Organization:** `EntasisLabs`
- **Repository:** `Medousa`
- **Entity type:** Branch (or Environment if you use one)
- **Branch name:** `main` (and/or `release` if you use it)
- **Name:** `github-medousa-release`

No client secret needed for OIDC.

**Alternative:** create a **Client secret** → GitHub secret `AZURE_CLIENT_SECRET` (simpler but rotates manually).

### 3. Grant signing permission

1. Portal → your **Artifact Signing account** → **Access control (IAM)**
2. **Add role assignment**
3. Role: **Artifact Signing Certificate Profile Signer**
4. Assign to: `Medousa-GitHub-Signing` app registration

Or assign on the specific **certificate profile** resource if IAM is scoped there.

---

## Part 2 — GitHub configuration

**EntasisLabs/Medousa** → Settings → Secrets and variables → Actions

### Secrets

| Secret | Value |
|--------|--------|
| `AZURE_CLIENT_ID` | App registration client ID |
| `AZURE_TENANT_ID` | Tenant ID |
| `AZURE_SUBSCRIPTION_ID` | Subscription ID |
| `AZURE_CLIENT_SECRET` | *(only if not using OIDC)* |

### Variables

| Variable | Value |
|----------|--------|
| `AZURE_CODESIGNING_ENDPOINT` | `https://eus.codesigning.azure.net/` *(your region)* |
| `AZURE_CODESIGNING_ACCOUNT` | Your signing account name |
| `AZURE_CODESIGNING_PROFILE` | Your certificate profile name |

When `AZURE_CODESIGNING_ACCOUNT` is set, the Release workflow signs Windows desktop + installer bundles automatically.

---

## Part 3 — Local signing (Windows PC)

For manual builds before upload:

### 1. Install tools

```powershell
winget install -e --id Microsoft.Azure.ArtifactSigningClientTools
az login
```

### 2. Create metadata file

```powershell
copy scripts\release\azure-signing-metadata.example.json scripts\release\azure-signing-metadata.json
```

Edit `azure-signing-metadata.json`:

```json
{
  "Endpoint": "https://eus.codesigning.azure.net/",
  "CodeSigningAccountName": "YOUR_ACCOUNT",
  "CertificateProfileName": "YOUR_PROFILE"
}
```

(`azure-signing-metadata.json` is gitignored.)

Your signed-in `az` user needs **Artifact Signing Certificate Profile Signer** on the profile (same as the app registration in CI).

### 3. Sign after build

```powershell
# Desktop
.\scripts\release\sign-windows.ps1 -Verify `
  apps\medousa-home\src-tauri\target\release\bundle\nsis\*.exe

# Installer
.\scripts\release\sign-windows.ps1 -Verify `
  apps\medousa-installer\src-tauri\target\release\bundle\nsis\*.exe
```

Sign **before** `publish-local.sh` so manifest SHA256s match signed files.

---

## Verify a signature

```powershell
signtool verify /pa /v path\to\MedousaInstaller.exe
```

Or: right-click → Properties → Digital Signatures.

SmartScreen reputation builds over time; a valid signature is required first.

---

## Troubleshooting

| Error | Fix |
|-------|-----|
| **403 Forbidden** | Identity validation incomplete, or missing **Certificate Profile Signer** role |
| **Identity validation failed** | Re-submit business verification in Azure; can take 1–5 business days |
| **Unable to resolve action** | Use `azure/artifact-signing-action@v2` (old `trusted-signing-action` repo was renamed) |
| **Dlib / signtool failed locally** | Use x64 signtool + x64 `Azure.CodeSigning.Dlib.dll`; SDK ≥ 10.0.22621 |
| **Signed in CI but SmartScreen still warns** | Normal for new publishers — reputation improves with download volume |

---

## Related

- [release-ci-setup.md](release-ci-setup.md) — full Release workflow
- [release-to-r2.md](release-to-r2.md) — R2 upload
- [Microsoft: Artifact Signing integrations](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations)
