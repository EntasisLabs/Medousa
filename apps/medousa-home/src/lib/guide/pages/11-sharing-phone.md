# Sharing and phone

Three different relationships: **phone portal** into *your* workshop, **peers** (other workshops on the LAN), and **Shared mode** seats on one brain. See [Architecture](guide:architecture#workshop-vs-peer-vs-phone) for the noun table.

Related: [Messaging channels](guide:messaging-channels) · [Getting started](guide:getting-started) · [Troubleshooting](guide:troubleshooting#phone-discovery)

## Phone companion

Desktop: Settings → **Sharing** → **Phone**, or the wizard’s optional phone step.

1. **Show pairing QR** — sheet notes it is generated now and **expires / refreshes** while open; countdown **Refreshes in…**; **Refresh QR**; short code copy.
2. Scan from the companion app (or use the pairing link / address).
3. Paired list shows device name and **Seat {profile}** when Shared; **Forget** revokes.

Phone first-run connects to a host — it does not install a separate Offline brain. Most model changes update the host.

Turn **Always reachable on Wi‑Fi** (or equivalent Sharing exposure) on when you actually use a companion. Leave it off on untrusted networks.

## Shared mode

Settings → Sharing → **Shared**:

- **Off** — personal hats as today.
- **On** — “Team seats on this brain — vault stays shared.” Seats via **Phone invites**; admin profile shown in meta.
- Desktop host required; older engines: not available.

With Shared on, Phone sheet can **Invite a seat** → pick profile → **Mint QR** (**Seat invite**). Chat **New shared room** also requires Shared mode.

## Peers

**Peers** surface — “People on your network.”

| Action | Meaning |
|--------|---------|
| Nearby **Connect** | Start trust with a discovered workshop |
| **Add peer** | Show your QR (“Visible on network”) |
| **Connect by address** | Workshop URL / optional name / invite |
| Inbox / threads | Messages; note/artifact attachments when offered |
| **Revoke** | Settings LAN share / peer trust controls |

Peers do **not** merge vaults. Canvas backup/send conflict policy (Rename / Skip / Overwrite) lives under Settings → Sharing → Nearby / LAN — see [Views and environments](guide:views-environments#backup-and-import).

## Pairing another computer

Paired portals appear under **Your workshops**. After trust, **Switch** puts Home on that engine. Vault and sessions follow the workshop — confirm before editing.

## Operator checklist

| Before… | Confirm… |
|---------|----------|
| Leaving with phone automations | Engine up; Sharing reachability |
| Pairing a new machine | You meant to trust that host |
| Phone can't see Home | Same network, workshop active, fresh QR |
| Shared rooms missing | Shared mode on |

```callout
tone: tip
title: Local first
body: Most days you only need This device. Add phone, peers, or Shared seats when a second portal earns a place at the bench.
```

Next: [Messaging channels](guide:messaging-channels) · [Workshops and connections](guide:workshops-connections).
