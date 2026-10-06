# Phone pairing

**Audience:** people who want a mobile Personal workshop to connect to another
Medousa workshop.

On iPhone and Android, first-run setup creates the embedded Personal workshop
and can configure its own model provider. Pairing is not part of that critical
path. Pairing later adds a separate
portal you may switch to; it does not replace Personal or merge their data.

---

## Before you pair

1. Desktop Medousa is open and the engine is healthy (**Settings → Connection**).
2. For a compact QR, phone and host are on the **same trusted Wi‑Fi** for first
   pair (café Wi‑Fi is a bad idea). An off-LAN host can instead issue a full Iroh
   invite as described below.
3. Install Medousa from TestFlight / the app store when available, or use a
   dev build ([mobile-and-lan cookbook](../cookbook/mobile-and-lan.md)).

---

## Pair from Settings → Phone

1. On desktop: **Settings → Phone**.
2. Show the QR / invite.
3. On the phone: open **Settings → Connection**, choose to add a workshop, then
   scan the QR or paste the invite link.
4. Choose a **Workshop name**, or leave it blank to use the host's name.
   Scanned QR codes and opened invite links show the same form as pasted links.
5. Choose **Join workshop** — the phone joins as a **portal** to that workshop.
   You can switch to it now or keep using your current workshop.

After pairing, you can leave the LAN pairing window off. Already-paired clients
keep working over the private tunnel (Iroh) when you’re off the LAN.

Pairing trusts the device, not one forever-lived bearer token. Medousa keeps
short-lived sessions in its secret store and renews them by proving possession
of the device key created during pairing. Closing the app, changing networks,
or letting a session expire does not require another QR scan.

When a launch or foreground health check cannot reach the workshop, Medousa
retries automatically while the app is visible. It rechecks the network route
and resumes the selected conversation and streams when the workshop returns.
A missed health check never opens a dialog over your chat. Loaded messages stay
readable and scrollable, and you can keep editing your draft while reconnection
runs in the background. A quiet **Reconnecting…** line near the composer offers
**Retry** and **Connection settings**; sending resumes once connected.
A dropped background stream reconnects from its saved revision without a
health-check preflight or reloading the screen. Retry delays grow until stream
data confirms that the connection has recovered.

If a paired connection becomes stale, open the workshop menu and choose
**Refresh connection**, or use **Refresh** on the active workshop in
**Settings → Connection**. Medousa checks the route and saved credentials,
reloads current workshop data, and reconnects its streams. Your selected
workshop, conversation draft, open notes, tabs, and running work stay in place.
If the host is offline, bring it back online and refresh again. Revoked or
expired device trust still requires pairing again.

Under **Settings → Phone**, expand a paired device to choose its trust policy:

- **Until removed** (default) keeps the device trusted until you choose
  **Forget device**.
- A fixed expiration works like a personal access token expiration.
- **Expire if unused** can remove trust after 7, 30, or 90 days without a
  successful connection.

Revocation and either expiration policy stop session renewal immediately. The
device must be paired again unless an administrator extends its policy.
Saving a policy also expires that device's current access session; its next
connection must prove the paired device key before receiving a new one.

Opening the LAN pairing window binds the daemon to the LAN, but application and
invite-management routes still require credentials. Only the bounded
`/pair/init` and `/pair/verify` ceremony is anonymous. Session challenge and
refresh routes also accept no bearer because they recover expired sessions, but
they issue nothing without a valid one-time challenge signed by the paired
device key. Use a trusted network for compact LAN invites, close the window when
finished, and never expose port 7419 directly to the internet. Prefer the full
Iroh invite below when off-LAN.

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

## Pair from a browser

A browser tab can join the same way. The tab keeps its own Personal workshop.
Pasting a full invite adds a portal to your private daemon; it does not upload
that daemon's files into the browser.

1. On the workshop, print a full invite: `medousa pair qr --full`.
2. Open Medousa in the browser.
3. Add a workshop and paste the complete `medousa://pair/2.0?...` link.
4. Switch to that workshop. Chat, sessions, and the rest of Home talk to the
   private daemon over Iroh.

A compact LAN link has no Iroh ticket, so it still belongs on the Medousa app
while you are on the same network. Switch back to Personal when you want the
workshop that lives in the browser. The pairing session stays in this site's
storage and renews with the device key created during pairing.

---

## What you can do on the phone

- Chat and work in the phone's independent Personal workshop
- Switch into a paired portal to work directly with that workshop
- Browse vault / library surfaces the shell exposes

## Optional delegated work from Personal

Delegation is separate from pairing and portal selection:

1. Connect to the receiving workshop, open **Settings → Sharing → Phone**, open
   the paired device, and choose its **Allowed on this workshop** permission.
   **Connected only** grants no execution; **Assistant work** grants the bounded
   assistant lane. Sandboxed, approved-project, and custom scopes remain
   independently selectable and never imply host shell, governed browser or
   computer access, or secrets. To let explicitly delegated work use a browser
   or desktop owned by this workshop, choose **Custom**, enable **Browser &
   computer worlds**, and keep only the other scopes you intend to grant. Once
   allowed, the workshop appears in Medousa's Browser world selector and under
   **Settings → Runtime Controls → Computer** when it advertises the matching
   driver. An open browser world or computer view remains bound to that exact
   workshop; changing the default does not move it.

   A custom policy that grants **Assistant work**, **Agent targeting**, and
   **Host shell** is treated as owner-level machine trust. External Codex,
   Cursor, and Hermes assignments are then launched directly from their exact,
   immutable handoff snapshot; Medousa does not require another approval card
   for every assignment. Policies without host shell continue to create an
   approval card before an external agent starts.
2. On the phone, keep Personal selected. Open the paired workshop's edit
   actions under **Settings → Connection**, then choose **Use for delegated
   work**.
3. Use **Stop delegated work** there to revoke the phone daemon's binding.

Creating the binding sends no work and does not switch workshops. When the
Personal agent delegates, it sends only bounded context to that exact paired
identity; the signed result returns with provenance while both stores and
session catalogs stay independent. Pairing by itself grants no execution, and
removing the pair prevents an old per-device policy from applying if that
device is paired again later.

### Grant access on a headless workshop

The same directional permissions are available without the desktop app. Run
these commands on the workshop host:

```bash
medousa pair permissions list
medousa pair permissions set <device-id> --preset assistant-work
```

Use the full device id printed by `permissions list`. Changes apply
immediately; the daemon does not need to restart. To revoke delegated work
while keeping the device paired:

```bash
medousa pair permissions set <device-id> --preset connected-only
```

The CLI also accepts `sandboxed-work`, `approved-projects`, and `custom`.
Approved-project access requires one or more `--project <project-id>` values.
Custom policies expose explicit scope flags in
`medousa pair permissions set --help`; omitted custom scopes remain denied.

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
