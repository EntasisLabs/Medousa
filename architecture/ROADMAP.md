# Roadmap — active work

> **Status:** Living document (updated 2026-07)  
> **Historical plans:** [archive/README.md](archive/README.md)

First-run UX, Home shell milestones, turn-loop FSM, user profiles (Phases 0–6), **centralized agent runtime**, **0.5.0 Vault / Versions / Liquid**, **0.6.0 Shared / Peer mesh / Dynamic + Home polish**, and **0.7.0 Accounts / Liquid depth / Browser act + Languages** are **shipped**. See [turn-runtime-and-lanes.md](turn-runtime-and-lanes.md), [ADR-002](../docs/architecture/decisions/adr-002-user-profiles.md), [v0.5.0-vault-versions-plan.md](v0.5.0-vault-versions-plan.md), [v0.6.0-shared-mode-plan.md](v0.6.0-shared-mode-plan.md), [v0.6.0-peer-mesh-plan.md](v0.6.0-peer-mesh-plan.md), [v0.7.0-plan.md](v0.7.0-plan.md).

**Next focus:** felt polish residual (empty states, F3–F5), optional mesh `client.relay`, inference profiles, **Forge integration phases** (daemon routes, ACP executors, Home review surface) — see below.

Full plans: **[v0.8.0-forge-plan.md](v0.8.0-forge-plan.md)** · **[v0.6.0-shared-mode-plan.md](v0.6.0-shared-mode-plan.md)** · **[v0.6.0-peer-mesh-plan.md](v0.6.0-peer-mesh-plan.md)** · **[home-messaging-matrix.md](home-messaging-matrix.md)** · [ADR-011](../docs/architecture/decisions/adr-011-shared-mode-portal-and-mesh.md) · **[workshop-and-automations-plan.md](workshop-and-automations-plan.md)** · **[polish-and-package-plan.md](polish-and-package-plan.md)** · **[operators-guide-docs-epic.md](operators-guide-docs-epic.md)**

---

## 0.8.0 Forge core (in progress)

**Goal:** the durable work-lifecycle substrate — user-owned work items, governed git environments, lease-fenced executor attempts, sealed evidence, human review, recoverable dispositions. External agents become replaceable executors on top; Medousa owns the work.

| Pillar | Plan | Status |
|--------|------|--------|
| Forge core crate | [v0.8.0-plan](v0.8.0-forge-plan.md) | ✅ `crates/medousa-forge` F1–F11 — model/events, FS store + journal, git engine, lifecycle + leases, policy-governed sealing, evidence-bound review, three dispositions, boot reconciliation, script adapter |
| Daemon integration | [v0.8.0-plan](v0.8.0-forge-plan.md) | ⬜ `Forge` in `AppState`, HTTP routes, process-backed `LivenessProbe` |
| ACP executors | [v0.8.0-plan](v0.8.0-forge-plan.md) | ⬜ Cursor/Codex adapters through the lease API; resume tokens |
| Home review surface | [v0.8.0-plan](v0.8.0-forge-plan.md) | ⬜ work kanban projection + evidence-bound review UI |

---

## 0.7.0 Accounts, Liquid depth, Browser + Languages (shipped)

**Goal:** ChatGPT/Cursor account connections that unlock coding agents (and ChatGPT-backed chat via Codex ACP); richer Liquid notes surface (CSV/Excel paste, chart export, note HTML/MD export, grammar check); shared-browser click/type plus markup languages in the Scripts editor.

| Pillar | Plan | Status |
|--------|------|--------|
| Accounts | [v0.7.0-plan.md](v0.7.0-plan.md) | ✅ Connections UI + vendor CLI login + ACP auth status |
| Liquid depth | [v0.7.0-plan.md](v0.7.0-plan.md) | ✅ CSV/Excel paste, chart PNG/SVG/CSV, note HTML/MD, grammar check |
| Browser + Languages | [v0.7.0-plan.md](v0.7.0-plan.md) | ✅ `cognition_browser_act` + handoff polish; HTML/CSS/JS/XML/JSON highlight |

---

## 0.6.0 Shared, Peer mesh, Dynamic (shipped)

**Goal:** Org-brain seats via Shared mode (no login); capability-scoped personal↔team mesh; finish hot-swappable ACP/MCP; Home polish (mobile, settings, spotlight, calendar).

