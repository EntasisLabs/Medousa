# Medousa Home — iPhone dev (Mac)

Run the **native mobile shell** on a physical iPhone while the **Medousa daemon** stays on your Mac. Same Pulse / Work / Chat / You UI you have been polishing — not just Safari at a narrow width.

## Architecture

```
┌─────────────┐     Wi‑Fi LAN      ┌──────────────────┐
│  iPhone     │  HTTP/SSE :7419    │  Mac             │
│  Medousa    │ ─────────────────► │  medousa_daemon  │
│  Home app   │                    │  (+ LLM keys)    │
└─────────────┘                    └──────────────────┘
```

The phone cannot use `http://127.0.0.1:7419` — that is the phone itself. Point the app at your Mac’s LAN IP.

---

## 1. Clone on the Mac

```bash
git clone <your-remote> medousa
cd medousa
```

Build the Rust toolchain + daemon once (same as desktop):

```bash
# rustup — https://rustup.rs
cargo build -p medousa --bin medousa_daemon
# optional: cargo install --path . --bin medousa
```

Copy config from your Linux box if you already have keys and vault:

```bash
# Typical paths (adjust if you use a custom MEDOUSA_DATA_DIR)
rsync -av user@linux-host:~/.config/medousa/ ~/.config/medousa/
rsync -av user@linux-host:~/medousa-data/ ~/medousa-data/   # if applicable
```

---

## 2. One-time iOS prerequisites (Mac)

| Requirement | Install |
|-------------|---------|
| **Xcode** (full app) | Mac App Store → open once → accept license |
| **Command Line Tools** | `xcode-select --install` |
| **Homebrew** | https://brew.sh |
| **CocoaPods** | `brew install cocoapods` |
| **Rust iOS targets** | `rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios` |
| **Node 20+** | `brew install node` |

**Apple ID (free)** — Xcode → Settings → Accounts → add Apple ID → Manage Certificates → **Apple Development**. No paid Developer Program required for device dev.

**iPhone** — Settings → Privacy & Security → **Developer Mode** → On (iOS 16+). Trust the Mac when prompted via USB.

---

## 3. Frontend deps

```bash
cd apps/medousa-home
npm install
```

---

## 4. Initialize iOS target (once per clone)

From `apps/medousa-home`:

```bash
npm run tauri ios init
```

If prompted for a development team:

```bash
export APPLE_DEVELOPMENT_TEAM="<10-char team id>"   # Xcode → Account → Team ID
# or set bundle.iOS.developmentTeam in src-tauri/tauri.conf.json
```

This generates `src-tauri/gen/apple/` (machine-local; gitignored).

**If init fails on CocoaPods / brew outdated** — ensure `pod` and `brew` are on PATH; re-run init after `pod setup`.

---

## 5. Start the daemon for mobile

On the Mac, start the daemon in **public** mode — binds to all interfaces, detects your LAN IP, and prints the URL for the phone:

```bash
# from repo root (preferred)
medousa start daemon --public
```

Or use the thin wrapper (same thing):

```bash
./scripts/mobile-dev-daemon.sh
```

Foreground / debug:

```bash
medousa daemon --public
# or: cargo run -p medousa --bin medousa -- start daemon --public
```

The CLI prints something like:

```text
[ok] Mobile / LAN clients: http://192.168.1.42:7419
[info] Point Medousa Home → Settings → Connection at that URL on iPhone.
```

Use that URL in **You → Settings → Connection** on the phone. Custom port: `medousa start daemon --public --bind 0.0.0.0:7420`.

**Firewall** — allow incoming TCP **7419** on the Mac (System Settings → Network → Firewall).

Health check from the Mac:

```bash
curl -s "http://127.0.0.1:7419/health"
curl -s "http://$(ipconfig getifaddr en0):7419/health"
```

---

## 6. Run on simulator vs physical iPhone

From `apps/medousa-home`:

### Simulator first (recommended for daily dev)

