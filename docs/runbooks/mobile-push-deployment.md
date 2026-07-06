# Mobile push deployment

**Audience:** Entasis release engineering — shipping Medousa Home remote push (APNs) to end users.

End users only pair their phone and enable **Remote push** in settings. They never configure Apple Developer credentials.

---

## Overview

```text
┌─────────────────────┐     pairing heartbeat      ┌──────────────────────┐
│  Medousa Home       │ ─────────────────────────► │  medousa_daemon      │
│  (iOS / TestFlight) │     APNs device token      │  (Mac)               │
└─────────────────────┘                            └──────────┬───────────┘
                                                            │
                     config.json (metadata)                 │ APNs HTTP/2
                     Keychain: medousa.apns / auth_key      ▼
                                                 ┌──────────────────────┐
                                                 │  Apple Push Notification │
                                                 │  service (APNs)          │
                                                 └──────────┬───────────┘
                                                            │
                                                            ▼
                                                 ┌──────────────────────┐
                                                 │  User's iPhone       │
                                                 └──────────────────────┘
```

| Component | Who sets it up | Where it lives |
|-----------|----------------|----------------|
| APNs Auth Key (`.p8`) | Release engineering | macOS Keychain (`medousa.apns` / `auth_key`) |
| APNs metadata (`teamId`, `keyId`, `sandbox`) | Release engineering | `~/Library/Application Support/medousa/apns/config.json` |
| Push Notifications capability | iOS build (Xcode) | App entitlements (`aps-environment`) |
| Device token | Automatic | Phone → daemon via `POST /pair/heartbeat` |
| Remote push toggle | End user | Medousa Home → Settings → Rhythm |

---

## Prerequisites

1. **Apple Developer Program** (paid) for TestFlight / App Store.
2. **Medousa Home** iOS app in App Store Connect with bundle ID `com.entasislabs.medousa-home`.
3. **APNs Auth Key** — [Developer → Keys](https://developer.apple.com/account/resources/authkeys/list):
   - Create key with **Apple Push Notifications service (APNs)** enabled.
   - Download `AuthKey_<KEY_ID>.p8` **once** (Apple does not let you re-download).
   - Note your **Team ID** (10 characters) and **Key ID** (10 characters).

---

## One-time Mac daemon setup (release engineering)

Run on each Mac host that will send push (or bake into your internal Mac provisioning image).

```bash
cd Medousa
./scripts/install-apns-push.sh \
  --team-id XXXXXXXXXX \
  --key-id YYYYYYYYYY \
  --key-file ~/Downloads/AuthKey_YYYYYYYYYY.p8
```

**What this does (macOS):**

1. Writes `config.json` to `{medousa_data_dir}/apns/` (default: `~/Library/Application Support/medousa/apns/`).
2. Stores the `.p8` PEM in **Keychain** — not on disk.
3. Sets directory permissions (`apns/` `700`, `config.json` `600`).

**Production (App Store) builds:**

```bash
./scripts/install-apns-push.sh \
  --team-id XXXXXXXXXX \
  --key-id YYYYYYYYYY \
  --key-file ~/Downloads/AuthKey_YYYYYYYYYY.p8 \
  --production
```

**Linux / file fallback** (no Keychain):

```bash
./scripts/install-apns-push.sh \
  --team-id XXXXXXXXXX \
  --key-id YYYYYYYYYY \
  --key-file /secure/AuthKey_YYYYYYYYYY.p8 \
  --file-storage
```

Restart the daemon:

```bash
medousa start daemon --public
```

Confirm in logs:

```text
home push: APNs configured (keychain)
```

or `(data dir file)` when using `--file-storage`.

---

## iOS app build requirements

Before each TestFlight / App Store upload:

1. **Push Notifications** capability on the iOS app target (Xcode → Signing & Capabilities).
2. Correct **bundle ID** and **Team** in `apps/medousa-home/src-tauri/tauri.conf.json`.
3. Build and upload per [MOBILE-DEV.md §12](../../apps/medousa-home/MOBILE-DEV.md).

| Build channel | APNs environment | `sandbox` in config |
|---------------|------------------|---------------------|
| Dev / `tauri ios dev` | Sandbox | `true` |
| TestFlight | Sandbox | `true` (default) |
| App Store | Production | `false` (`--production`) |

---

## CI / secrets handling

**Do not** commit `.p8` files or real `config.json` to git (`config/apns/.gitignore` blocks them).

Recommended CI pattern:

1. Store `AuthKey_*.p8` in your secrets manager (GitHub Actions secret, 1Password, etc.).
2. On the release Mac or provisioning step, run `install-apns-push.sh` with the decrypted key file.
3. Delete the temporary `.p8` from CI workspace after install.
4. Never embed the key inside the iOS IPA or the Medousa Installer PKG.

**Dev override** (local only — env vars beat file/keychain):

```bash
export MEDOUSA_APNS_TEAM_ID="XXXXXXXXXX"
export MEDOUSA_APNS_KEY_ID="YYYYYYYYYY"
export MEDOUSA_APNS_KEY_PATH="$HOME/Downloads/AuthKey_YYYYYYYYYY.p8"
export MEDOUSA_APNS_SANDBOX=true
```

---

## End-user experience (no Apple setup)

1. Install Medousa Home (TestFlight or App Store).
2. Mac: official Medousa / daemon running with APNs already configured.
3. Pair: **You → Settings → Connection** → Mac LAN URL (`http://<mac-ip>:7419`).
4. Enable **Remote push** (Settings → Rhythm) and allow iOS notifications.
5. Background or quit the app — pushes arrive when work state changes on the Mac.

---

## Security model

| Topic | Behavior |
|-------|----------|
| APNs publisher key | Keychain on macOS; optional file storage on Linux |
| Paired device tokens | Encrypted at rest in daemon pairing store |
| Revoke pairing (`DELETE /pair/{id}`) | Loopback (`127.0.0.1`) without token, or remote with that device's `Authorization: Bearer` session token |
| LAN transport | HTTP today — pairing tokens can be sniffed on hostile Wi‑Fi; use trusted networks |
| Push content | Work card titles in notification body — visible on lock screen |

Admin remove from the Mac:

```bash
medousa pair list
medousa pair remove <full-pairing-uuid>
# defaults to http://127.0.0.1:7419 (loopback admin)
```

---

## Verification checklist

- [ ] `install-apns-push.sh` run on Mac; daemon log shows APNs configured
- [ ] iOS app has Push Notifications entitlement
- [ ] Phone paired; heartbeat succeeds (connection green)
- [ ] Remote push enabled in app settings; iOS permission granted
- [ ] Force-quit app; complete work on Mac → push received
- [ ] TestFlight: `sandbox: true`; App Store: `sandbox: false` / `--production`

---

## Troubleshooting

| Symptom | Check |
|---------|--------|
| No push, daemon log says APNs not configured | Run install script; restart daemon; verify `config.json` exists |
| Push in dev but not TestFlight | Same sandbox key; confirm bundle ID matches |
| Push in TestFlight but not App Store | Re-run install with `--production` |
| `home push: APNs configured` but no delivery | Phone token registered? Heartbeat with `apnsDeviceToken`? Firewall? |
| Keychain read fails on headless Mac | Unlock login keychain or use `--file-storage` with locked-down permissions |

---

## Related docs

- [MOBILE-DEV.md §9 Remote push](../../apps/medousa-home/MOBILE-DEV.md) — developer + user summary
- [mobile-and-lan.md](../cookbook/mobile-and-lan.md) — pairing flow
- [http-api.md](../engine/http-api.md) — pairing HTTP routes
- `config/apns/config.example.json` — config schema
