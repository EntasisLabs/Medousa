# Normie onboarding + LAN pairing

> **Status:** Approved for implementation — Phase A next  
> **Date:** 2026-06-11  
> **Epic target:** Phase 1 LAN Magic (4–6 weeks)  
> **Supersedes:** informal `strategy/medousa-normie-onboarding-strategy.md` references (strategy doc not yet in repo)  
> **Related:** [medousa-home-plan.md](medousa-home-plan.md), [medousa-home-mobile-plan.md](medousa-home-mobile-plan.md), [medousa-home-product-ux-plan.md](medousa-home-product-ux-plan.md), [durable-turn-worker-plan.md](durable-turn-worker-plan.md), [embedded-local-inference-plan.md](embedded-local-inference-plan.md), [component-daemon.md](component-daemon.md)

## Product promise

**Download → chat in 90 seconds, zero terminal.**

A normie opens Medousa Home, picks how the brain thinks, optionally pairs a phone on the same Wi‑Fi, and lands in chat. No `medousa setup`, no daemon jargon, no JSON files.

Marketing line: *Your second brain — always here, always yours, always private.*

The daemon already idles at ~27 MB with full orchestration (Stasis, durable workers, identity). Onboarding must feel as light as that footprint.

---

## Locked decisions (reconciled from both specs)

These resolve conflicts between the pairing protocol draft and the first-run wizard draft.

| Topic | Decision | Rationale |
|-------|----------|-----------|
| QR payload | **`medousa://pair/1.0?a=…&d=…&t=…&s=…&n=…`** (URL, not JSON) | One format for wizard, Settings, CLI, mobile |
| Signing | **Ed25519** (long-term daemon key + ephemeral QR session key) | Pairing spec; drop wizard draft’s ECDSA |
| HTTP surface | **Same axum app** as today’s daemon — new `/pair/*` + `/qr` routes | One firewall hole; mDNS TXT `port` matches bind |
| Default port | **`7419`** (`DEFAULT_DAEMON_PORT`) unless overridden | Already in `daemon_api.rs`; wizard draft’s `4737` is wrong |
| LAN bind | **`--public`** → `0.0.0.0:{port}` for pairing; Home stays on localhost IPC | Reuse `detect_lan_ipv4()` + `resolve_mobile_client_daemon_url()` |
| Config files | **Extend existing on-disk layout** — no silent `config.toml` migration in v1 | `product_config.json`, `tui_defaults.json`, new `wizard.json` |
| UI terminology | **“Medousa Core”** in wizard copy — never “daemon”, “SurrealDB”, “backend” | Wizard spec brand voice |
| Garage wizard | **Separate** from product first-run wizard | M8f `garageOnboarding` is Vault-only localStorage |
| Session after restart | **Pairing identity persists**; bearer token refreshes via stored keys | Avoid “scan QR every reboot” |
| Managed AI / cloud auth | **Screen 2 stub OK for v1 prod** — Skip must work | BYOM + Ollama + offline path ship without Medousa Cloud |

---

## Architecture overview

```mermaid
flowchart TB
    subgraph first_run [First-run wizard — Home Tauri]
        S1[Screen 1: How should I think?]
        S2[Screen 2: Account optional]
        S3[Screen 3: Pair phone optional]
        DONE[Completion → chat]
    end

    subgraph core [Medousa Core — medousa_daemon]
        AXUM[axum HTTP :7419 or --public]
        PAIR[/pair/* /qr /qr.png]
        AGENT[Agent runtime + Stasis]
        MDNS[mDNS _medousa._tcp]
        STORE[(~/.medousa/pairings/ encrypted)]
        ID[(~/.medousa/identity/ Ed25519)]
    end

    subgraph phone [Phone app future]
        BROWSE[Bonjour browse]
        SCAN[QR scan]
        HB[Heartbeat /pair/heartbeat]
    end

    S1 -->|persist tui_defaults + secrets| AXUM
    S2 -.->|Phase E cloud| AXUM
    S3 -->|GET /qr poll| PAIR
    S3 --> DONE
    BROWSE --> MDNS
    SCAN --> PAIR
    HB --> PAIR
    PAIR --> STORE
    PAIR --> ID
    MDNS --> AXUM
```

