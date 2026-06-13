# Active work — configuration & operator surface

> **Status:** Active (2026-06-07)  
> **Supersedes:** ad-hoc “what’s in progress” notes scattered across milestone plans

Product UX polish (Home layout, nav tiers, Work Hub manifestations, chat presentation, normie onboarding Phases A–D) is **shipped**. The remaining blockers before “normie can extend Medousa without editing files” are:

---

## 1. Configuration reference (docs)

**Goal:** One authoritative env + file path catalog — not scattered grep archaeology.

| Deliverable | Status |
|-------------|--------|
| [`docs/configuration-reference.md`](../docs/configuration-reference.md) — grouped vars, defaults, audience | ✅ Started |
| `medousa doctor --config` (or doctor section) — effective values + missing secrets | ⬜ |
| Link from cookbook + Settings Diagnostics | ⬜ |

**Rule:** non-devs see product settings in Home. Power users get the full reference. Advanced/diagnostic vars stay documented but not in the wizard.

---

## 2. LLM providers in UI

**Goal:** genai’s 25+ providers exposed like Claude/Cursor — pick provider, paste key, done.

| Deliverable | Status |
|-------------|--------|
| Daemon: list supported providers + key env pattern | ✅ `providers_list` catalog |
| Home wizard + Settings: searchable provider picker | ✅ Voice + wizard BYOK |
| Secrets → `~/.local/share/medousa/secrets/` (existing pattern) | ✅ pattern exists |

---

## 3. MCP servers in UI

**Goal:** Add MCP server (name + URL or command + optional API key) without `mcp-gateway.toml`.

| Deliverable | Status |
|-------------|--------|
| Read gateway config + server health (Home Skills / Settings) | ✅ read-only catalog |
| Write API: add/edit/remove `[[servers]]`, restart gateway | ✅ Home Services tab |
| Home: “Add MCP server” form + test connection | ✅ Save & connect |
| HTTP/SSE MCP transport (not only stdio) | ✅ remote `url` + bearer token in Home + gateway client |

See [`docs/mcp-gateway-setup.md`](../docs/mcp-gateway-setup.md) for today’s file-based flow.

---

## 4. Capabilities in UI

**Goal:** Toggle bindings and web-search provider without `capabilities.toml`.

| Deliverable | Status |
|-------------|--------|
| Read `GET /v1/capabilities` in Skills → Tools | ✅ |
| Web search prefs (`[web_search]` in capabilities.toml) | ✅ Reach settings + capabilities overlay API |
| Enable/disable capability bindings | ✅ Tools detail toggles |

---

## 6. Home UI polish (streaming, thinking, work hygiene)

**Goal:** Mobile resume aligns with daemon; chat hides engine telemetry; work cards auto-hide/wipe with operator control.

| Deliverable | Status |
|-------------|--------|
| Phased plan + product decisions | ✅ [`home-ui-polish-plan.md`](home-ui-polish-plan.md) |
| B1/B4: Engine details toggle + chat stream filter | ✅ |
| B2: Verification badges hidden in chat | ✅ |
| A1/A2: Foreground resume + transcript reconcile | ✅ |
| D1–D3: Retention settings, tray clear, engine TTL + archive | ✅ (engine defaults 24h hide / 7d wipe) |
| C2: Mobile “Mac daemon defaults” chip | ⬜ |

---

| Deliverable | Status |
|-------------|--------|
| Rewrite [`architecture/README.md`](README.md) — current map + archive index | ✅ |
| Mark shipped milestone plans with dates | ✅ |
| [`architecture/archive/README.md`](archive/README.md) — historical index | ✅ |

---

## Explicitly not blockers (deferred)

| Item | Doc |
|------|-----|
| Phase E cloud auth | [normie-product-gap-analysis.md](normie-product-gap-analysis.md) |
| Phase F accessibility + prod packaging | [normie-onboarding-and-lan-pairing-plan.md](normie-onboarding-and-lan-pairing-plan.md) |
| P5 media & attachments | [media-and-attachments-plan.md](media-and-attachments-plan.md) |
| Work Hub W2 archive persistence | [medousa-home-work-hub-plan.md](medousa-home-work-hub-plan.md) |
| Loop FSM mock integration tests | [turn-state-machine-plan.md](turn-state-machine-plan.md) |

---

## Suggested implementation order

1. Finish configuration reference + doctor summary  
2. Provider picker + API key UI  
3. MCP add-server (daemon write path + Home form)  
4. Capabilities toggles (web search first)
