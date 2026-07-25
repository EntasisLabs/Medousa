# ADR-011: Shared mode seats, portal, and peer mesh

## Status

Accepted (0.6.0 train)

## Context

Operators want an org “team brain” without true multi-tenant SSO. We already have switchable **profiles** (ADR-002), **pairing roles** (portal vs peer), and **Iroh** transport. Conflating “member of Acme” with “Alice’s work hat” or with “full portal sudo from a personal daemon” would break security and UX.

## Decision

1. **Three doors, one product language**
   - **Portal** — client of a workshop brain (existing). On Shared-mode daemons, portal sessions are bound to a **member profile** via pairing.
   - **Shared mode** — opt-in daemon configuration: profiles = members; `root` administers; `general` is the org agent persona for shared rooms; vault is shared with **attribution**, not partitioned.
   - **Mesh peer** — capability-scoped daemon↔daemon handoffs (extend peer). Never elevates to portal.

2. **Profiles as members (reuse ADR-002 registry)**  
   QR invites may carry a target `profile_id`. `PairedDeviceRecord` stores the bind. Remote portal callers resolve identity from the bearer bind, not from a process-global `set_active` race.

3. **Dual session catalogs**  
   Keep `session_catalog` (single-owner) and add `shared_session_catalog` (membership set). Create-time chooses the catalog; list merges for the caller’s bound profile.

4. **Transport trust**  
   Proxied Iroh traffic must not be treated as loopback. Gateway marks transport; auth bypasses that key on IP alone are forbidden for marked requests.

5. **Non-goals (v1)**  
   Login/SSO, ambient cross-daemon vault sync, mesh = portal, vault roots per member, CRDT co-edit.

## Consequences

- Personal daemons unchanged until Shared mode is enabled.
- Portal path allowlists gain profile rules (settings/admin) beyond today’s Peer-only restriction.
- Mesh product flows depend on M1 transport trust before unsigned share/message expansion.
- Shared chat agent principal stays on `general`; human lines stamp `speaker_profile_id`.

## Code anchors

- `src/user_profiles.rs`, `src/pairing/`, `src/session_catalog.rs`, `src/shared_session_catalog.rs`, `src/shared_mode.rs`
- `src/iroh_transport/gateway.rs`, `src/remote_trust.rs`, `src/portal_acl.rs`, `src/peer_scope.rs`, `src/mesh/`
- `architecture/v0.6.0-shared-mode-plan.md`, `architecture/v0.6.0-peer-mesh-plan.md`
