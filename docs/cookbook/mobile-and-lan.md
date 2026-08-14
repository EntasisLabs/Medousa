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

1. Authenticated desktop Home or `medousa pair qr` creates an expiring invite.
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

## Non-loopback bind

`--public` publishes the complete daemon router, but application routes require
local-app or paired credentials. QR/status/code/ticket operations are protected;
only the bounded pairing ceremony is anonymous. Browser origins and request
hosts are exact allowlists rather than wildcard CORS.

Prefer the full Iroh invite flow in the [phone pairing
guide](../guides/phone-pairing.md). Do not expose port 7419 directly to the
internet. Use a trusted LAN and firewall for compact invites, keep the pairing
window short, and stop/restart on loopback when finished.

LAN development command:

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