| Pillar | Plan | Status |
|--------|------|--------|
| Shared mode | [v0.6.0-shared-mode-plan.md](v0.6.0-shared-mode-plan.md) | ✅ S0–S6 |
| Peer mesh | [v0.6.0-peer-mesh-plan.md](v0.6.0-peer-mesh-plan.md) | ✅ M0–M4 + introducer; optional next `client.relay` |
| Dynamic | [ADR-008](../docs/architecture/decisions/adr-008-hot-swappable-agent-runtime.md) | ✅ MCP space reads + ACP pump/permissions/Home bar |
| Polish | [polish-and-package-plan.md](polish-and-package-plan.md) | ✅ Mobile Home / nav / settings craft for 0.6; residual F3–F5 next |
| Docs / Operator’s Guide | [operators-guide-docs-epic.md](operators-guide-docs-epic.md) | ✅ Coverage + **P0 product voice**; [maintenance](operators-guide-maintenance.md) for contributors |

---

## 0.5.0 Vault, Versions, Liquid feeds (shipped)

**Goal:** Optional Git-backed **Versions** (off by default), liquid snapshot timeline+carousel, ```feed``` last-good Stasis results, Scripts `CodeEditorShell`, trash restore.

Living plan: [v0.5.0-vault-versions-plan.md](v0.5.0-vault-versions-plan.md)

---

## 0. Workshop & Automations (Home — active)

**Goal:** W0–W5.7 shipped Grapheme editor, flows, and bridges. **W6** reframes IA: **Capabilities** (runtime access) + **Automations** (Scripts Workbench, Flows, Schedules, History). Default `agent_turn`; Grapheme default, OpenShell advanced; Stasis dashboard admin-only.

| Phase | Theme | Status |
|-------|--------|--------|
| W0 Automations honesty | Run history, delivery picker, nav rename | ✅ |
| W1 Workshop browse | Grapheme modules, script library, Connections | ✅ |
| W2 Specialist create | Import wizard, editor-lite, allowlist preview | ✅ |
| W3 Flows v1 | Workflow composer, run/schedule from Home | ✅ |
| W4 Flows from history | Tool slice → replay steps | ✅ |
| W5 Grapheme depth | Save/compile/allowlist/WASM APIs | ✅ |
| W5.5–W5.6 Script workshop | Tabs + CodeMirror + `grapheme-lsp` + vault chrome | ✅ |
| W5.7 Workshop bridges | Add to flow; module insert from editor | ✅ |
| **W6 Scripts Workbench** | IA refactor, IDE shell, script chat, library ↔ flow links | 🔄 W6.0–W6.5 shipped |

**Next:** [scripts-workbench-plan.md](scripts-workbench-plan.md) — Workbench layout, Capabilities rename, Automations **Scripts** tab.

Full plan: [workshop-and-automations-plan.md](workshop-and-automations-plan.md) (W0–W5) · [scripts-workbench-plan.md](scripts-workbench-plan.md) (W6+)

## 1. Polish & package (0.6 felt polish)

**Goal:** Language, wayfinding, micro-interactions — not more capability exposure. Living plan: [polish-and-package-plan.md](polish-and-package-plan.md). Historical: [archive/polish-and-package-plan-capability-era.md](archive/polish-and-package-plan-capability-era.md).

| Phase | Theme | Status |
|-------|--------|--------|
| F0 Onboarding brain path | In-wizard install + non-blocking model download | ✅ |
| F1 First-run tone | Profiles teach examples (identity/prefs); wizard ownership + Presence “we” kept | ✅ |
| F2 Wayfinding | Bindings discoverability ✅; empty-state UI next | 🔄 |
| F3 Surface interactions | F3.0 rail chrome ✅ (+/>, rail history, Channels in Settings); Chat/Vault/Scripts next | 🔄 |
| F4 Spotlight + chrome | Relevance, focus, discovery (rail chrome slice in F3.0) | 🔄 |
| F5 Motion + micro | Intentional presence; reduced-motion | ⬜ |
| F6 Package residual | Signed updates / Iroh smoke if still ship-blocking | optional |

---

## 2. Inference stack + attachments (active)

