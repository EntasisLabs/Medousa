# Peers & Nearby

**Audience:** Medousa app users sharing workshops on a LAN or over a tunnel.

Medousa treats the **daemon as the app**. Phones and other desktops are surfaces
that connect with credentials. Two relationships matter:

| Role | Meaning | Where |
|------|---------|--------|
| **portal** | Full client of that brain | Workshop switcher |
| **peer** | Message / inbox that brain | **Peers** rail |

Same pairing crypto. Different scope.

---

## Quick path — add a peer

1. Open **Peers** (Users icon under Chat on desktop; **More → Peers** on mobile).
2. Tap **+** / **Add peer**.
3. Show the QR on the host; scan from the other device (same Wi‑Fi for first pair).
4. Prefer turning **LAN pairing** on only while pairing (**Settings → Nearby**),
   then **off** again.

Already-paired clients keep working over the private tunnel when you’re away
from the LAN.

---

## Phone as portal

That’s **Settings → Phone**, not Peers. See [Phone pairing](phone-pairing.md).

---

## Deep dive

Full portal vs peer, Iroh desktop-as-client, compact QR vs full link, and Nearby
toggle details:

**[Peers, portals, and LAN sharing](../cookbook/lan-discovery-and-sharing.md)**

Operator / mobile build notes: [Mobile & LAN](../cookbook/mobile-and-lan.md).
