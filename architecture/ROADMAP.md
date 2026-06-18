# Roadmap — active work

> **Status:** Living document (updated 2026-06)  
> **Historical plans:** [archive/README.md](archive/README.md)

First-run UX, Home shell milestones, turn-loop FSM, and **user profiles** (Phases 0–6) are **shipped**. See [ADR-002](../docs/architecture/decisions/adr-002-user-profiles.md).

Remaining focus: **attachments**, **Iroh pairing**, **distribution**, and operator-surface polish.

---

## 1. Local attachments (P5 — active)

**Goal:** Attach files in Home chat; bytes on disk under `medousa/media/`; references in `parts[]`; localhost upload only.

Full plan: [media-and-attachments-plan.md](media-and-attachments-plan.md)

| Slice | Status |
|-------|--------|
| P5a envelope + media API + composer UI | ⬜ |
| P5a text extract (PDF/xlsx/csv) | ⬜ |
| P5b vision for images | ⬜ deferred |

---

## 2. Iroh P2P pairing (active)

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

## 3. Configuration & operator surface

| Deliverable | Status |
|-------------|--------|
| [configuration-reference.md](../docs/configuration-reference.md) | ✅ started |
| `medousa doctor --config` summary | ⬜ |
| LLM provider picker in Home | ✅ |
| MCP add/edit in Home | ✅ |
| Capabilities toggles in Home | ✅ |

---

## 4. Desktop distribution

Signed `.app` / `.msi` / AppImage in CI — [desktop-distribution-plan.md](desktop-distribution-plan.md)

---

## 5. Embedded local inference

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

1. Iroh Phase 0 smoke + Phase 2 mobile pairing  
2. P5a local attachments  
3. Desktop distribution CI  
4. Doctor config summary + finish configuration reference