**Goal:** Daemon-owned model catalog, explicit main/vision/STT profiles with cross-provider fallbacks, clean turn failures, and local attachments.

| Plan | Topic |
|------|--------|
| [inference-profiles-and-model-catalog-plan.md](inference-profiles-and-model-catalog-plan.md) | Catalog registry, inference profiles, API keys, fallbacks, error UX |
| [media-and-attachments-plan.md](media-and-attachments-plan.md) | Local upload, text extract, vision routing (P5) |

| Slice | Status |
|-------|--------|
| Phase 0 — turn failure hygiene | ✅ |
| Phase 1 — model capability registry | ✅ |
| Phase 2 — inference profiles (main / vision / STT) | ✅ |
| Phase 3 — per-provider keys + fallback router | ✅ |
| Phase 4 — STT on daemon | ✅ |
| P5a envelope + media API + composer UI | ✅ |
| P5a text extract (PDF/xlsx/csv) | ✅ |
| P5b vision (uses **vision** profile + registry) | ✅ |
| Epic polish — catalog badges + TUI profiles | ✅ |

---

## 3. Iroh P2P pairing (active)

**Goal:** Scan once; phone reaches workshop over encrypted P2P (direct or relay).

Full plan: [iroh-p2p-pairing-plan.md](iroh-p2p-pairing-plan.md)

| Phase | Status |
|-------|--------|
| 0 Transport scaffold + smoke | ✅ started |
| 1 QR v2 + iroh ticket | ✅ |
| 2 Mobile handshake | 🔄 |
| 3 Phone Iroh FFI transport | ⬜ |
| 4 Relay hardening | ⬜ |

Runbook: [connection-reliability](../docs/runbooks/connection-reliability.md)

---

## 4. Configuration & operator surface

| Deliverable | Status |
|-------------|--------|
| [configuration-reference.md](../docs/configuration-reference.md) | ✅ started |
| `medousa doctor --config` summary | ✅ |
| `medousa status` / `medousa stop` | ✅ |
| Per-engine settings on desktop (engine API) | ✅ |
| LLM provider picker in Home | ✅ |
| MCP add/edit in Home | ✅ |
| Capabilities toggles in Home | ✅ |

---

## 5. Desktop distribution

Signed `.app` / `.msi` / AppImage in CI — [desktop-distribution-plan.md](desktop-distribution-plan.md)

---

## 6. Embedded local inference

Gemma matrix + routing — [embedded-local-inference-plan.md](embedded-local-inference-plan.md)

---

## 7. Road To Production (power users)

**Goal:** Operator parity — per-engine settings, power-user CLI, headless packaging, CI gates.

Full plan: [road-to-production-plan.md](road-to-production-plan.md)

| Workstream | Status |
|------------|--------|
| WS1 P5a media routing | ✅ |
| WS2 Per-engine desktop settings | ✅ |
| WS3 CLI + headless install/Docker | ✅ |
| WS4 Multi-workshop hardening | ✅ |
| WS5 PR CI + version unify | ✅ |

---

## Deferred (not blockers)

| Item | Doc |
|------|-----|
| Phase E cloud auth | [archive/first-run-gap-analysis-2026-06.md](archive/first-run-gap-analysis-2026-06.md) |
| Phase F accessibility + prod packaging | [archive/first-run-and-lan-pairing-plan.md](archive/first-run-and-lan-pairing-plan.md) |
| Durable worker hardening | [durable-turn-worker-plan.md](durable-turn-worker-plan.md) |
| Identity recall ranking | [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md) |

---

## Suggested order

1. **W0 Automations honesty** — run history, delivery, nav rename ([workshop-and-automations-plan.md](workshop-and-automations-plan.md))  
2. **W1 Workshop browse** — Grapheme modules, Connections polish  
3. Polish **P0–P2** — trust + first ten minutes + teach Medousa  
4. **W2 Specialist create** — import wizard + editor-lite  
5. **W3 Flows v1** — workflow composer in Home  
6. Iroh Phase 2 mobile pairing + P5a attachments (parallel where possible)  
7. **W4 Flows from history** — tool slice → replay (differentiated bet)  
8. Desktop distribution CI (Polish P6)  
9. **W5 Grapheme depth** — WASM when daemon wired