When a phone is USB-connected, plain `tauri ios dev` **prefers the physical device**. Use the sim script or pass a simulator name explicitly:

```bash
npm run tauri:ios:dev:sim
# or any booted simulator by name / UDID:
npm run tauri:ios:dev -- "iPhone 16 Pro"
npm run tauri:ios:dev -- 59F67F16-0FF1-42CC-9122-A9CC52F27B6F
```

List simulators: `xcrun simctl list devices available | grep iPhone`

Simulator uses `localhost` for the Vite dev server — no LAN / firewall setup required.

### Physical iPhone

USB-connect the phone (or pick it explicitly). **Unplug the phone** if you only want the simulator and Tauri keeps choosing the device.

```bash
npm run tauri:ios:dev
# or force a specific device:
npm run tauri:ios:dev -- --host
```

First run opens Xcode signing if needed — pick your **Personal Team** on the app target.

When the app launches on device:

1. Open **You → Settings → Connection**
2. Set daemon URL to the **Mobile / LAN clients** URL printed by `medousa start daemon --public`
3. Confirm **Connected** / green health

`tauri ios dev` runs Vite on your Mac and hot-reloads the webview on device — keep the Mac awake on the same Wi‑Fi as the phone. If the app opens then goes blank, open `http://<mac-lan-ip>:1420` in Safari on the phone; if that fails, re-run with `npm run tauri:ios:dev -- --host` so Vite binds to the LAN address.

### Useful variants

```bash
npm run tauri:ios:dev -- --open          # open Xcode project
npm run tauri:ios:dev -- "iPhone 16"     # specific simulator
npm run tauri:ios:build                  # release IPA for TestFlight-style install
```

---

