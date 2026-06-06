# Medousa Home — daemon-first workspace plan

> **Status:** Design — daemon APIs first; all UI (Tauri, TUI panels) deferred until contracts are stable  
> **Date:** 2026-05-30  
> **Related:** [turn-worker-bus-plan.md](turn-worker-bus-plan.md), [cognitive-identity-memory-plan.md](cognitive-identity-memory-plan.md), [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md), [component-daemon.md](component-daemon.md), [worker-continuity-plan.md](worker-continuity-plan.md)

## Executive summary

**Medousa Home** is the operator workspace where conversation, work-in-flight, and personal notes live together — not as a manager dashboard, but as a **workshop**: chat at the bench, work on the wall, notebooks on the shelf.

This document designs the full system **daemon-down**:

1. **Workspace** — unified feed + work cards projected from Stasis jobs and turn-worker records (Kanban is a *view*, not a new queue).
2. **Vault** — portable markdown notes owned by the daemon, indexed and linkable to memory + work.
3. **UI** — Tauri / rich TUI panels **explicitly deferred** until daemon APIs pass stability gates.

**Principle:** `medousa_daemon` is the only source of truth. Every future client (Tauri, TUI, Telegram) is a thin adapter over HTTP + SSE.

---

## Ontology (coherent model — guard this)

| Concept | Meaning | Canonical store |
|---------|---------|-----------------|
| **Conversation** | Interaction with Medousa | Sessions + interactive turns |
| **Work** | Execution in flight or recently finished | Stasis jobs + turn workers |
| **Vault** | Operator-authored knowledge | Markdown files + vault index |
| **Identity** | Stable relationships and preferences | Stasis identity graph |
| **Locus** | Episodic history and trails | Locus graph |
| **Workspace** | **Projection** of all of the above — not a sixth store | Daemon workspace service (ephemeral views + append-only feed) |

**Design guardrail:** Workspace **projects** truth; it does not **own** it. Prevent `WorkCard` from becoming a universal metadata bucket. Keep cards thin; push complexity into detail views, association tables, and the activity feed.

---

## North star experience (target, not day-one scope)

| Surface | Role | Daemon backing |
|---------|------|----------------|
| **Chat** | Primary — talk to Medousa | `POST /v1/interactive/turn` (exists) |
| **Work board** | What exists right now | `GET /v1/workspace/cards` (thin list) |
| **Activity feed** | What happened (chronological) | `GET /v1/workspace/feed` |
| **Live updates** | Both board + feed | `GET /v1/workspace/stream` (SSE) |
| **Library** | Journals, docs, project notes | `GET/POST /v1/vault/*` (new) |
| **Drawers** | Settings, obs, doctor | Existing routes; never the homepage |

**Cards answer:** *What exists?*  
**Feed answers:** *What happened?*

Kanban columns map to **job/work state**, not a separate task database:

```text
backlog      →  enqueued / scheduled recurring
in_flight    →  leased, running, turn_worker Running
wrapping_up  →  see below — first-class, not folded into "running" or "done"
done         →  terminal success (delivery + synthesis complete)
blocked      →  failed, dead_letter, cancelled
```

### Why `WrappingUp` is first-class

Most systems hide the gap between "worker finished" and "operator sees the answer." Medousa should not.

Agent systems often spend meaningful time in post-execution phases while users perceive *"it's done"*:

| Substate | System knows | User often thinks |
|----------|--------------|-------------------|
| Worker complete, synthesis pending | Host re-entry turn not finished | Done |
| Memory write pending | Locus/identity bridge in flight | Done |
| Delivery pending | Outbox → Telegram not acked | Done |
| Agent turn finalizing | Gatekeeper / verifier pass | Done |

`WrappingUp` makes that visible on the board and in the feed (`kind: work_wrapping_up`). Cards stay in this column until **all** attached terminal conditions clear (synthesis done, delivery ack or N/A). This is high-value UX differentiation — implement in W1 column mapping, not as a UI-only hack.

---

## What already exists (reuse, don’t rebuild)

