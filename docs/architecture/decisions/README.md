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
| [adr-008-hot-swappable-agent-runtime.md](adr-008-hot-swappable-agent-runtime.md) | Hot-swappable agentic runtime (MCP + ACP bones) | Accepted |
| [adr-009-vault-workbooks.md](adr-009-vault-workbooks.md) | Vault workbooks + overlay formulas | Accepted |
| [adr-010-slides-player-and-layers.md](adr-010-slides-player-and-layers.md) | Slides player + declarative CSS layers | Accepted |
| [adr-010-virtual-shell-workspaces.md](adr-010-virtual-shell-workspaces.md) | Virtual shell workspaces | Accepted |
| [adr-011-shared-mode-portal-and-mesh.md](adr-011-shared-mode-portal-and-mesh.md) | Shared mode seats, portal, and peer mesh | Accepted |
| [adr-012-medousa-anywhere-surfaces.md](adr-012-medousa-anywhere-surfaces.md) | Native host surfaces vs external-agent adapters | Accepted |
| [adr-013-daemon-trust-zones-and-auth.md](adr-013-daemon-trust-zones-and-auth.md) | Daemon trust zones and mandatory authentication | Proposed |
| [adr-014-identifier-and-filesystem-authority.md](adr-014-identifier-and-filesystem-authority.md) | Typed identifiers and handle-relative filesystem authority | Proposed |
| [adr-015-bounded-durable-turn-pipeline.md](adr-015-bounded-durable-turn-pipeline.md) | Bounded single-writer durable turn pipeline | Proposed |
| [adr-016-transactional-store-ownership.md](adr-016-transactional-store-ownership.md) | Transactional store ownership and crash consistency | Proposed |
| [adr-017-request-scoped-runtime-context.md](adr-017-request-scoped-runtime-context.md) | Request-scoped runtime context and exact ownership | Proposed |
| [adr-018-untrusted-webview-isolation.md](adr-018-untrusted-webview-isolation.md) | Untrusted webview isolation and minimal browser bridge | Proposed |

> **Numbering note:** two accepted decisions were independently assigned
> ADR-010. Their filenames and identifiers remain unchanged so existing links
> and history do not lie. New decisions continue at ADR-013; the collision can
> be resolved only by a separate explicit supersession/alias decision.

## Template

New ADRs use `adr-NNN-short-title.md`:

1. **Context** — problem and constraints  
2. **Decision** — what we chose  
3. **Consequences** — tradeoffs, migration  
4. **Code anchors** — paths to verify in the repo
