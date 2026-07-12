# Phone pairing

**Audience:** people who want iOS or Android as a **portal** into a desktop
engine — not a second brain.

Your phone is a window. The Mac/PC (or a workshop host) runs Medousa Engine.

---

## Before you pair

1. Desktop Medousa is open and the engine is healthy (**Settings → Connection**).
2. Phone and desktop are on the **same trusted Wi‑Fi** for first pair (café Wi‑Fi
   is a bad idea).
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

---

## What you can do on the phone

- Chat with the same brain
- Browse vault / library surfaces the shell exposes
- Switch workshops you’ve paired as portals (workshop switcher)

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
| Pairing fails | Turn on **LAN pairing** briefly under Settings → Nearby, pair, then turn it **off** |
| Phone offline later | Confirm desktop engine is running; tunnel needs the host up |
| Push / Live Activities | Operator setup: [mobile push runbook](../runbooks/mobile-push-deployment.md) |

More operator detail: [Mobile & LAN](../cookbook/mobile-and-lan.md).