| Primitive | Location | Home use |
|-----------|----------|----------|
| Stasis `Job` + `JobState` | `stasis::domain::runtime::job` | Work card backbone |
| `TurnWorkRecord` | `src/agent_runtime/turn_worker/store.rs` | Delegation cards |
| `cognition_runtime_jobs_list` | `src/runtime_tools.rs` | Agent visibility (keep; workspace supersedes for clients) |
| `cognition_turn_worker_status` | `src/agent_runtime/turn_worker_tools.rs` | Worker drill-down |
| Interactive turn SSE | `InteractiveTurnStreamEvent` in `daemon_api.rs` | Chat stream (unchanged) |
| Ingest + outbox | `session_mapping.rs`, deliver webhook | Mobile chat path |
| Locus graph | `memory_bundle.rs` | Episodic trails on note/job events |
| Identity store | Stasis cognitive mode | Relational links (“note about Mario”) |
| Artifacts | `artifact_store.rs` | Evidence attachments on cards (not user journals) |
| Manuscripts | `identity_manuscript.rs` | Card labels via `manuscript_id` |
| Stasis `/dashboard` | `medousa_daemon` mount | **Debug only** — not Medousa Home UX |

---

## Three memory layers + vault routing

Extends [cognitive-identity-memory-plan.md](cognitive-identity-memory-plan.md):

| Store | Holds | Write path | Read path |
|-------|-------|------------|-----------|
| **Vault** | Operator-authored markdown (journals, docs) | `cognition_vault_write`, `POST /v1/vault/notes` | `cognition_vault_read/search`, turn-start hints |
| **Locus** | Episodic trails (edited note X, job Y completed) | Bridge on vault save / job terminal | `cognition_memory_*` |
| **Identity** | Stable facts, people, preferences | `cognition_identity_remember` | Ranked digest, `cognition_identity_recall` |

**Routing rules:**

- Durable **content** the operator wrote → vault file (canonical).
- What **happened** while working on that content → Locus bridge node (audit + recall).
- Stable **relationships** (“weekly review is about Project Medousa”) → identity edge or preference.

Grapheme operates on **vault paths** as workspace files (same pattern as TUI `/open` + `/run`).

---

## Daemon module layout (new code)

```
src/
  workspace/           # Projections + activity feed (Phase W)
    mod.rs
    card.rs            # Thin WorkCard + WorkCardDetail projection
    event.rs           # WorkspaceEvent schema + append-only feed log
    feed.rs            # Feed read path + SSE multiplex (cards + events)
    revision.rs        # Monotonic workspace_revision counter
    store.rs           # Associations + feed persistence (not card metadata dump)
  vault/               # Notes corpus (Phase V)
    mod.rs
    store.rs           # Filesystem + index
    links.rs           # Wikilink parse, backlink index
    bridge.rs          # Locus bridge on save
  daemon_api.rs        # + Workspace* + Vault* request/response types
  bin/medousa_daemon.rs  # route registration
```

CLI smoke commands (daemon client, no UI):

```bash
medousa workspace cards --limit 20
medousa workspace feed --limit 50
medousa workspace snapshot
medousa workspace stream    # SSE: card + feed events, carries revision
medousa vault list
medousa vault read weekly-review.md
medousa vault write weekly-review.md --stdin
```

---

## Phase W — Workspace (board + feed)

**Goal:** Daemon contracts for (1) thin work board, (2) chronological activity feed, (3) versioned snapshot for SSE reconciliation. No Tauri until W3 passes stability gate.

### W1 — Thin cards, detail view, feed schema, versioned snapshot

#### Design rule: thin list, fat detail, separate associations

| Type | Purpose | Where |
|------|---------|-------|
| `WorkCard` | Kanban tile — scannable | `GET /v1/workspace/cards` |
| `WorkCardDetail` | Drill-down — metadata, payloads, links | `GET /v1/workspace/cards/{id}` |
| `WorkCardAssociation` | Cross-refs (vault paths, job_id, work_id) | `workspace_card_assoc` table / store |
| `WorkspaceEvent` | Append-only activity line | `GET /v1/workspace/feed` + SSE |