**Layering:** Wizard is the product front door. Pairing is the wire protocol. Screen 3 and Settings → Phone both consume **`GET /qr`** — Home never mints its own crypto.

---

## Part 1 — First-run GUI wizard

Replaces TUI `medousa setup` for normie installs. TUI wizard remains for power users (`src/bin/medousa/onboard_wizard/`).

### Three screens

| # | Screen | Purpose | Skip? |
|---|--------|---------|-------|
| 1 | **Welcome + How should I think?** | Provider / model selection | No — must pick one path |
| 2 | **Create account** | Medousa Cloud (managed AI, relay, sync) | Yes |
| 3 | **Add your phone** | QR + short code LAN pairing | Yes |

**Entry points**

1. **Fresh install** — no `wizard.json` with `state = completed` → full wizard.
2. **Settings → Re-run wizard** — IPC `rerun_wizard`; pre-fill from existing config.
3. **Upgrade migration** — `tui_defaults.json` exists but no wizard state → one-screen “Welcome to the new Medousa” splash, then Home (Screens 2–3 only from Settings).

### Screen 1 — Model paths

| Path | Default | v1 ship |
|------|---------|---------|
| **A — Recommended (Managed AI)** | Highlighted card | Stub: “Coming soon” or waitlist; **offline default = Gemma 4 12B** via [embedded-local-inference-plan.md](embedded-local-inference-plan.md) |
| **B — Bring your own model** | OpenAI, Anthropic, Gemini, Ollama | **P0** — probe Ollama `:11434`, validate key via lightweight API call, store in keyring |
| **C — Offline** | **Gemma 4** (tier-sized: E2B / E4B / **12B Unified**) | **P0** — Core download + embedded engine; Ollama optional fallback |

**Extract from TUI wizard (shared Rust, not ratatui UI):**

- Provider detection and key validation
- `persist_tui_defaults` / secrets keyring pattern (`medousa.tui` / `api_key`)
- Daemon service install templates (`launchd` / systemd)

**Do not port:** Surreal backend picker, channel hub (Discord/Telegram/…), daemon bind prompts — power-user surfaces stay in TUI or Settings → Basement.

### Screen 2 — Account (optional)

Magic link + Apple/Google SSO → `medousa://auth/callback?token=…` (deep link plugin already in Home).

**On skip:** `account.status = skipped` — all local features work; banner in Settings only.

**Blocked until cloud exists:** managed AI provisioning, multi-device relay away from home LAN.

### Screen 3 — Phone (optional)

- Poll daemon **`GET /qr`** (30s or WebSocket `pairing_success` event).
- Show SVG/PNG QR, countdown (~5 min token), short code fallback (`M3D-0US-A` style).
- States: waiting → connected → failed/retry.
- **I'll do this later** → `pairing.status = skipped`; re-enter from Settings → Phone.

Pre-flight via IPC: Core running, mDNS registered (when enabled), identity key present. If Core down: “Starting Medousa Core…” spinner → `daemon_start` IPC.

### Completion

- Verify Core via `daemon_health` loop (2s × 15).
- Install auto-start: macOS LaunchAgent, Linux systemd user unit, Windows service or Run key.
- Open main Home chat — composer focused, “Ask me anything…”

### Wizard state machine

```
NO_WIZARD → WIZARD_ACTIVE → SCREEN_1 → SCREEN_2 → SCREEN_3 → COMPLETED → HOME_READY
```

**Persistence:** `~/.local/share/medousa/wizard.json` (platform data dir via `dirs`):

```json
{
  "state": "completed",
  "completedAt": "2026-06-11T19:00:00Z",
  "screen1Model": "byok",
  "screen2Skipped": true,
  "screen3Skipped": false,
  "migrationFrom": null
}
```

Provider choice continues in existing `tui_defaults.json` + `product_config.json` — wizard does not duplicate provider schema.

### Window UX

