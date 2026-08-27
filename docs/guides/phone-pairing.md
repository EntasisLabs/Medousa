# Phone pairing

**Audience:** people who want iOS or Android to connect to another Medousa
workshop.

The phone's Personal workshop remains its own brain. Pairing adds a separate
portal you may switch to; it does not replace Personal or merge their data.

---

## Before you pair

1. Desktop Medousa is open and the engine is healthy (**Settings → Connection**).
2. For a compact QR, phone and host are on the **same trusted Wi‑Fi** for first
   pair (café Wi‑Fi is a bad idea). An off-LAN host can instead issue a full Iroh
   invite as described below.
3. Install the Medousa companion from TestFlight / store when available, or a
   dev build ([mobile-and-lan cookbook](../cookbook/mobile-and-lan.md)).

---

## Pair from Settings → Phone

1. On desktop: **Settings → Phone**.
2. Show the QR / invite.
3. On the phone: scan the QR or paste the invite link.
4. Accept — the phone joins as a **portal** to that workshop.

After pairing, you can leave the LAN pairing window off. Already-paired clients
keep working over the private tunnel (Iroh) when you’re off the LAN.

Opening the LAN pairing window binds the daemon to the LAN, but application and
invite-management routes still require credentials. Only the bounded
`/pair/init` and `/pair/verify` ceremony is anonymous. Use a trusted network for
compact LAN invites, close the window when finished, and never expose port 7419
directly to the internet. Prefer the full Iroh invite below when off-LAN.

## Pair with a VPS or other off-LAN host

Keep the daemon on loopback; port 7419 does not need to be exposed publicly.
On the host, print a full v2 invite containing the Iroh ticket:

```bash
medousa start daemon-restart
medousa pair qr --full
```

Copy the complete `medousa://pair/2.0?...` URL. In the mobile app, choose the
pair/join-workshop flow and paste that full URL. The initial pairing ceremony
and subsequent workshop traffic use Iroh, so the phone does not need to reach
the host's LAN or public IP address.

---

## What you can do on the phone

- Chat and work in the phone's independent Personal workshop
- Switch into a paired portal to work directly with that workshop
- Browse vault / library surfaces the shell exposes

## Optional delegated work from Personal

Delegation is separate from pairing and portal selection:

1. On the receiving workshop, explicitly allow **Run delegated work** for the
   paired phone in **Settings → Sharing**.
2. On the phone, keep Personal selected. Open the paired workshop's edit
   actions under **Settings → Connection**, then choose **Use for delegated
   work**.
3. Use **Stop delegated work** there to revoke the phone daemon's binding.

Creating the binding sends no work and does not switch workshops. When the
Personal agent delegates, it sends only bounded context to that exact paired
identity; the signed result returns with provenance while both stores and
session catalogs stay independent.

You do **not** install offline brain packages on the phone — do that on the host
via [Packages](packages.md).

---

## Peers vs phone portal

| | Phone portal | Peer |
|--|--------------|------|
| Scope | Full client of that workshop | Inbox / share with another brain |
| Where | Settings → Phone, workshop switcher | **Peers** rail |
| Guide | This page | [Peers & Nearby](peers-and-nearby.md) |

Same crypto family; different product scope.

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| QR won’t scan | Move closer; use **Copy link** / full invite if off-LAN |
| Pairing fails | Prefer a full Iroh invite. If an isolated trusted LAN is the only option, open **LAN pairing** only for the ceremony and turn it off immediately. |
| Phone offline later | Confirm desktop engine is running; tunnel needs the host up |
| Push / Live Activities | Operator setup: [mobile push runbook](../runbooks/mobile-push-deployment.md) |

More operator detail: [Mobile & LAN](../cookbook/mobile-and-lan.md).