Do **not** add new fields to `WorkCard` when they belong in detail, associations, or feed. If a field is only needed on click → `WorkCardDetail`. If it's a link → association row. If it's a point-in-time happening → `WorkspaceEvent`.

#### `WorkCard` (thin projection)

```rust
/// Stable card id: prefer work_id for turn workers, else job_id
pub struct WorkCardId(pub String);

pub enum WorkBoardColumn {
    Backlog,
    InFlight,
    WrappingUp,
    Done,
    Blocked,
}

/// List item only — intentionally small
pub struct WorkCard {
    pub id: WorkCardId,
    pub column: WorkBoardColumn,
    pub title: String,
    pub status_label: String,   // "running", "synthesis pending", "delivery pending"
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}
```

Optional single-letter hint for client icons only (not for logic): `kind_hint: Option<"job" | "worker" | "turn" | "recurring">` — derived, never stored as source of truth.

#### `WorkCardDetail` (expanded view)

```rust
pub enum WorkCardKind {
    StasisJob,
    TurnWorker,
    InteractiveTurn,
    RecurringTick,
}

pub struct WorkCardDetail {
    pub card: WorkCard,              // thin fields repeated for convenience
    pub kind: WorkCardKind,
    pub subtitle: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub manuscript_id: Option<String>,
    pub job_id: Option<String>,
    pub work_id: Option<String>,
    pub job_type: Option<String>,
    pub user_ack: Option<String>,
    pub wrapping_up_reasons: Vec<String>,  // e.g. ["synthesis_pending", "delivery_pending"]
    pub terminal: bool,
    pub error: Option<String>,
    pub result_excerpt: Option<String>,
    pub associations: WorkCardAssociations,
}

pub struct WorkCardAssociations {
    pub vault_paths: Vec<String>,
    pub artifact_ids: Vec<String>,
    pub locus_node_ids: Vec<String>,
}
```

Associations live in `WorkCardAssociations` / `workspace_card_assoc` — **not** on the list card.

#### `WorkspaceEvent` (activity feed — define in W1 even if UI deferred)

Chronological stream of what happened across conversation, work, vault, identity, locus:

```rust
pub enum WorkspaceEventKind {
    // Work
    JobEnqueued,
    JobStarted,
    JobSucceeded,
    JobFailed,
    WorkDelegated,
    WorkCompleted,
    WorkWrappingUp,      // entered wrapping_up column
    WorkUnblocked,       // left wrapping_up → done
    // Conversation
    TurnAccepted,
    TurnCompleted,
    AgentReplied,
    // Vault (Phase V hooks append same schema)
    VaultNoteCreated,
    VaultNoteUpdated,
    // Memory (optional W2+)
    IdentityRemembered,
    LocusBridgeWritten,
}

pub enum WorkspaceEventActor {
    System,
    Agent,
    Operator,
    Scheduler,
}

pub struct WorkspaceEventRef {
    pub ref_type: String,   // "card", "job", "work", "session", "vault_path", "turn"
    pub ref_id: String,
}

pub struct WorkspaceEvent {
    pub id: String,                    // "wse:uuid"
    pub timestamp_utc: DateTime<Utc>,
    pub kind: WorkspaceEventKind,
    pub actor: WorkspaceEventActor,
    pub summary: String,               // human line: "Skill completed: echo-skill"
    pub refs: Vec<WorkspaceEventRef>,
}
```

Example feed (what operators want to scan):

```text
09:31  Job started — Skill: echo-skill
09:34  Skill completed — echo-skill (wrapping up: synthesis)
09:35  Note updated — journal/2026-05-30.md
09:37  Vault note created — projects/weekly-review.md
09:40  Agent replied — session ab12
```

Feed is **append-only** with retention (default 7 days, configurable). Cards are **derived current state** from jobs/workers — regenerated on read, not the feed log.

#### Versioned snapshot

Every snapshot and SSE reconnect handshake carries a monotonic revision so clients reconcile out-of-order events:

```rust
pub struct WorkspaceSnapshot {
    pub workspace_revision: u64,       // increments on any card or feed append
    pub server_time_utc: DateTime<Utc>,
    pub cards: Vec<WorkCard>,
    pub counts_by_column: HashMap<String, u32>,
    pub feed_tail: Vec<WorkspaceEvent>,  // last N events (default 20) for quick hydrate
}
```

SSE events include `workspace_revision` on every frame. Client rule: if `event.revision <= last_seen`, ignore or treat as duplicate; if `gap detected`, `GET /v1/workspace/snapshot?since_revision=N`.

```json
{
  "workspace_revision": 442,
  "event_type": "feed_appended",
  "event": { "id": "wse:…", "summary": "Skill completed", "…": "…" }
}
```

**Title projection rules** (deterministic, no LLM):

| Source | Title |
|--------|-------|
| `TurnWorkRecord` | `user_ack` if non-empty, else first line of `task_prompt` (truncated) |
| `openshell.sandbox.run` + `skill_script` | `Skill: {manuscript_id} — {script}` |
| `openshell.sandbox.run` + `command` | `Sandbox: {argv0}` |
| `agent_turn` / ingest ask | `Chat turn` + session prefix |
| `workflow.*` | `Workflow: {workflow_id}` |
| recurring `morning-brief` | `Scheduled: morning brief` |
| fallback | last segment of `job_type` |

**Column mapping:**

| `JobState` / `TurnWorkStatus` | Column | Notes |
|-------------------------------|--------|-------|
| Enqueued | Backlog | |
| Leased, Running | InFlight | |
| TurnWorker `Completed` + synthesis pending | WrappingUp | `wrapping_up_reasons: ["synthesis_pending"]` |
| Job `Succeeded` + delivery not acked | WrappingUp | `["delivery_pending"]` |
| Agent turn finalizing (verifier/gatekeeper) | WrappingUp | `["turn_finalizing"]` |
| All terminal conditions met | Done | |
| Failed, DeadLetter, Cancelled | Blocked | |

**API:**

```
GET /v1/workspace/cards
  ?session_id=...         # optional filter
  ?column=in_flight       # optional
  ?limit=50               # default 50, max 200
  ?include_terminal=true  # default false (hide done >24h)
  → { workspace_revision, cards: WorkCard[] }

GET /v1/workspace/cards/{card_id}
  → WorkCardDetail

GET /v1/workspace/feed
  ?since_id=wse:...       # cursor pagination
  ?since_revision=400     # optional: only events after revision
  ?limit=50               # default 50, max 200
  → { workspace_revision, events: WorkspaceEvent[] }

GET /v1/workspace/snapshot
  ?since_revision=0       # client last seen; returns delta hint if stale
  → WorkspaceSnapshot
```

**Implementation notes:**

- Project cards from `list_jobs_by_state` + `TurnWorkerStore` (extend `list_all` with cap) — **thin** mapping only.
- Build `WorkCardDetail` on demand in `cards/{id}` handler (load job payload, turn record, associations).
- On every state transition: append `WorkspaceEvent`, bump `workspace_revision`.
- Deduplicate cards: if job `correlation_id` matches active `work_id`, one card id (worker-primary).
- Feed + revision persisted: Surreal tables or append-only JSONL `workspace_feed.jsonl` + `workspace_revision` file.

**Stability gate W1:**

- [ ] `GET /cards` returns thin shape only (no `session_id` on list items)
- [ ] `GET /cards/{id}` returns full detail + associations
- [ ] `GET /feed` returns chronological `WorkspaceEvent` list
- [ ] `GET /snapshot` includes `workspace_revision` + `feed_tail`
- [ ] WrappingUp cases covered in unit tests (synthesis, delivery, finalizing)
- [ ] CLI `workspace cards` active count matches `cognition_runtime_jobs_list`

---

### W2 — Workspace stream (SSE: cards + feed + revision)

**Goal:** One subscription for board updates **and** activity lines; clients reconcile via `workspace_revision`.