- First launch: fixed 640×720, frameless, non-resizable, 300ms crossfade between screens.
- Settings re-entry: modal 520×600.
- `prefers-reduced-motion`: instant transitions.
- Keyboard: full Tab loop; Enter continue; Escape back where allowed.

### Error states (summary)

| Condition | Behavior |
|-----------|----------|
| No network | Screen 1: grey Managed AI; highlight Offline/BYOM. Screen 2: skip-only. Screen 3: QR still works (LAN). |
| Core failed to start | Retry / View logs / Skip — Home shows “Core offline” + Start button |
| QR generation failed | Text pairing code fallback — never hard-block |
| Magic link timeout (5 min) | Resend + skip |
| Provider probe all fail | Try again / Use offline |

---

## Part 2 — LAN pairing protocol

Phones discover and trust the desktop Core on the local network. Implements TOFU with Ed25519 and layered discovery.

### 2.1 Bonjour / mDNS service

**Service type:** `_medousa._tcp.local.`  
(Prototype fallback: `_medousa-local._tcp` if unregistered type rejected; register `_medousa` at IANA before wide prod.)

**TXT record schema** (UTF-8 strings; booleans `"1"` / `"0"`):

| Key | Req | Description |
|-----|-----|-------------|
| `dv` | Yes | Device id — first 8 hex chars of SHA-256(long-term Ed25519 public key) |
| `pn` | Yes | Peer name from Home preferences (1–64 chars), e.g. `"Alley's Studio"` |
| `pv` | Yes | Protocol version `"1.0.0"` |
| `pf` | No | 16-bit capability hex bitfield (see below) |
| `ar` | Yes | Auth required `"1"` / `"0"` |
| `vp` | No | Voice portal available |
| `md` | No | Model descriptor, e.g. `llama3.2:3b` |

**Capability bitfield `pf` (4 hex chars):**

| Bit | Name |
|-----|------|
| 0 | `pairing_v1` |
| 1 | `web_transcript` |
| 2 | `voice_push` |
| 3 | `file_push` |
| 4 | `brain_sync` |
| 5 | `relay_capable` (Phase 2 cloud) |
| 6–15 | Reserved |

Keep TXT **< 400 bytes** (conservative mDNS limit).

**Visibility**

| State | mDNS |
|-------|------|
| Core running, default | Advertise |
| “Hide from pairing” toggled | Stop advertise; optional unpair all |
| Core quitting | Goodbye packet TTL=0 |
| Network change | Re-register after ~1s delay (RFC 6762) |

**Instance name conflict:** append ` (2)` when human name collides but `dv` differs; warn if same `dv` duplicate.

### 2.2 QR pairing URL

```
medousa://pair/1.0?a=HOST:PORT&d=DEVICE_ID&t=BASE64URL_TOKEN&s=BASE64URL_SIG&n=NAME
```

| Param | Description |
|-------|-------------|
| `a` | `host:port` — IPv6 as `[::1]:8080` |
| `d` | 8-char device id (matches TXT `dv`) |
| `t` | 32-byte one-time token, base64url — **ephemeral QR session public key** |
| `s` | Ed25519 signature of `a\|d\|t` by **long-term** daemon key |
| `n` | URL-encoded display name (optional) |

**QR rendering:** ECC level M, byte mode, 4-module quiet zone, optional 15% center logo, countdown overlay, dark mode invert (contrast ≥ 4.5:1).

**Short code fallback:** 6 chars from alphabet `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, grouped `M3D-0US-A`. Rotates every 2 min or on use. Verify via mDNS resolve + TCP with challenge.

**Token lifecycle**

| Duration | Expires |
|----------|---------|
| 5 min | QR on screen — auto-refresh in Home |
| 2 min | Short code |
| 1 use | Token + code invalidated on successful `/pair/init` |
| Core restart | Pending tokens cleared; **paired devices kept** |

### 2.3 Pairing HTTP API

Mount on existing daemon router (`medousa_daemon.rs`):

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/pair/status` | Paired devices + QR active flag |
| `GET` | `/qr` | JSON: `url`, `expires_at`, `short_code` |
| `GET` | `/qr.png` | PNG image |
| `GET` | `/pair/code` | Current short code |
| `POST` | `/pair/init` | Start pairing with QR token |
| `POST` | `/pair/verify` | Complete mutual nonce exchange |
| `GET` | `/pair/heartbeat` | Phone presence |
| `DELETE` | `/pair/{pairing_id}` | Revoke pairing |

