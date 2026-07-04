# Peers, portals, and LAN sharing

Medousa treats the **daemon as the app**. Phones, desktops, and Peers are **surfaces** that connect with credentials. There are two different relationships:

| Role | Meaning | Where it appears | Access |
|------|---------|------------------|--------|
| **portal** | You are a full client of that brain | Workshop switcher | Chat, vault, canvas, work, … |
| **peer** | You can message that brain | Peers only | Inbox + share bundles |

Same crypto and transport (QR, bearer, Iroh). Different **scope**.

## Peers surface (peer role)

Open **Peers** in the Life rail (Users icon, under Chat).

### My invite

Large QR, short code, **Copy link** (`medousa://pair/…`). Others on the same Wi‑Fi can connect without typing codes.

### One-tap Connect

Nearby workshops appear via mDNS. Tap **Connect**:

1. Fetch their `GET /qr` over LAN
2. Complete the trust ceremony with **`role: peer`**
3. Store credentials as a **peer** entry (`peer-{deviceId}`) — **not** a workshop switcher membership

Peer tokens on the host are scoped: they may only call `/v1/peer/*`, `/v1/share/*`, and pairing heartbeat/status. Escalation to vault/chat is rejected with 403.

### Inbox

Select a person to message (optional note/artifact attachment). Unread badges the Peers rail icon.

**Replies** require mutual peer trust: they must Connect to you from their Peers surface.

Revoke a peer from Peers (⋯ → Remove) — that device’s peer credentials only.

## Workshop membership (portal role)

Phone pairing and **Join workshop** (Settings / workshop switcher) use **`role: portal`**.

- Full client of that Medousa
- Appears in the workshop switcher
- Legacy `paired` entries migrate to `portal` on load

Compromised phone: revoke that portal pairing without removing peer relationships (and vice versa).

## Settings → Nearby

Advanced only:

- **Open Peers** CTA
- Peer revoke list
- Canvas **share bundle** export / import / push (peer credentials)

Phone portal QR stays under **Settings → Phone**.

## Share bundles

- **Peers / Vault / Artifact menus** — share one note or artifact to a **peer**
- **Settings → Nearby** — full canvas bundle export/import

Daemon:

- `GET /v1/lan/workshops` — mDNS browse
- `POST /v1/share/export` / `import` / `push`
- `POST/GET /v1/peer/messages`, mark-read, unread-count
- Pair init accepts `role`: `portal` | `peer`

## Capability bits

| Bit | Name | Behavior |
|-----|------|----------|
| 3 | `file_push` | Share push/import |
| 4 | `brain_sync` | Environment + vault in bundles |
| 5 | `relay_capable` | Iroh transport available |

## Out of scope

- Camera QR scan
- Live typing / presence
- Agent-to-agent protocols
- Daemon-to-daemon relay (enterprise mesh)
- Auto-promoting a peer to a portal membership
