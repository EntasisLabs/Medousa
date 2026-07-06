# Mobile & LAN

**Audience:** operator, integrator

Connect the Medousa mobile app to a desktop engine over LAN (or Iroh when enabled).

---

## Prerequisites

- Desktop: `medousa_daemon` running (app bundle or `medousa start daemon`)
- Phone and desktop on same LAN, or Iroh pairing configured
- Firewall allows port **7419** (or your custom bind)

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

## Public bind

For phone access when not on same interface:

```bash
medousa start daemon --public
```

Set `MEDOUSA_DAEMON_PUBLIC_URL` so mobile clients resolve the correct host (see connection runbook).

Env vars: [configuration-reference.md](../configuration-reference.md)

---

## Mobile shell

At viewport ≤768px: **Pulse**, **Work**, **Chat**, **You**.

Library → **Notes** | **Presentations**. Presentations open artifacts fullscreen with safe-area chrome.

App integrator doc: [medousa-home.md](../apps/medousa-home.md)

---

## Transport stack

Mobile Tauri → `medousa-sdk` `Transport` → LAN HTTP with auth → optional Iroh failover.

[SDK transports](../sdk/transports.md) · [connection-reliability](../runbooks/connection-reliability.md)