**Handshake (summary)**

```
Phone → POST /pair/init { qr_token, phone_id, phone_name, public_key }
Core  → 200 { status: challenge, server_nonce, session_id }
Phone → POST /pair/verify { session_id, signed_nonce, phone_nonce }
Core  → 200 { status: paired, session_token, pairing_id, server_signed_nonce }
```

Race: first `/pair/init` wins; second gets **409** (token single-use). Verify must complete within **10s** of init.

### 2.4 Security model

**Keys**

| Key | Lifetime | Storage |
|-----|----------|---------|
| Daemon long-term Ed25519 | Install | `~/.medousa/identity/ed25519_sk` — device id = first 8 bytes SHA-256(pk) |
| QR session Ed25519 | 5 min / one QR render | Ephemeral in memory |
| Phone long-term Ed25519 | Install | Keychain / EncryptedSharedPreferences |

**Replay prevention:** single-use QR token; mutual nonces; rate-limit `/pair/init` to 3/min/source IP; session bearer scoped to `pairing_id`, rotate every 24h.

**LAN eavesdropper:** attacker scanning QR first gets challenge but cannot verify without phone’s long-term key; mDNS spoofing fails without valid QR signature.

**Pairing store:** `~/.medousa/pairings/{phone-id}.json` encrypted at rest (XChaCha20-Poly1305 with long-term key). Revocation list in `revoked.json`.

### 2.5 Phone app (future client)

- Browse `_medousa._tcp` every 60s foreground.
- Green dot = paired + reachable; grey = discovered only.
- QR scan when mDNS blocked (corporate Wi‑Fi).
- Heartbeat every 30s; 3 misses → disconnected; re-browse on failure.

---

## Part 3 — Integration points

| Surface | Consumes |
|---------|----------|
| Wizard Screen 3 | `GET /qr`, `pairing_success` event, `daemon_health` |
| Settings → Phone | Same QR modal + paired device list + Forget |
| Settings → Account | Screen 2 re-entry (Phase E) |
| Settings → Model | Inline Screen 1 path picker |
| `medousa pair` CLI | `/pair/status`, terminal QR (`--term`), `--code` |
| Mobile Pulse tab | Heartbeat + session token (post-pair) |

**Tauri events (new)**

| Event | Payload |
|-------|---------|
| `pairing_success` | `{ deviceName, deviceId }` |
| `daemon_status_change` | `{ status, message }` |
| `model_download_progress` | `{ percent, bytes, total }` (Phase D offline) |
| `auth_session_ready` | `{ user }` (Phase E) |

---

## Code inventory

### Exists today (reuse)

| Area | Path |
|------|------|
| Daemon HTTP | `src/bin/medousa_daemon.rs` — axum router |
| LAN IP detection | `src/daemon_api.rs` — `detect_lan_ipv4`, `resolve_public_daemon_bind` |
| TUI setup logic | `src/bin/medousa/onboard_wizard/` |
| Home daemon IPC | `apps/medousa-home/src-tauri/src/daemon/mod.rs` — `daemon_health`, `daemon_url` |
| Config persistence | `apps/medousa-home/src-tauri/src/medousa_paths.rs` — `tui_defaults`, `product_config` paths |
| Secrets keyring | `apps/medousa-home/src-tauri/src/messaging/secrets.rs` |
| Deep link plugin | `apps/medousa-home/src-tauri/src/lib.rs` — `tauri_plugin_deep_link` |
| Garage onboarding (separate) | `apps/medousa-home/src/lib/utils/garageOnboarding.ts` |

### To build