```
GET /v1/workspace/stream
  ?session_id=...     # optional: operator scope
  ?since_revision=0   # last seen on reconnect
  Accept: text/event-stream
```

**First frame on connect:** `snapshot` event with full `WorkspaceSnapshot` (or delta if `since_revision` current).

**Subsequent frames:**

```json
{
  "workspace_revision": 443,
  "stream_event_type": "card_upserted | card_removed | feed_appended | column_counts | heartbeat",
  "emitted_at_utc": "2026-05-30T12:00:00Z",
  "card": { /* thin WorkCard only */ },
  "feed_event": { /* WorkspaceEvent, when feed_appended */ },
  "counts": { "backlog": 2, "in_flight": 1, "wrapping_up": 1, "done": 5 }
}
```

**Emitters (daemon hooks):**

| Hook | Stream | Feed `WorkspaceEvent` |
|------|--------|------------------------|
| Job enqueued | `card_upserted` | `JobEnqueued` |
| Job running | `card_upserted` | `JobStarted` |
| Job succeeded | `card_upserted` | `JobSucceeded` |
| Enter wrapping_up | `card_upserted` | `WorkWrappingUp` |
| Leave wrapping_up → done | `card_upserted` | `WorkUnblocked` |
| Turn worker delegated | `card_upserted` | `WorkDelegated` |
| Turn worker completed | `card_upserted` | `WorkCompleted` |
| Interactive turn done | `card_removed` or upsert | `AgentReplied` |
| Vault save (V+) | — | `VaultNoteUpdated` |
| Done card TTL expiry | `card_removed` | — |
| Every 30s | `heartbeat` | — |

Each hook: append feed event → bump revision → emit SSE frame.

**Bus alignment:** Implements structured adapter events from [turn-worker-bus-plan.md](turn-worker-bus-plan.md) at daemon persistence; feed is the operator-visible chronicle.

**Stability gate W2:**

- [ ] `medousa workspace stream` shows `feed_appended` + `card_upserted` within 1s of job enqueue
- [ ] Reconnect with `since_revision` does not duplicate events
- [ ] Revision gap forces client snapshot refresh

---

### W3 — Card actions (mutations through existing paths)

No new cancel/retry implementation — wrap existing tools:

```
POST /v1/workspace/cards/{card_id}/cancel
  → resolves to job_id or work_id → cognition_runtime_jobs_cancel / turn_worker cancel

POST /v1/workspace/cards/{card_id}/retry
  → POST /v1/jobs/{job_id}/replay-and-resume (existing)
```

**Vault association (prep for Phase V):**

```
POST /v1/workspace/cards/{card_id}/link-vault
  { "vault_path": "journal/2026-05-30.md" }
```

**Stability gate W3 (Workspace API frozen):**

- [ ] OpenAPI-style section in this doc matches implemented routes
- [ ] Integration test: ingest `/skill` → card appears → job completes → card → Done
- [ ] No breaking changes for 2 weeks of daily dogfood OR explicit version bump `Accept-Version: workspace-v1`

**Only after W3:** UI work may begin (Tauri milestone M0).

---

## Phase V — Vault (library)

**Goal:** Portable markdown corpus; daemon-indexed; linkable to work cards and memory.

### V0 — Files + CRUD

**Storage layout:**

```
~/.local/share/medousa/vault/           # user vault (default)
  journal/2026-05-30.md
  projects/medousa/home-plan.md
.medousa/vault/                         # optional project overlay (merged index)
```

**Note record:**

```rust
pub struct VaultNote {
    pub path: String,              // relative posix path, e.g. "journal/2026-05-30.md"
    pub title: String,             // from first # heading or filename
    pub byte_size: usize,
    pub content_hash: String,      // sha256 of body
    pub modified_at_utc: DateTime<Utc>,
    pub created_at_utc: DateTime<Utc>,
    pub tags: Vec<String>,         // from frontmatter
    pub wikilinks_out: Vec<String>, // resolved paths after save
    pub backlinks: Vec<String>,    // computed index
}
```

**API:**

