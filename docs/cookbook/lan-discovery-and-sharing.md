# LAN discovery and workshop sharing

Discover other Medousa workshops on your local network, trust them with the same Ed25519 pairing ceremony used for phones, and share canvas layouts, vault notes, and artifacts as a versioned bundle.

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

## Capability bits

| Bit | Name | Behavior |
|-----|------|----------|
| 3 | `file_push` | Share bundle push/import |
| 4 | `brain_sync` | Environment + vault sections in bundles |
| 5 | `relay_capable` | Iroh transport available |

## Out of scope

- Agent-to-agent messaging
- Continuous sync / CRDT replication
- Sharing chat runtime state
