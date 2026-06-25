# Road To Production

> **Status:** Active (2026-06)  
> **Audience:** Primary operator + power users / headless deployers  
> **Thesis:** Core turn runtime and LTE pairing are field-validated. This epic closes operator parity (per-engine settings, CLI, packaging), attachment routing (P5a), and release hygiene.

**Related:** [polish-and-package-plan.md](polish-and-package-plan.md) (normie track), [media-and-attachments-plan.md](media-and-attachments-plan.md), [ROADMAP.md](ROADMAP.md)

---

## Workstream status

| WS | Focus | Status |
|----|--------|--------|
| WS1 | P5a media routing — PDF/docs without vision profile | Shipped |
| WS2 | Per-engine settings on desktop (engine API) | Shipped |
| WS3 | Power-user CLI + headless install/Docker | Shipped |
| WS4 | Multi-workshop hardening + desktop remote UX | Shipped (UX note; Iroh desktop deferred) |
| WS5 | PR CI + version unify + test fixes | Shipped |

---

## WS1 — Attachments (P5a)

**Problem:** Any `media_refs` forced `vision_target()` — PDFs failed before text extract.

**Fix:** [`media_vision.rs`](../src/media_vision.rs) `has_vision_media()`; [`daemon_interactive_turn.rs`](../src/agent_runtime/daemon_interactive_turn.rs) gates vision only for images.

**Exit criteria:** PDF attach without vision model → turn runs with `[Attachments]` extract block.

---

## WS2 — Per-engine settings

**Problem:** Desktop Settings used host-global `tui_defaults.json`; mobile used engine API.

**Fix:**
- `GET/PUT /v1/runtime/tui-defaults` on daemon
- Home `workshopDefaults` loads/saves via `getEngineTuiDefaults` / `putEngineTuiDefaults`
- One-time `migrate_global_tui_defaults_to_engine`

**Exit criteria:** Two workshops, different model profiles, switch without restart bleed.

---

## WS3 — Power-user CLI

**Dual surface:** Home = normie land. CLI = operator land (no gatekeeping).

| Command | Purpose |
|---------|---------|
| `medousa status` | Bind, health, data dir |
| `medousa stop [--local-engine]` | Graceful shutdown |
| `medousa doctor --config [--json]` | Config summary for scripts |
| `medousa iroh` | First-class in help |

**Headless:** `install.sh --profile headless-server`, `Dockerfile`, `docker-compose.yml`, `contrib/systemd/medousa-engine.service`

---

## WS4 — Multi-workshop

- Workshop switch reloads engine defaults
- Paired remote workshops on desktop: "LAN only — use mobile app for LTE"
- `home_daemon_url.txt` remains legacy seed only (workshops registry is source of truth)

**Deferred:** Desktop Iroh client transport (Option B in plan) — only if desktop-off-LAN is required.

---

## WS5 — Release engineering

- `.github/workflows/ci.yml` — `cargo test --lib`, `cargo check`, Home `npm run check`
- Version unified at `0.1.0` (root `Cargo.toml` → Home `package.json`)

---

## Parallel track

Continue [polish-and-package-plan.md](polish-and-package-plan.md) for P1–P2 normie polish — do not block on RTP.