```
GET  /v1/vault/notes?prefix=journal/&limit=100
GET  /v1/vault/notes/{path}        # path URL-encoded
PUT  /v1/vault/notes/{path}        # body = markdown raw; If-Match: content_hash optional
POST /v1/vault/notes               # { path, content }
DELETE /v1/vault/notes/{path}      # soft-delete → .trash/ (default)

GET  /v1/vault/search?q=medousa&limit=20   # ranked hits — see below
GET  /v1/vault/notes/{path}/backlinks
```

**Cognition tools** (host + worker read; write host-gated like `identity_remember`):

| Tool | Tier | Purpose |
|------|------|---------|
| `cognition_vault_list` | observe | List paths + titles |
| `cognition_vault_read` | observe | Read note body (budget-capped) |
| `cognition_vault_search` | observe | Full-text search |
| `cognition_vault_write` | propose/commit | Create/update (operator approval or auto for TUI session) |

**Vault search hits (ranked from day one — even ripgrep-backed):**

Do not return a flat path list. Return relevance metadata so clients never re-rank:

```rust
pub struct VaultSearchHit {
    pub note: VaultNoteSummary,   // path, title, modified_at — not full body
    pub score: f32,               // higher = better match (normalized 0..1)
    pub matched_terms: Vec<String>,
    pub snippet: Option<String>,  // line excerpt with match context
}

pub struct VaultSearchResponse {
    pub query: String,
    pub hits: Vec<VaultSearchHit>,
}
```

**Scoring (V0 heuristic, no embeddings):**

| Signal | Weight |
|--------|--------|
| Term in title / filename | High |
| Term in first heading | High |
| Term frequency in body | Medium |
| Recency (`modified_at`) | Low tie-break |
| Exact phrase match | Boost |

Ripgrep (or walk + scan) produces candidate lines; daemon assigns `score` + `matched_terms` before respond. Semantic search later replaces scoring function, not response shape.

**Stability gate V0:** round-trip write/read; search returns `VaultSearchHit` shape; `manuscript-validate` unaffected; external editor watch refreshes index.

---

### V1 — Wikilinks + work card links

- Parse `[[note]]`, `[[folder/note]]`, `#tags` in frontmatter on save.
- Maintain backlink index in Surreal table `vault_note_link`.
- `POST /v1/workspace/cards/{id}/link-vault` writes association row.
- On job `Succeeded`, auto-append optional footer to linked vault note (config flag, default off).

**Stability gate V1:** `[[weekly-review]]` resolves; `WorkCardDetail.associations.vault_paths` populated (not list card); vault save emits `VaultNoteUpdated` feed event; CLI smoke passes.

---

### V2 — Locus + identity bridge

On vault save (debounced):

1. Emit Locus bridge node: `{ kind: vault_edit, path, hash, links[], session_id? }`.
2. If frontmatter `identity_ref:` or operator tool call → `cognition_identity_remember` edge.

Turn-start hint (like context packs): inject **top 3 relevant vault titles** from search query = user prompt keywords (cheap, not semantic yet).

**Stability gate V2:** Locus node exists after save; identity edge optional; no turn latency regression >50ms p95.

---

### V3 — Grapheme workspace roots

- `spec.vault` on manuscript YAML (optional): allowed path prefixes for worker grapheme runs.
- `cognition_vault_grapheme_run` → enqueue grapheme job with `workspace_root = vault_path_parent`.
- Reuse `editor_runtime.rs` patterns.

**Stability gate V3 (Vault API frozen):** same criteria as W3 — dogfood + version header.

---

## Phase M — UI (deferred)

**Do not start until:** Workspace W3 **and** Vault V1 stability gates pass.

### Entry criteria checklist

- [ ] `GET /v1/workspace/stream` stable 2+ weeks
- [ ] `GET/PUT /v1/vault/notes/*` stable 2+ weeks
- [ ] CLI smoke scripts in `scripts/smoke-home-api.sh`
- [ ] `medousa doctor` section: Workspace + Vault

### M0 — Tauri shell (minimal)

