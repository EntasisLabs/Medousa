# Roadmap — active work

> **Status:** Living document (updated 2026-06)  
> **Historical plans:** [archive/README.md](archive/README.md)

First-run UX, Home shell milestones, turn-loop FSM, user profiles (Phases 0–6), and **centralized agent runtime + host/worker bus + Specialists** are **shipped**. See [turn-runtime-and-lanes.md](turn-runtime-and-lanes.md), [identity-manuscripts-and-recall-plan.md](identity-manuscripts-and-recall-plan.md), [ADR-002](../docs/architecture/decisions/adr-002-user-profiles.md).

Remaining focus: **Workshop + Automations** (Home exposure of Grapheme/Stasis/MCP), **polish & package**, **attachments**, **Iroh pairing**, and **distribution**.

Full plans: **[workshop-and-automations-plan.md](workshop-and-automations-plan.md)** · **[polish-and-package-plan.md](polish-and-package-plan.md)**

---

## 0. Workshop & Automations (Home — active)

**Goal:** Replace thin Cron + Skills surfaces with paired **Workshop** (configure specialists, skills palette, modules, connections) and **Automations** (schedules, flows, run history, delivery). Default `agent_turn`; Grapheme default, OpenShell advanced; Stasis dashboard admin-only.

Full plan: [workshop-and-automations-plan.md](workshop-and-automations-plan.md)

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

---

## 1. Polish & package (normie continuity)

**Goal:** Expose shipped engine (memory, vault, plugins, AVEC) — install, trust, teach, provenance — without new runtime work.

| Phase | Theme | Status |
|-------|--------|--------|
| P0 Trust baseline | Sidecar, Iroh smoke, health | ⬜ |
| P1 First ten minutes | Wizard epilogue, guided win | ⬜ |
| P2 Teach Medousa | Identity from Home | ⬜ |
| P3 Continuity surfaces | Unified search, provenance | ⬜ |
| P4 Workshop exposure | Superseded by [workshop-and-automations-plan.md](workshop-and-automations-plan.md) W1–W2 | ⬜ |
| P5 App affordances | Share, context menus, P5 attach | ⬜ |
| P6 Package & ship | Signed bundles, updates | ⬜ |
| P7 Promise & copy | README / empty states | ⬜ |

---

## 2. Local attachments (P5 — active)

**Goal:** Attach files in Home chat; bytes on disk under `medousa/media/`; references in `parts[]`; localhost upload only.

Full plan: [media-and-attachments-plan.md](media-and-attachments-plan.md)

| Slice | Status |
|-------|--------|
| P5a envelope + media API + composer UI | ⬜ |
| P5a text extract (PDF/xlsx/csv) | ⬜ |
| P5b vision for images | ⬜ deferred |

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
| `medousa doctor --config` summary | ⬜ |
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
