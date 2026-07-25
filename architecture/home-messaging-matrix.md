# Home messaging matrix

One-page lock for how Medousa Home (desktop/mobile) handles messages across doors.

**Rule:** Home is a surface. State lives on daemons. Each compose/read path uses **one active door** (workshop connection + role). No ambient cross-daemon sync.

Related: [ADR-011](../docs/architecture/decisions/adr-011-shared-mode-portal-and-mesh.md) · [Shared mode](v0.6.0-shared-mode-plan.md) · [Peer mesh](v0.6.0-peer-mesh-plan.md)

---

## Doors Home can hold

| Door | Pairing role | What it is |
|------|--------------|------------|
| **Host** | local trusted | Desktop Home’s own workshop daemon (loopback / host engine) |
| **Portal** | `portal` | Full client *inside* a remote workshop |
| **Seat** | `portal` + `profile_id` | Portal on a Shared-mode workshop, bound to a member profile |
| **Peer** | `peer` | Capability-scoped outsider to a workshop (messages / share / mesh only) |

A Home install may *list* many doors. It never merges their catalogs into one brain.

---

## Compose / read matrix

| User intent | Active door | API (on that daemon) | Durable store (on that daemon) | Home UI bucket |
|-------------|-------------|----------------------|--------------------------------|----------------|
| Chat with the workshop agent | Host / Portal / Seat | `POST /v1/turns` or turn tickets | `session_catalog` or `shared_session_catalog` + session turns | Chat for **this** workshop |
| Shared-room chat (multi-seat) | Seat (Shared mode) | same turn APIs; `catalog: shared` | `shared_session_catalog`; turns stamp `speaker_profile_id` | Same chat rail, room marked shared |
| Steer a live workshop handoff | Host / Portal / Seat | `POST …/workshop/steer` (bound work) | session transcript (+ speaker when shared) | Same session thread |
| Peer conversation (human inbox) | Peer (or Portal using peer surface) | `POST/GET /v1/peer/messages` | that daemon’s `peer_inbox.json` | Peer inbox row tagged `workshopId` |
| Push a share bundle to a peer workshop | Peer | enveloped `POST /v1/share/push` | import into **recipient** vault/artifacts | Share action on that peer door |
| Daemon↔daemon mesh handoff | Host daemon’s mesh layer (Home operates local outbox) | `POST /v1/mesh/outbox` (+ flush) → remote `/v1/peer/messages` or share | sender `mesh/outbox`; recipient `mesh/inbox` + receipt | Mesh / handoff UI on **local** workshop (M4) |
| Mesh ack / retries | Host | `GET /v1/mesh/outbox`, `POST /v1/mesh/receipts` | `mesh/outbox`, `mesh/receipts` | Delivery status for local outbox items |

Remote peer/portal calls that deliver mesh payloads still hit the **recipient daemon**; Home only proxies with that door’s bearer + signed envelope.

---

## Same channel vs different channel

| Feels like | Actually |
|------------|----------|
| “Chat with Acme” as a member | Portal/Seat → Acme sessions (inside Acme) |
| “Message Acme as an outsider” | Peer → Acme `peer_inbox` (not Acme vault sudo) |
| “Send this note to Acme from Personal” | Mesh/share from Personal daemon → Acme import (capability mail) |
| Bidirectional peer thread | One pairing, one daemon’s peer inbox; Home may also keep a local outbound copy on Host for UX |

Peer chat UX and mesh outbox/inbox are **related but not the same store**:
- peer messages = human-readable conversation on a workshop  
- mesh inbox/outbox/receipts = delivery durability (`sender+seq`, acks)

---

## What never crosses

- Personal sessions ↛ Acme session list (or the reverse)
- Peer door ↛ portal settings/admin/vault browse
- Seat on Acme ↛ identity/active profile on Personal
- Mesh grant revoke ↛ automatic portal unpair (and reverse)
- Iroh ticket / reachability ↛ trust or “sync everything”
- Home UI cache of workshop A ↛ auto-replay into workshop B

---

## Home tagging (client-side)

Every peer/mesh-adjacent bubble Home shows should carry:

- `workshopId` — which door  
- `sinkKind` — e.g. peer vs host chat  
- optional `mesh` receipt/outbox id when showing delivery state  

Unread badges and inboxes are **per door**, then optionally summed in a switcher — never a single merged transcript.

---

## Desktop vs mobile

| | Desktop Home | Mobile Home |
|--|--------------|-------------|
| Host daemon | Often yes | Usually no |
| Portal / Seat | Yes | Yes |
| Peer door | Yes | Yes |
| Operate local mesh outbox | Yes (via Host) | Only if acting through a Host portal that exposes it — not by inventing a phone-side mesh brain |

Mobile is still a surface: it opens doors; it does not become a sync hub.

---

## M4 product mapping (later)

| Product phrase | Matrix row |
|----------------|------------|
| Share a note to a team brain | Peer/mesh share push → recipient import |
| Ask for review | Mesh/task-style handoff (capability) + peer or work card on recipient |
| Bring result home | Recipient → sender mesh/share back into **sender** daemon only |

Until M4 UX ships, the pipes above are the contract; Home must not invent a fourth “global inbox” store.
