# Peers, portals, and LAN sharing

Medousa treats the **daemon as the app**. Phones, desktops, and Peers are **surfaces** that connect with credentials. There are two different relationships:

| Role | Meaning | Where it appears | Access |
|------|---------|------------------|--------|
| **portal** | You are a full client of that brain | Workshop switcher | Chat, vault, canvas, work, … |
| **peer** | You can message that brain | Peers only | Inbox + share bundles |

Same crypto and transport (QR, bearer, Iroh). Different **scope**.

## Desktop as a client (Iroh)

Home on **desktop** uses the same transport as mobile: try LAN, then fall back to the **Iroh ticket** saved at pair time. A laptop can join your Mac mini as a **portal** (full client) or **peer** (inbox only) and keep working off-LAN without binding the host to the public internet.

## LAN pairing window

Public bind (`0.0.0.0`) is only for **pairing**, not ongoing access.

1. Turn on **LAN pairing window** (Settings → Nearby, or Peers → Add peer)
2. Engine restarts listening on the LAN (mDNS + `GET /qr`)
3. Pair phones / peers / laptop on trusted Wi‑Fi
4. Turn the toggle **off** — engine restarts on **loopback only**
5. Already-paired clients keep working over the **private Iroh tunnel**

Do not leave LAN pairing on at a café.

## Peers surface (peer role)

Open **Peers** in the Life rail (Users icon, under Chat).

### My invite

Large **compact** QR (`medousa://pair/1.0?…`, camera-friendly), short code, **Copy link**. The Iroh ticket is fetched after LAN pair so the QR stays scannable. Use **Full link** only when pasting an off-LAN invite (`GET /qr?full=true` / v2).

### Mobile

**More → Peers** — two sections when portal-paired to the **active workshop**:

- **Workshop inbox** — same sudo view as desktop host (everyone who connected to your workshop)
- **My peer connections** — outbound peer connects from this phone

If the phone only has peer connections (no active portal workshop), you see **My peer connections** only.

Portal phone pairing (QR / Iroh) is for chat, vault, canvas **and** workshop inbox when that workshop is active in the switcher.

**Mobile Home button** — Settings → Canvas → Mobile Home button (Native Home or a custom view). Opening a custom view from More is temporary; tap Home again to leave it.

### One-tap Connect

Nearby workshops appear via mDNS. Tap **Connect**:

1. Fetch their `GET /qr` over LAN
2. Complete the trust ceremony with **`role: peer`**
3. Store credentials as a **peer** entry (`peer-{deviceId}`) — **not** a workshop switcher membership

If mDNS misses them, use **Connect by address** and enter their workshop URL (`http://10.12.0.13:7419`) — same as `medousa peer connect`. Invite link is optional (only if `/qr` is unreachable).

Peer tokens on the host are scoped: they may only call `/v1/peer/*`, `/v1/share/*`, and pairing heartbeat/status. Escalation to vault/chat is rejected with 403.

### Inbox

Peers splits into two sections when you have **Workshop inbox** access (host desktop or **portal** on the active workshop):

| Section | Who sees it | Scope |
|---------|-------------|--------|
| **Workshop inbox** | Host engine or portal on active workshop | Everyone who connected **to you** as a peer — reply **as the workshop** |
| **My peer connections** | Any surface | Workshops **you** connected to — one thread each, send **as you** |

**Peer** role (Connect / `medousa peer connect`): single thread with that host only.

**Portal** role (phone pairing / workshop switcher): sudo on that workshop — full inbox on the active workshop, same as desktop host.

Select a person to message (optional note/artifact attachment). Unread badges the Peers rail icon. Opening a thread marks it read; use ⋯ → **Mark read** to clear manually.

Local notifications fire when new inbound peer messages arrive (poll-based; respects Settings notification preference).

Threads show **both sides**: messages they sent you and messages you sent them (outbound copies stay on your workshop).

**People who connected to you** (CLI / another Home) appear as *Connected to you*. You can reply immediately — your reply is stored on this host and they pick it up with `medousa peer inbox` (or their Peers surface).

**People you Connected to** get live delivery when online, and Home also pulls their replies from their host over LAN/Iroh so one-way Connect is enough for a full thread.

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

## Headless CLI

Host (this engine):

```bash
medousa pair lan on          # bind 0.0.0.0 for pairing
medousa pair qr --term       # show invite
medousa pair status          # surfaces + roles
medousa pair lan off         # back to loopback; clients use Iroh
```

Client (another machine or the same host connecting out):

```bash
medousa peer nearby                              # mDNS browse via local engine
medousa peer connect http://192.168.1.20:7419    # role=peer (inbox only)
medousa peer connect http://192.168.1.20:7419 --portal --name mini
medousa peer list
medousa peer send mini "hello from headless"
medousa peer inbox --unread                      # this engine's inbox
medousa peer read <message-id>
```

Credentials live under `{medousa_data_dir}/cli/connections.json`. Peer tokens stay scoped to inbox + share on the remote host. Portal tokens see the remote workshop's **full** peer inbox (same as host loopback).

## Out of scope

- Camera QR scan
- Live typing / presence
- Agent-to-agent protocols
- Daemon-to-daemon relay (enterprise mesh)
- Auto-promoting a peer to a portal membership
- Full portal chat/vault over CLI (use Home or `medousa tui` / workspace APIs)
