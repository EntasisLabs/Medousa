# LAN discovery and workshop sharing

Discover other Medousa workshops on your local network, trust them with the same Ed25519 pairing ceremony used for phones, share canvas layouts / notes / artifacts, and exchange async inbox messages.

## Discover nearby workshops

When Medousa Engine is running on your LAN, it advertises `_medousa._tcp.local.` via mDNS with TXT metadata:

- `dv` — device id prefix
- `pn` — peer display name
- `pf` — capability bitfield (`file_push`, `brain_sync`, relay)
- `ar` — auth required flag

Open **Settings → Nearby** to browse the network. The list refreshes every few seconds while the panel is open.

Daemon endpoint: `GET /v1/lan/workshops`

## Trust another workshop

Trust is **not** phone pairing — it reuses the same crypto, but stores credentials as a **paired workshop** in your workshop registry.

1. On the remote workshop, generate a pair invite (`GET /qr` or Settings → Phone QR).
2. On your machine, open **Settings → Nearby**, click **Trust** on the discovered row (or paste the `medousa://pair/…` link).
3. Complete the ceremony — your app stores a bearer session token for cross-workshop HTTP.

Revoke trust from the **Trusted workshops** list.

**Mutual trust for replies:** you can send messages and share items one-way as soon as *you* trust them. For them to reply, they must also trust you (each side completes the trust ceremony).

## Single note / artifact share

Share one vault note or one presentation artifact to a trusted peer:

- **Settings → Nearby** — “Share a note…” / “Share an artifact…”
- **Artifact menu** (Share → Share to peer…) when trusted peers exist
- **Vault note actions** — Share to peer…

Flow: local mini-bundle export (`POST /v1/share/export` with one id/path) → `POST /v1/share/push` on the peer with bearer auth.

Conflict strategy: rename (default), skip, or overwrite.

## Share bundles

Share bundles (`.medousa-share.json`) are versioned JSON payloads:

- `artifacts` — HTML presentation artifacts (base64)
- `vaultNotes` — note paths + markdown content
- `environment` — custom surfaces, components, and layout presets

### Export / import locally

**Settings → Nearby → Share bundle**

- **Export bundle** downloads JSON (includes custom canvas views when checked).
- **Import file** applies the bundle with a conflict strategy: rename, skip, or overwrite duplicates.

Daemon endpoints:

- `POST /v1/share/export`
- `POST /v1/share/import`
- `GET /v1/share/capabilities`

### Push to a trusted peer

After exporting (or reusing the last bundle in-session), choose a trusted workshop and **Push bundle**. The app sends `POST /v1/share/push` to the peer over LAN HTTP (Iroh fallback when configured) using the stored bearer token.

Remote imports require a valid pairing session token unless the request originates from loopback.

## Peer inbox (async messaging)

Trusted workshops can send short text messages (optionally with a note/artifact attachment). Delivery is push-based and async — no live typing or presence.

| Method | Path | Who | Behavior |
|--------|------|-----|----------|
| POST | `/v1/peer/messages` | Peer (bearer) or loopback | Append inbound message; auto-import attachment with rename |
| GET | `/v1/peer/messages` | Local only | List inbox (newest first); `?unreadOnly=true` |
| POST | `/v1/peer/messages/{id}/read` | Local only | Mark read |
| GET | `/v1/peer/messages/unread-count` | Local only | Badge count |

Messages persist in `peer_inbox.json` (cap 500). Unread count badges **Settings → Nearby**.

Compose from Nearby: pick peer, write text, optionally attach a note or artifact (mini-bundle).

## Capability bits

| Bit | Name | Behavior |
|-----|------|----------|
| 3 | `file_push` | Share bundle / item push/import |
| 4 | `brain_sync` | Environment + vault sections in bundles |
| 5 | `relay_capable` | Iroh transport available |

## Out of scope

- Agent-to-agent messaging
- Continuous sync / CRDT replication
- Live typing / presence
- Desktop Iroh off-LAN (LAN + mobile Iroh only)
- Push notifications for peer messages
- Sharing chat runtime state