## 7. Environment shortcuts

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_DAEMON_URL` | Default daemon URL at **desktop** launch. On iPhone, set URL in **You → Settings → Connection** (saved in app data) or rely on dev auto-detect from the Vite host. |
| `APPLE_DEVELOPMENT_TEAM` | Code signing team for `ios init` / `ios dev` |
| `MEDOUSA_DAEMON_PUBLIC_URL` | Set automatically by `--public` so chat stream URLs use your Mac LAN IP (not `0.0.0.0`). Override only if auto-detect picks the wrong interface. |

Example:

```bash
export MEDOUSA_DAEMON_URL="http://192.168.1.42:7419"
npm run tauri ios dev
```

**Settings on mobile:** Connection URL is stored on the phone. Provider, model, and API keys live on the **Mac daemon** (`tui_defaults.json`). After connecting, the app loads runtime defaults from the daemon. Change model on the phone via **You → Runtime → Controls** (updates the workshop for all clients).

---

## 8. Live Activity (Lock Screen / Dynamic Island)

Medousa can show an **in motion** Live Activity while work is running. Requires **iOS 16.1+** and a one-time Widget Extension setup in Xcode.

### Source layout (checked into git)

| Path | Role |
|------|------|
| `src-tauri/ios-live-activity/Shared/` | `MedousaWorkAttributes` — shared by app + widget |
| `src-tauri/ios-live-activity/App/` | ActivityKit start/update/end bridge (linked from Rust) |
| `src-tauri/ios-live-activity/Widget/` | Lock Screen + Dynamic Island SwiftUI |

### One-time setup

After `npm run tauri:ios:init`:

```bash
Live Activity native bridge + push entitlements are applied automatically by `npm run ios:prepare` (runs before `tauri:ios:dev` / `tauri:ios:build` / after `tauri:ios:init`).
```

Then in Xcode (`src-tauri/gen/apple/*.xcodeproj`):

1. **File → New → Target → Widget Extension**
   - Name: `MedousaWorkWidget`
   - **Include Live Activity:** yes
   - Include Configuration App Intent: no
2. Replace the generated widget entry with files from `src-tauri/ios-live-activity/Widget/`
3. Add `Shared/MedousaWorkAttributes.swift` to **both** the main app target and the widget target
4. **Signing & Capabilities → App Groups** on both targets: `group.com.entasislabs.medousa-home`

The Rust bridge compiles `App/` + `Shared/` Swift automatically during `tauri ios build` via `build.rs`.

### Toggle

**You → Settings → Rhythm → Live Activity** (on by default). Syncs from workspace state while the app is foregrounded.

### Verify

1. Connect to daemon, start a work card (`in_flight`)
2. Background the app — Live Activity should show on Lock Screen / Dynamic Island
3. Tap the activity — should deep-link via `medousa://work/<card-id>`

---

## 9. Remote push (Mac daemon → iPhone)

When your phone is paired, the Mac daemon can send **APNs** notifications for work finished, blocked cards, budget approval, and worker start — even when the app is closed.

### For users (no Apple Developer setup)

1. Install **Medousa Home** from TestFlight or the App Store (official builds include push capability).
2. On your Mac, run the official Medousa installer or daemon — APNs credentials are bundled for Entasis builds.
3. Pair your phone (**You → Settings → Connection**).
4. On iPhone: **You → Settings → Rhythm → Remote push** (on by default).
5. Accept the iOS notification permission prompt on first launch.

You do **not** need Team ID, Key ID, or `.p8` files. Those are publisher credentials, installed once on the Mac by Entasis release engineering.

The app registers its APNs device token with the daemon on each pairing heartbeat (`POST /pair/heartbeat`).

### For release engineering (one-time Mac setup)

Create an APNs Auth Key (`.p8`) in [Apple Developer → Keys](https://developer.apple.com/account/resources/authkeys/list), then install into the daemon data directory:

```bash
cd Medousa
./scripts/install-apns-push.sh \
  --team-id XXXXXXXXXX \
  --key-id YYYYYYYYYY \
  --key-file ~/Downloads/AuthKey_YYYYYYYYYY.p8
```

This writes:

```text
~/Library/Application Support/medousa/apns/config.json
~/Library/Application Support/medousa/apns/AuthKey_YYYYYYYYYY.p8
```

Restart the daemon. Logs show `home push: APNs configured (data dir file)` when ready.

Use `--production` for App Store builds (default is sandbox for dev/TestFlight). See `config/apns/config.example.json` for the schema.

Full release checklist: [mobile-push-deployment.md](../../../docs/runbooks/mobile-push-deployment.md).

On macOS the install script stores the `.p8` in **Keychain** (`medousa.apns`); only metadata stays in `config.json`.

**Development override** — env vars take precedence over the data-dir file:

```bash
export MEDOUSA_APNS_TEAM_ID="XXXXXXXXXX"
export MEDOUSA_APNS_KEY_ID="YYYYYYYYYY"
export MEDOUSA_APNS_KEY_PATH="$HOME/Downloads/AuthKey_YYYYYYYYYY.p8"
export MEDOUSA_APNS_BUNDLE_ID="com.entasislabs.medousa-home"
export MEDOUSA_APNS_SANDBOX=true
```

### Xcode (Push Notifications capability)

Enable **Push Notifications** on the iOS app target (adds `aps-environment`). Required for device tokens in dev and release builds.

### Test

1. Pair phone, confirm heartbeat succeeds (connection stays green).
2. Start work on the Mac, background or force-quit Medousa on the phone.
3. When a card finishes or blocks, you should receive a push notification.

Push payload includes `cardId`, `kind`, and `url` (`medousa://work/<id>`) for deep linking when the app opens.

---

## 10. Deep links on device

Custom scheme: `medousa://work/<card-id>` (configured in `tauri.conf.json`).

After install, notification taps and links should route into the Work card. Test from Notes or Safari:

```
medousa://work/<paste-card-id>
```

---

## 11. App icons

Source art lives in the repo at `Medousa/assets/`:

| File | Use |
|------|-----|
| `medousa-blk.png` | **Default app icon** (1024×1024, dark background — matches Black Lily) |
| `medousa-cream.png` | Light-background variant |
| `medousa-transparent.png` | Logo only (avoid for iOS home screen — Apple rejects transparency) |

Regenerate every platform size (desktop, iOS `AppIcon.appiconset`, Android, favicon):

```bash
cd apps/medousa-home
npm run icons:generate
```

This reads `app-icon.json` → `medousa-blk.png` and writes:

- `src-tauri/icons/` — bundle icons referenced in `tauri.conf.json`
- `src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/` — iOS home-screen sizes
- `static/favicon.png` — web favicon

To swap art temporarily, edit `app-icon.json` or pass another PNG:

```bash
npx tauri icon ../../assets/medousa-cream.png -o src-tauri/icons --ios-color "#f5f0e8"
```

After changing icons, rebuild iOS (`npm run tauri:ios:build:testflight`) — `tauri ios dev` hot-reload does **not** refresh the home-screen icon.

---

## 12. TestFlight install (first time)

TestFlight is Apple’s beta channel. You need the **paid Apple Developer Program** ($99/yr). A free Personal Team works for USB dev (`tauri ios dev`) but **not** for TestFlight.

### One-time Apple setup

1. **Enroll** — [developer.apple.com/programs](https://developer.apple.com/programs/) (same Apple ID as Xcode).
2. **App Store Connect** — [appstoreconnect.apple.com](https://appstoreconnect.apple.com) → **Apps** → **+** → **New App**.
   - Platform: iOS  
   - Name: `Medousa Home` (or your display name)  
   - Bundle ID: **`com.entasislabs.medousa-home`** (must match `identifier` in `src-tauri/tauri.conf.json`)  
   - SKU: any unique string, e.g. `medousa-home`
3. **Xcode signing** — Xcode → Settings → Accounts → your team → **Download Manual Profiles** (or let automatic signing handle it on first build).
4. **Team ID** — already in `tauri.conf.json` as `bundle.iOS.developmentTeam` (`K5SZ28RN9P`). Change only if you use a different team.

### Bump version before each upload

Apple rejects duplicate **build numbers**. Edit `src-tauri/tauri.conf.json`:

```json
"version": "0.1.0"
```

For each TestFlight upload, pass a new build number (timestamp is fine):

```bash
npm run tauri:ios:build:testflight
# or explicitly (must fit in 0..4294967295 — unix seconds works):
npm run tauri ios build -- --export-method release-testing --build-number $(date +%s) --ci
```

Or use the helper script (runs frontend build + iOS export):

```bash
./scripts/ios-testflight-build.sh
# BUILD_NUMBER=2 ./scripts/ios-testflight-build.sh   # optional override
```

**Output IPA:**

```text
apps/medousa-home/src-tauri/gen/apple/build/Medousa Home.ipa
```

Export methods (Tauri `--export-method`):

| Method | Purpose |
|--------|---------|
| `release-testing` | **TestFlight** beta (what you want) |
| `app-store-connect` | App Store / Connect upload variant |
| `debugging` | Dev install on registered devices only (not TestFlight) |

### Upload to App Store Connect

**Option A — Transporter (easiest)**

1. Install **Transporter** from the Mac App Store.
2. Sign in with your Apple Developer Apple ID.
3. Drag `Medousa Home.ipa` into Transporter → **Deliver**.
4. Wait for processing (usually 5–20 minutes).

```bash
open -a Transporter "src-tauri/gen/apple/build/Medousa Home.ipa"
```

**Option B — Xcode Organizer**

1. `npm run tauri ios build -- --open` (or open `src-tauri/gen/apple/medousa-home.xcodeproj`).
2. Window → Organizer → Archives → Distribute App → App Store Connect.

### Enable TestFlight testers

1. App Store Connect → your app → **TestFlight** tab.
2. When the build finishes processing, answer **Export Compliance** (typically “No” for encryption beyond standard HTTPS unless you added custom crypto).
3. **Internal testing** — instant for up to 100 team members on your App Store Connect team.
4. **External testing** — add emails; first build needs a short Beta App Review.

Testers install the **TestFlight** app from the App Store, accept your invite, then install **Medousa Home**.

### On the phone after TestFlight install

Same as dev: daemon on Mac with `medousa start daemon --public`, then **You → Settings → Connection** → `http://<mac-lan-ip>:7419`. TestFlight builds are release — no Vite dev server on :1420.

### TestFlight troubleshooting

| Symptom | Fix |
|---------|-----|
| **No valid signing identity** | Xcode → Accounts → manage certificates; confirm `developmentTeam` in `tauri.conf.json`. |
| **Bundle ID not found in Connect** | Create the app in App Store Connect with exact bundle ID before uploading. |
| **Duplicate build number** | Run `npm run tauri:ios:clean` then rebuild; confirm printed `CFBundleVersion` before upload. |
| **Same version after bumping config** | Stale cache — `npm run tauri:ios:clean` (or `CLEAN=1 ./scripts/ios-testflight-build.sh`). If still stale: `rm -rf src-tauri/gen/apple && npm run tauri:ios:init`, then rebuild. |
| **Upload stuck / invalid IPA** | Rebuild with `--export-method release-testing`, not `debugging`. |
| **Processing forever** | Check email from Apple for compliance/metadata issues. |
| **Old icon on home screen** | Delete app, reinstall from TestFlight after `npm run icons:generate` + rebuild. |

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| **Offline / connection failed** | Daemon running? `medousa start daemon --public`? Mac firewall? Same Wi‑Fi? URL uses LAN IP not `127.0.0.1`? In dev, the app auto-sets daemon URL from the Vite host (e.g. `http://10.x.x.x:7419`). |
| **ATS / cleartext HTTP blocked** | `tauri.conf.json` includes `NSAllowsLocalNetworking` for iOS. Re-run `ios dev` after config changes. |
| **Code signing errors** | Xcode → Accounts → Apple Development cert; set team in Xcode project under `gen/apple`. |
| **Blank / white webview** | Phone must reach Mac Vite on **1420** (open `http://<mac-ip>:1420` in Safari on the phone). Allow **1420** in Mac firewall. Re-run `npm run tauri ios dev` after config changes. On device, try `npm run tauri ios dev -- --force-ip-prompt` and pick the phone’s TUN address if LAN IP fails. iOS uses only the **main** window — desktop `chat-popout` is excluded from mobile builds. |
| **Chat fails / stream URL** | Restart with `medousa start daemon --public` (sets LAN stream URLs). Old daemons bound to `0.0.0.0` without `--public` return unreachable stream URLs. |
| **Wrong model on mobile** | Mobile reads provider/model from the Mac daemon after connect — not local `tui_defaults`. Use **You → Runtime → Controls** or edit `tui_defaults.json` on the Mac. |

---

## Quick checklist

- [ ] Mac: Xcode + CocoaPods + Rust iOS targets
- [ ] `npm install` in `apps/medousa-home`
- [ ] `npm run tauri ios init` (once)
- [ ] `npm run icons:generate` (after icon art changes)
- [ ] Daemon: `medousa start daemon --public`
- [ ] iPhone: Developer Mode on, USB trusted
- [ ] `npm run tauri ios dev`
- [ ] Settings → Connection → `http://<mac-ip>:7419`

**TestFlight (paid Apple Developer):**

- [ ] App created in App Store Connect (`com.entasislabs.medousa-home`)
- [ ] `npm run tauri:ios:build:testflight`
- [ ] Upload `src-tauri/gen/apple/build/Medousa Home.ipa` via Transporter
- [ ] TestFlight → add testers → install on iPhone

Desktop dev is unchanged: `npm run tauri dev` with daemon on `127.0.0.1:7419`.