| Area | Path / module |
|------|----------------|
| Pairing core | `src/pairing/` — identity, QR mint, store, crypto |
| Pairing handlers | `src/pairing_handlers.rs` or `src/bin/medousa_daemon/pairing.rs` |
| mDNS advertise | `src/pairing/mdns.rs` — `mdns-sd` or platform Bonjour |
| Wizard IPC | `apps/medousa-home/src-tauri/src/commands/wizard.rs`, `providers.rs`, `bonjour.rs` |
| Auth IPC (Phase E) | `apps/medousa-home/src-tauri/src/commands/auth.rs` |
| Wizard UI | `apps/medousa-home/src/lib/components/wizard/*.svelte` |
| CLI | `medousa pair list \| qr \| add \| remove \| status` |

**New dependencies (daemon):** `ed25519-dalek`, `mdns-sd` (or `mdns`), `qrcode`, `chacha20poly1305` (pairing file encryption).

---

## Implementation phases

### Phase A — Pairing daemon (P0, ~1 week) — **In progress (core landed)**

**Goal:** curl-testable pairing; unblocks wizard Screen 3 and Settings.

- [x] `~/.medousa/identity/` long-term Ed25519 + stable `dv`
- [x] `/qr`, `/qr.png`, `/pair/init`, `/pair/verify`, `/pair/status`, `/pair/code`
- [x] Encrypted pairing store + revoke
- [x] mDNS advertise when `--public` or `MEDOUSA_PAIRING_ADVERTISE=1`
- [x] Env flags: `MEDOUSA_MDNS_DISABLE`, `MEDOUSA_PAIRING_DISABLE`, `MEDOUSA_PAIRING_ADVERTISE`
- [x] Integration tests: happy path, replay 409, handshake (unit)
- [x] `medousa pair status` + `medousa pair qr --term`

**Exit:** `curl localhost:7419/qr` returns signed URL; `dns-sd -B _medousa._tcp` sees service when public.

### Phase B — Wizard shell + migration (~3–4 days) — **Landed (shell)**

**Goal:** First-run gate without full Screen 1 logic.

- [x] `wizard.json` state machine (`src-tauri/src/wizard.rs`)
- [x] `WizardContainer.svelte` + routing + crossfade
- [x] Fresh install detection + upgrade migration splash
- [x] Settings → Re-run wizard
- [x] Link doc in architecture README ✅

**Exit:** Fresh VM opens wizard; completed state skips to Home; migration from TUI install shows splash once.

### Phase C — Screen 1 BYOM path (~4 days) — **Landed**

**Goal:** 90-second path without cloud.

- [x] `providers_probe` + `providers_validate_key` Tauri IPC (extract from TUI)
- [x] WelcomeScreen.svelte — three cards; BYOM expand with Ollama auto-detect
- [x] Wire `persist_tui_defaults` + keyring save
- [x] `daemon_start` IPC + Core health wait loop
- [x] CompletionScreen → main chat

**Exit:** Clean macOS VM: pick Ollama or paste OpenAI key → chat works, no terminal.

### Phase D — Screen 3 + Settings phone (~3 days) — **Landed**

**Goal:** QR in wizard and Settings.

- [x] PhonePairScreen.svelte — poll `/qr`, countdown, short code, pairing_success
- [x] Settings Phone panel — pair / forget / device list
- [x] `bonjour_status` IPC
- [x] Network diagnostics link (pairing troubleshooting copy)

**Exit:** Phone simulator or curl harness completes pair; wizard skip + Settings re-entry works.

### Phase E — Screen 2 cloud auth (~1 week, can trail v1)

**Goal:** Managed AI + account when Medousa Cloud API exists.

- [ ] `send_magic_link`, `exchange_auth_token`, `auth_status`
- [ ] `medousa://auth/callback` handler
- [ ] AccountScreen.svelte + Settings → Account upgrade
- [ ] Recommended path fully wired

**Exit:** Magic link E2E in staging; skip still works.

### Phase F — Polish + prod packaging (~1 week)

- [ ] Offline model download progress (optional — Ollama may suffice v1)
- [ ] launchd / systemd / Windows auto-start from wizard completion
- [ ] Accessibility pass (keyboard, screen reader, contrast)
- [ ] Manual dogfood checklist (see below)
- [ ] Release notes + IANA service name registration

---

## Testing strategy