- Chat panel → existing interactive turn SSE
- Work rail → thin cards from workspace stream
- Activity sidebar → `feed_appended` events (chronological)
- No vault editor yet (CLI + external editor OK)

### M1 — Library tab

- Vault tree + markdown editor
- Click card → linked vault notes

### M2 — Full home

- Kanban columns + swimlanes (`intent`, `manuscript_id`)
- Native notifications on `card_upserted` → Done
- Optional: Telegram card summary via outbox (one line, not full board)

### M3 — Polish

- Drag card → cancel only (no fake reorder)
- Split panes, system tray
- TUI parity via workspace side panel (optional; lower priority than Tauri)

---

## Work board — Kanban detail (for UI phase)

**Swimlane options** (client-side projection from `WorkCardDetail` on drill-down or cached detail map — not required on thin list):

| Swimlane key | Source field (`WorkCardDetail`) |
|--------------|--------------------------------|
| By intent | `TurnWorkRecord.intent` |
| By manuscript | `manuscript_id` |
| By job family | `job_type` prefix before `.` |
| By session | `session_id` |

**Card click actions (all daemon API):**

| Action | API |
|--------|-----|
| View output | `GET /v1/workspace/cards/{id}` (`WorkCardDetail`) + `GET /v1/jobs/{job_id}/result` |
| Activity history | `GET /v1/workspace/feed?refs.card={id}` (filter by ref) |
| Jump to chat | `session_id` → `GET /v1/sessions/{id}/history` |
| Open linked note | `GET /v1/vault/notes/{path}` |
| Cancel | `POST /v1/workspace/cards/{id}/cancel` |
| Ask Medousa about this | `POST /v1/interactive/turn` with prompt referencing `card_id` |

---

## Agent integration

### Prompt blocks

```
[MEDOUSA_WORKSPACE]
revision=442 in_flight=1 wrapping_up=1
cards:
- Skill: echo-skill (in_flight)
- Chat turn ab12 (wrapping_up: synthesis_pending)
recent:
- 09:34 Skill completed — echo-skill
```

Inject at host turn start when `in_flight + wrapping_up > 0` (cap 5 thin card lines + 3 feed lines).

### Chat commands (ingest)

| Command | Action |
|---------|--------|
| `/work` | Reply with active thin cards + recent feed lines |
| `/work {card_id}` | `WorkCardDetail` + result excerpt |
| `/activity` | Last N `WorkspaceEvent` summaries (like feed tail) |

### Manuscript integration

```yaml
# .medousa/manuscripts/weekly-review.yaml
spec:
  vault:
    read_prefixes: ["journal/", "projects/"]
    default_note: "journal/weekly-review.md"
```

Morning brief manuscript reads vault template instead of hardcoded prompt only.

---

## Persistence schema (Surreal, when enabled)

```sql
DEFINE TABLE workspace_feed_event SCHEMAFULL;
DEFINE FIELD id ON workspace_feed_event TYPE string;
DEFINE FIELD workspace_revision ON workspace_feed_event TYPE int;
DEFINE FIELD timestamp_utc ON workspace_feed_event TYPE datetime;
DEFINE FIELD kind ON workspace_feed_event TYPE string;
DEFINE FIELD actor ON workspace_feed_event TYPE string;
DEFINE FIELD summary ON workspace_feed_event TYPE string;
DEFINE FIELD refs ON workspace_feed_event TYPE object;
DEFINE INDEX idx_feed_revision ON workspace_feed_event COLUMNS workspace_revision;

DEFINE TABLE workspace_card_assoc SCHEMAFULL;
DEFINE FIELD card_id ON workspace_card_assoc TYPE string;
DEFINE FIELD job_id ON workspace_card_assoc TYPE option<string>;
DEFINE FIELD work_id ON workspace_card_assoc TYPE option<string>;
DEFINE FIELD vault_paths ON workspace_card_assoc TYPE array<string>;
DEFINE FIELD artifact_ids ON workspace_card_assoc TYPE array<string>;
DEFINE FIELD session_id ON workspace_card_assoc TYPE option<string>;

DEFINE TABLE vault_note_index SCHEMAFULL;
DEFINE FIELD path ON vault_note_index TYPE string;
DEFINE FIELD content_hash ON vault_note_index TYPE string;
DEFINE FIELD wikilinks_out ON vault_note_index TYPE array<string>;
DEFINE FIELD modified_at_utc ON vault_note_index TYPE datetime;
DEFINE INDEX idx_vault_path ON vault_note_index COLUMNS path UNIQUE;
```

