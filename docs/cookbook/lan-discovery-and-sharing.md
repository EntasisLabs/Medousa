# Peers, LAN discovery, and sharing

Connect to other Medousa workshops on your network, message them asynchronously, and share notes, artifacts, and canvas layouts.

## Peers surface (primary)

Open **Peers** in the Life rail (Users icon).

### My invite

Your workshop shows a large QR, short code, and **Copy link** (`medousa://pair/…`). Others on the same Wi‑Fi can connect without typing codes. A **Visible on network** pill means mDNS advertising is active.

### One-tap Connect

Nearby workshops appear via mDNS (`_medousa._tcp.local.`). Tap **Connect**:

1. Home fetches their unauthenticated `GET /qr` over LAN
2. Completes the existing Ed25519 trust ceremony
3. Adds them to your trusted people list

No paste required on the same network. If auto-connect fails, a fallback sheet accepts their invite link (daemon URL prefilled).

### Inbox

Select a person to see messages and compose (optional note/artifact attachment). Unread count badges the Peers rail icon.

**Replies** require mutual trust: after you connect, they should open Peers and tap Connect on your name.

## Settings → Nearby

Advanced only:

- **Open Peers** CTA
- Revoke trusted workshops
- Full canvas **share bundle** export / import / push

Phone pairing (portal to this brain) stays under **Settings → Phone**.

## Share bundles and single items

Share bundles (`.medousa-share.json`) include artifacts, vault notes, and optional environment sections.

- **Peers / Vault / Artifact menus** — share one note or artifact to a peer
- **Settings → Nearby** — export/import full canvas bundles

Daemon:

- `GET /v1/lan/workshops` — mDNS browse
- `POST /v1/share/export` / `import` / `push`
- `POST/GET /v1/peer/messages`, mark-read, unread-count

## Capability bits

| Bit | Name | Behavior |
|-----|------|----------|
| 3 | `file_push` | Share push/import |
| 4 | `brain_sync` | Environment + vault in bundles |
| 5 | `relay_capable` | Iroh transport available |

## Out of scope

- Camera QR scan (link paste + one-tap LAN is v1)
- Live typing / presence
- Agent-to-agent protocols
- Merging external channel Messaging into Peers