### Automated (Phase A+)

```bash
cargo test pairing --lib          # crypto, token expiry, replay
cargo test pairing_handlers       # axum handler integration
# CI: MEDOUSA_MDNS_DISABLE=1 medousa_daemon --dev-pairing
pytest tests/test_pairing.py      # harness from spec (optional Python)
```

### P0 scenarios

| # | Scenario |
|---|----------|
| 1 | Full QR handshake → paired + session token |
| 2 | Same QR twice → second 409 |
| 3 | Heartbeat after pair → 200 |
| 4 | Heartbeat after Core restart → reconnect with stored identity |
| 5 | Revoke → 401 revoked |
| 6 | mDNS browse finds Core (manual / dns-sd) |
| 7 | mDNS blocked → QR direct IP still pairs |
| 8 | Wizard BYOM path → chat without terminal |
| 9 | Wizard skip account + skip phone → full local features |
| 10 | Upgrade from TUI-only install → migration splash once |

### Manual dogfood

- [ ] Fresh install macOS VM — no terminal
- [ ] Kill network mid-wizard — graceful degradation
- [ ] Kill Core mid-pair — retry UX
- [ ] Keyboard-only wizard completion
- [ ] Screen reader on all wizard screens

---

## CLI reference (target)

```bash
medousa pair list
medousa pair qr --term
medousa pair qr --open
medousa pair add --code M3D0USA
medousa pair add --address 192.168.1.42:7419
medousa pair remove <pairing_id>
medousa pair status --verbose
```

Debug: `--debug-pairing`, `--debug-mdns`, `MEDOUSA_PAIRING_DISABLE_TLS=1` (dev only).

---

## Deferred / out of scope (this epic)

| Item | Notes |
|------|-------|
| Medousa Cloud relay (Phase 2) | `relay_capable` bit reserved |
| NFC / BLE pairing | Protocol v2 |
| TLS 1.3 on LAN | v1 plaintext + nonce exchange; TLS in v2 |
| X3DH / Signal-style | v2 |
| Bundled Gemma 2B download | Ship Ollama path first |
| Multi-user daemon on one machine | Single-user like Obsidian v1 |
| Phone app implementation | Separate epic; protocol ready in Phase A |
| IANA `_medousa._tcp` registration | Before wide prod marketing |

---

## Protocol evolution (v1 → v2)

| Feature | v1 (this epic) | v2 |
|---------|----------------|-----|
| Transport | Plain TCP on LAN | TLS 1.3 + cert fingerprint in TXT |
| Discovery | mDNS + QR + short code | + NFC + BLE |
| Key exchange | Ed25519 TOFU | X3DH forward secrecy |
| Session | Bearer 24h rotation | TLS PSK / 0-RTT |
| Presence | HTTP heartbeat | WebSocket push |

---

## Appendix A — Example `/qr` response

```json
{
  "url": "medousa://pair/1.0?a=192.168.1.42:7419&d=a1b2c3d4&t=Ab12...&s=Wx34...&n=Alley%27s%20Studio",
  "expires_at": "2026-06-11T19:05:00Z",
  "short_code": "M3D-0US-A"
}
```

## Appendix B — Example paired device record (decrypted)

```json
{
  "pairing_id": "uuid",
  "phone_id": "phone-hex-8",
  "phone_name": "Alley's Phone",
  "phone_public_key": "base64...",
  "paired_at": "2026-06-11T19:00:00Z",
  "last_seen": "2026-06-11T19:00:05Z",
  "session_token_hash": "sha256...",
  "session_token_expiry": "2026-06-12T19:00:00Z"
}
```

## Appendix C — Wizard copy anchors

**Screen 1 headline:** *I'm your second brain — always here, always yours, always private.*

**Screen 3:** *Talk to Medousa from anywhere in your home — your phone becomes a second portal to your brain.*

**Privacy (Screen 2):** *Your data stays yours. Creating an account only links your devices. Your conversations, memories, and manuscripts live on your machine unless you turn on cloud sync.*

---

*End of epic plan. Start Phase A (`src/pairing/`) when implementation begins.*