In-memory backend:

- `~/.local/share/medousa/workspace/feed.jsonl` — append-only `WorkspaceEvent`
- `~/.local/share/medousa/workspace/revision` — single u64
- `~/.local/share/medousa/workspace/associations.jsonl`
- `~/.local/share/medousa/vault/index.jsonl`

---

## Testing strategy (daemon-only)

| Phase | Tests |
|-------|-------|
| W1 | Unit: title rules, column mapping, WrappingUp substates; thin vs detail shape; feed append; revision monotonic |
| W2 | SSE: card + feed events; reconnect `since_revision`; no duplicates |
| W3 | Cancel/retry via card id; link-vault association |
| V0 | Round-trip CRUD; `VaultSearchHit` scoring; concurrent external file edit |
| V1 | Wikilink resolution + backlinks |
| V2 | Locus bridge node created (mock store) |
| Smoke | `scripts/smoke-home-api.sh` for CI (no Tauri) |

No Playwright, no Tauri tests until Phase M.

---

## Implementation order (recommended)

```text
Daemon foundation (parallel/minimal)
  └─ Agent runtime Ph5 hardening (heartbeat, delivery) — see centralized-agent-runtime-roadmap

Phase W1  Thin cards + detail + feed schema + versioned snapshot  ← start here
Phase W2  Workspace SSE (cards + feed + revision)
Phase W3  Card actions + link-vault prep + freeze gate

Phase V0  Vault CRUD + cognition tools
Phase V1  Wikilinks + card associations
Phase V2  Locus/identity bridge + turn hints
Phase V3  Grapheme vault roots + freeze gate

Phase M0+ Tauri (only after both freeze gates)
```

**Explicitly out of scope until post-M1:**

- Real-time collaborative editing
- Semantic embedding search (use keyword search first)
- Custom task priorities / drag-reorder queue
- Replacing manuscripts with vault notes
- Replacing Stasis `/dashboard` (stays debug)

---

## Success metrics

| Metric | Target |
|--------|--------|
| Time to see new job on workspace stream | < 1s p95 |
| CLI `workspace cards` vs reality | 100% match active jobs |
| Vault file portable | Readable in Obsidian / any editor without Medousa |
| Turn latency with workspace hint | < +50ms p95 |
| UI freeze gates | 2 weeks dogfood, zero breaking API changes |

---

## Open questions (resolve during W1)

1. **Card retention:** Done cards visible 24h default — configurable?
2. **Feed retention:** 7d default — same TTL as done cards or longer?
3. **Multi-session board:** Global vs per-session filter default for single operator?
4. **WrappingUp exit:** Require all of synthesis + delivery + memory bridge, or configurable per job type?
5. **Vault write auth:** Same bearer token as dashboard actions, or session-scoped only?
6. **Project vault merge:** Union index or project overrides user on path conflict?

---

## Code anchors (existing)

| Area | File |
|------|------|
| Job listing | `src/runtime_tools.rs` |
| Turn work records | `src/agent_runtime/turn_worker/store.rs` |
| Daemon routes | `src/bin/medousa_daemon.rs` |
| API types | `src/daemon_api.rs` |
| OpenShell job cards | `src/openshell_sandbox_run.rs` |
| Skill ingest | `src/skill_ingest.rs` |
| Interactive SSE | `src/interactive_turn_runtime.rs` |

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | Initial daemon-first design — workspace + vault + deferred UI |
| 2026-05-30 | Review pass: thin `WorkCard` vs `WorkCardDetail`; first-class `WorkspaceEvent` feed; `workspace_revision`; ranked vault search; WrappingUp substates; ontology + anti-bucket guardrails |
