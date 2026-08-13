# Mobile & LAN

**Audience:** operator, integrator

Connect the Medousa mobile app to a desktop engine over LAN (or Iroh when enabled).

---

## Prerequisites

- Desktop: `medousa_daemon` running (app bundle or `medousa start daemon`)
- Iroh pairing configured (recommended), or a private isolated LAN used only
  for a short development pairing ceremony
- No internet/guest-network exposure of port **7419**

---

## Pairing flow

1. Desktop exposes `GET /qr` (and `/qr/image` for PNG).
2. Mobile scans QR or enters pair code (`GET /pair/code`).
3. `POST /pair/init` + `POST /pair/verify` exchange credentials.
4. Mobile stores workshop URL + bearer token; uses [`medousa-sdk-iroh`](../../crates/medousa-sdk-iroh/) `WorkshopTransport` via Tauri `daemon/sdk.rs`.

Routes: [http-api.md](../engine/http-api.md#pairing-lan--phone)

---

## iPhone development (Mac)

Full walkthrough: [`MOBILE-DEV.md`](../../apps/medousa-home/MOBILE-DEV.md)

```bash
cd apps/medousa-home
npm install
npm run tauri ios init   # once
npm run tauri ios dev
```

---

## Non-loopback bind (temporary unsafe development escape hatch)

`--public` currently publishes the complete daemon router, uses permissive
CORS, and does not require authentication on every personal-mode route.
Pairing a client does not close the anonymous surface. Until
[hardening H01](../../architecture/hardening/README.md) ships:

- prefer the full Iroh invite flow in the [phone pairing guide](../guides/phone-pairing.md);
- never expose port 7419 to the internet, guest Wi-Fi, or an untrusted LAN; and
- if LAN-only development requires a non-loopback bind, use an isolated network
  and firewall, keep the window short, and stop/restart on loopback immediately.

Unsafe development command:

```bash
medousa start daemon --public
```

Set `MEDOUSA_DAEMON_PUBLIC_URL` so mobile clients resolve the correct host (see connection runbook).

Env vars: [configuration-reference.md](../configuration-reference.md)

---

## Mobile shell

At viewport ≤768px: **Pulse**, **Work**, **Chat**, **You**.

Library → **Notes** | **Artifacts**. Artifacts open fullscreen with safe-area chrome.

App integrator doc: [medousa-home.md](../apps/medousa-home.md)

---

## Transport stack

Mobile Tauri → `medousa-sdk` `Transport` → LAN HTTP with auth → optional Iroh failover.

[SDK transports](../sdk/transports.md) · [connection-reliability](../runbooks/connection-reliability.md)
