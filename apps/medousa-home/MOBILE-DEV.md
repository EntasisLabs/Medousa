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

On the Mac, bind the daemon to all interfaces so the phone can reach it:

```bash
# from repo root
./scripts/mobile-dev-daemon.sh
```

Or manually:

```bash
medousa start daemon --bind 0.0.0.0:7419
# foreground debug:
# cargo run -p medousa --bin medousa_daemon -- --bind 0.0.0.0:7419
```

Note your Mac’s Wi‑Fi IP:

```bash
ipconfig getifaddr en0
# e.g. 192.168.1.42
```

**Firewall** — allow incoming TCP **7419** on the Mac (System Settings → Network → Firewall).

Health check from the Mac:

```bash
curl -s "http://127.0.0.1:7419/health"
curl -s "http://$(ipconfig getifaddr en0):7419/health"
```

---

## 6. Run on the iPhone

USB-connect the phone (or use a simulator). From `apps/medousa-home`:

```bash
npm run tauri ios dev
```

First run opens Xcode signing if needed — pick your **Personal Team** on the app target.

When the app launches:

1. Open **You → Settings → Connection**
2. Set daemon URL to `http://<MAC_LAN_IP>:7419` (e.g. `http://192.168.1.42:7419`)
3. Confirm **Connected** / green health

`tauri ios dev` runs Vite on your Mac and hot-reloads the webview on device — keep the Mac awake on the same Wi‑Fi as the phone.

### Useful variants

```bash
npm run tauri ios dev -- --open          # open Xcode project
npm run tauri ios dev -- --device <id>   # specific device (tauri ios dev --help)
npm run tauri ios build                  # release IPA for TestFlight-style install
```

---

## 7. Environment shortcuts

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_DAEMON_URL` | Default connection in the Tauri shell (set before `ios dev` if you want a baked-in URL) |
| `APPLE_DEVELOPMENT_TEAM` | Code signing team for `ios init` / `ios dev` |
| `MEDOUSA_MOBILE_BIND` | Override daemon bind (default `0.0.0.0:7419`) in `scripts/mobile-dev-daemon.sh` |

Example:

```bash
export MEDOUSA_DAEMON_URL="http://192.168.1.42:7419"
npm run tauri ios dev
```

---

## 8. Deep links on device

Custom scheme: `medousa://work/<card-id>` (configured in `tauri.conf.json`).

After install, notification taps and links should route into the Work card. Test from Notes or Safari:

```
medousa://work/<paste-card-id>
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| **Offline / connection failed** | Daemon running? `--bind 0.0.0.0:7419`? Mac firewall? Same Wi‑Fi? URL uses LAN IP not `127.0.0.1`? |
| **ATS / cleartext HTTP blocked** | `tauri.conf.json` includes `NSAllowsLocalNetworking` for iOS. Re-run `ios dev` after config changes. |
| **Code signing errors** | Xcode → Accounts → Apple Development cert; set team in Xcode project under `gen/apple`. |
| **Blank webview / dev server** | Phone must reach Mac Vite port (default **1420**). `tauri ios dev` usually handles this; check firewall. |
| **Tray / desktop-only APIs** | Mobile build skips system tray; app icon badge still updates for blocked work. |

---

## Quick checklist

- [ ] Mac: Xcode + CocoaPods + Rust iOS targets
- [ ] `npm install` in `apps/medousa-home`
- [ ] `npm run tauri ios init` (once)
- [ ] Daemon: `./scripts/mobile-dev-daemon.sh`
- [ ] iPhone: Developer Mode on, USB trusted
- [ ] `npm run tauri ios dev`
- [ ] Settings → Connection → `http://<mac-ip>:7419`

Desktop dev is unchanged: `npm run tauri dev` with daemon on `127.0.0.1:7419`.
