# Architecture Decision Records

Short, durable decisions — not sprint plans. For build history see [../../../architecture/archive/README.md](../../../architecture/archive/README.md).

| ADR | Title | Status |
|-----|-------|--------|
| [adr-002-user-profiles.md](adr-002-user-profiles.md) | Switchable user profiles + Locus tenancy | Accepted |
| [adr-003-multi-workshop-connections.md](adr-003-multi-workshop-connections.md) | Multi-workshop registry and active workshop | Accepted |
| [adr-004-durable-turn-spine.md](adr-004-durable-turn-spine.md) | Durable turn journal + SSE `?since=` replay | Accepted |
| [adr-005-host-scheduler-bound-workshop.md](adr-005-host-scheduler-bound-workshop.md) | Host scheduler + bound async workshop turns | Accepted |
| [adr-006-shell-tabs.md](adr-006-shell-tabs.md) | Shell-level tabs (everything is a tab) | Accepted |
| [adr-007-shell-split-panes.md](adr-007-shell-split-panes.md) | Shell split panes + stream pool | Accepted |

## Template

New ADRs use `adr-NNN-short-title.md`:

1. **Context** — problem and constraints  
2. **Decision** — what we chose  
3. **Consequences** — tradeoffs, migration  
4. **Code anchors** — paths to verify in the repo
