# ADR-002: Switchable user profiles and Locus tenancy

**Status:** Accepted (2026-06)  
**Epic history:** [../../../architecture/archive/user-profiles-plan.md](../../../architecture/archive/user-profiles-plan.md)

## Context

Operators need separate **work** and **home** (or other) contexts: distinct identity memory, episodic recall, and chat session catalogs — without treating chat `session_id` as the identity principal.

Legacy installs must keep working with a single implicit profile and existing Locus data under tenant `default`.

## Decision

1. **Profile id format:** `user:{slug}` (e.g. `user:work`, `user:default`).
2. **Registry:** `user_profiles.json` holds `active_profile_id` + profile records; daemon API + Home UI switch active profile.
3. **Identity principal:** Active profile resolves to `identity_user_id` on turns and identity tools (`MEDOUSA_IDENTITY_USER_ID` env overrides for ops).
4. **Locus isolation:** Scoped session keys — default profile keeps plain chat session ids (tenant `default`); other profiles use `tenant:{slug}::session:{chatSessionId}`.
5. **Channel policy:** Interactive lane uses `channel:{slug}` for non-default profiles (`channel:interactive` for default).
6. **Turn ledger:** Each JSONL row records `active_profile_id` for observability.
7. **Portability:** Export/import bundle (identity context + Locus nodes per session) via daemon API and CLI.

Profiles are **not** authentication boundaries — document clearly for operators.

## Consequences

- Session catalog rows carry `profile_id`; list APIs filter by active profile.
- Paired mobile reloads active profile on foreground resume; cross-device last-writer-wins with user notice.
- MCP/ingest paths keep sender-derived ids; no automatic profile mapping for external channels.
- Profile export is bounded (500 sessions × 500 nodes per session in v1).

## Code anchors

| Area | Path |
|------|------|
| Registry | `src/user_profiles.rs` |
| Locus scoping | `src/locus_memory.rs` |
| Daemon routes | `src/bin/medousa_daemon.rs` (`/v1/identity/profiles*`) |
| Export/import | `src/profile_portability.rs` |
| Home store + UI | `apps/medousa-home/src/lib/stores/userProfiles.svelte.ts` |
| Turn ledger | `src/agent_runtime/turn_ledger.rs` |
