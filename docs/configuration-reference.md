# Configuration reference

> **Status:** Active — operator reference (2026-06-07)  
> **Audience:** Power users, self-hosters, contributors  
> **Related:** [cookbook/install-and-self-host.md](cookbook/install-and-self-host.md), [mcp-gateway-setup.md](mcp-gateway-setup.md), [architecture/NEXT.md](../architecture/NEXT.md)

non-devs configure Medousa through the **app wizard and Settings**. This document lists **every environment override** the engine and app honor today. When a setting moves into the UI, it stays here for automation and CI.

**Check effective config:** `medousa doctor` (runtime health + paths). A dedicated `doctor --config` summary is planned ([NEXT.md](../architecture/NEXT.md)).

---

## Config files (preferred over env when possible)

| File | Purpose |
|------|---------|
| `~/.local/share/medousa/tui_defaults.json` | Provider, model, routing, tool policy, **work card retention** (shared with TUI + Home) |
| `~/.local/share/medousa/product_config.json` | Channels (Telegram, Slack, …) |
| `~/.config/medousa/capabilities.toml` | Capability bindings, web search prefs |
| `~/.config/medousa/mcp-gateway.toml` | MCP server launch config |
| `~/.config/medousa/manuscripts/` | Specialty YAML |
| `~/.local/share/medousa/secrets/` | API keys (preferred over raw env in production) |

Data dir override: set `MEDOUSA_DATA_DIR` or use Stasis legacy paths below.

---

## LLM & providers (genai)

Medousa resolves LLM settings in order: **saved defaults → env → built-in defaults**.

| Variable | Purpose | Default / notes |
|----------|---------|-----------------|
| `MEDOUSA_LLM_PROVIDER` | Active provider id | From `tui_defaults.json` |
| `MEDOUSA_LLM_MODEL` | Model id | From defaults |
| `MEDOUSA_LLM_BASE_URL` | Generic API base | Provider-specific if unset |
| `MEDOUSA_<PROVIDER>_API_KEY` | Provider API key | e.g. `MEDOUSA_OPENAI_API_KEY` |
| `MEDOUSA_<PROVIDER>_BASE_URL` | Provider endpoint | e.g. `MEDOUSA_DEEPSEEK_BASE_URL` |
| `STASIS_LLM_PROVIDER` | Legacy alias | Same as `MEDOUSA_*` |
| `STASIS_LLM_MODEL` | Legacy alias | |
| `STASIS_LLM_BASE_URL` | Legacy alias | |
| `STASIS_<PROVIDER>_API_KEY` | Legacy alias | e.g. `STASIS_DEEPSEEK_API_KEY` |
| `STASIS_<PROVIDER>_BASE_URL` | Legacy alias | |
| `OLLAMA_HOST` | Ollama host | Default `http://127.0.0.1:11434` |
| `MEDOUSA_OLLAMA_BASE_URL` | Ollama OpenAI-compat base | |
| `STASIS_OLLAMA_BASE_URL` | Legacy alias | |

**Local embedded brain:**

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_LOCAL_ENGINE_BIND` | Inference engine listen | `127.0.0.1:7421` |
| `MEDOUSA_LOCAL_ENGINE_BASE_URL` | Client URL for `medousa-local` | `http://127.0.0.1:7421/v1` |
| `MEDOUSA_LOCAL_ENGINE_CPU` | Force CPU-only inference | unset |
| `HF_TOKEN` | Hugging Face downloads (Gemma catalog) | unset |

25+ providers are supported via [rust-genai](https://github.com/jeremychone/rust-genai). The Home wizard currently surfaces OpenAI, Anthropic, Google, and Ollama only — see [NEXT.md](../architecture/NEXT.md).

---

## Engine & clients

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_DAEMON_URL` | Engine base URL for clients | `http://127.0.0.1:7419` |
| `STASIS_DAEMON_URL` | Legacy alias | |
| `MEDOUSA_DAEMON_PUBLIC_URL` | Advertised URL for SSE/stream (mobile/LAN) | Auto with `--public` |
| `MEDOUSA_DEV_HOST` | Dev Vite host → daemon URL hint | unset |
| `MEDOUSA_MEDOUSA_DAEMON_BIN` | Explicit path to `medousa_daemon` | Tauri sidecar or PATH |
| `MEDOUSA_PROJECT_ROOT` | Vault / project root override | unset |

**Tauri app (desktop dev):**

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_DAEMON_URL` | Default at desktop launch |
| `MEDOUSA_MEDOUSA_DAEMON_BIN` | Sidecar missing → PATH fallback |

**Channel adapter binaries:**

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_MEDOUSA_TELEGRAM_BIN` | Explicit telegram adapter path |
| `MEDOUSA_MEDOUSA_DISCORD_BIN` | Discord |
| `MEDOUSA_MEDOUSA_SLACK_BIN` | Slack |

---

## Storage (Surreal / Stasis)

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_SURREALKV_PATH` | KV database file | under data dir |
| `STASIS_SURREALKV_PATH` | Legacy alias | |
| `MEDOUSA_SURREAL_ENDPOINT` | Remote Surreal endpoint | file backend default |
| `STASIS_SURREAL_ENDPOINT` | Legacy alias | |

---

## MCP gateway

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_MCP_GATEWAY_URL` | Gateway base URL | `http://127.0.0.1:7420` |
| `MEDOUSA_MCP_GATEWAY_TOKEN` | Daemon → gateway auth | open if unset |
| `MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN` | Admin API auth | open if unset |
| `MEDOUSA_MCP_POLICY_TOKEN` | Gateway → daemon policy | open if unset |
| `MEDOUSA_MCP_TURN_TOKEN_SECRET` | Turn-scoped invoke HMAC | open if unset |

Config file: `~/.config/medousa/mcp-gateway.toml` — see [mcp-gateway-setup.md](mcp-gateway-setup.md).

---

## Capabilities & web search

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_WEB_SEARCH_PROVIDER` | Override `[web_search].preferred_provider` | from `capabilities.toml` |

File: `~/.config/medousa/capabilities.toml` — bindings for grapheme ops and MCP tools.

---

## Turn loop & execution

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_TURN_HOST_BUS` | Host bus tool profile | runtime default |
| `MEDOUSA_HOST_BUS_MAX_TOOL_ROUNDS` | Host round ceiling | policy default |
| `MEDOUSA_TURN_BUDGET_OPERATOR_GATE` | Require operator approve for budget extend | `0` / off |
| `MEDOUSA_PARALLEL_TOOL_CALLS_ENABLED` | Parallel tool invocations | policy default |
| `MEDOUSA_ALLOW_MUTATING_PARALLEL` | Parallel mutating tools | policy default |
| `MEDOUSA_LANE_SAFETY_BLOCK_RECURRING_ON_INTERACTIVE` | Block recurring jobs on chat lane | `false` |

---

## Grapheme & memory

| Variable | Purpose | Default |
|----------|---------|---------|
| `MEDOUSA_GRAPHEME_COMPACTION_TRIGGER_BYTES` | Grapheme payload compaction threshold | runtime default |
| `MEDOUSA_GRAPHEME_COMPACTION_INLINE_NOTICE` | Inline notice when compacting | runtime default |
| `MEDOUSA_MEMORY_INGEST_PROFILE` | Memory ingest profile | unset |
| `MEDOUSA_IDENTITY_USER_TIMEZONE` | User timezone for ambient context | `TZ` or UTC |

**Locus graph (daemon startup):**

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_SKIP_LOCUS_INIT_ON_DAEMON` | Skip Locus schema init (large DB) |
| `MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON` | Force init even if tables exist |
| `LOCUS_MCP_PARSE_PROFILE` | Locus MCP parse profile |

---

## Pairing & LAN (mobile)

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_PEER_NAME` | mDNS / pairing display name |
| `MEDOUSA_MDNS_DISABLE` | Disable Bonjour advertise |
| `MEDOUSA_PAIRING_DISABLE` | Disable pairing endpoints |
| `MEDOUSA_PAIRING_ADVERTISE` | Force mDNS when not `--public` |
| `MEDOUSA_PAIRING_DISABLE_TLS` | Dev only — plaintext pairing |

See [normie-onboarding-and-lan-pairing-plan.md](../architecture/normie-onboarding-and-lan-pairing-plan.md).

---

## Channels & delivery

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_TELEGRAM_BOT_TOKEN` | Telegram bot token (also `TELOXIDE_TOKEN`) |
| `MEDOUSA_TELEGRAM_TOKEN` | Alias |
| `MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED` | Heartbeat nudges |
| `MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS` | Chat IDs for nudges |
| `MEDOUSA_DISCORD_COMMAND_PREFIX` | Discord prefix |
| `MEDOUSA_DISCORD_HEARTBEAT_*` | Discord heartbeat |
| `MEDOUSA_SLACK_HEARTBEAT_*` | Slack heartbeat |
| `MEDOUSA_DELIVER_WEBHOOK_TOKEN` | Internal deliver webhook auth |
| `MEDOUSA_WHATSAPP_DELIVER_URL` | WhatsApp deliver endpoint |
| `MEDOUSA_WHATSAPP_DELIVER_BIND` | WhatsApp deliver bind |

Channel tokens belong in `product_config.json` for normie flow; env vars remain for headless adapters.

---

## Heartbeat & dashboard (daemon)

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_HEARTBEAT_WEBHOOK_URL` | Outbound heartbeat webhook |
| `MEDOUSA_HEARTBEAT_JSONL` | JSONL heartbeat log path |
| `MEDOUSA_HEARTBEAT_AGENT_TURN_ENABLED` | Agent turn on heartbeat |
| `MEDOUSA_HEARTBEAT_MIN_SIGNIFICANCE` | Min score to notify |
| `MEDOUSA_HEARTBEAT_*_WEIGHT` | Scoring weights |
| `MEDOUSA_HEARTBEAT_MIN_NOTIFY_INTERVAL_SECS` | Notify throttle |
| `MEDOUSA_HEARTBEAT_QUIET_START_HOUR_UTC` | Quiet hours |
| `MEDOUSA_HEARTBEAT_QUIET_END_HOUR_UTC` | Quiet hours |
| `MEDOUSA_DASHBOARD_ACTION_BEARER_TOKEN` | Dashboard actions |
| `MEDOUSA_DASHBOARD_ACTION_REQUIRED_ROLE` | RBAC role |
| `MEDOUSA_DASHBOARD_ACTION_ROLE_CLAIM_HEADER` | Role claim header |

---

## Workspace retention

Controls when terminal work cards disappear from the board and when archived ask jobs / turn workers are purged from disk.

| Source | Key | Default | Range |
|--------|-----|---------|-------|
| `tui_defaults.json` | `work_card_hide_after_hours` | `24` | 1–168 |
| `tui_defaults.json` | `work_card_wipe_after_days` | `7` | 1–90 |
| Env override | `MEDOUSA_WORK_CARD_HIDE_AFTER_HOURS` | — | 1–168 |
| Env override | `MEDOUSA_WORK_CARD_WIPE_AFTER_DAYS` | — | 1–90 |

**Hide** applies to terminal done, failed, stopped, and cancelled cards in workspace projections. **Wipe** applies to archived rows in `workspace/ask_jobs.json` and `workspace/turn_workers.json`.

Home Settings → Rhythm writes these fields to `tui_defaults.json` on Mac desktop. Mobile reads live values from `GET /v1/runtime/defaults` after connecting to the daemon.

---

## OpenShell & observability

| Variable | Purpose |
|----------|---------|
| `OPENSHELL_GATEWAY` | OpenShell gateway name |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry export |
| `OTEL_SERVICE_NAME` | Service name |
| `STASIS_OTEL_SERVICE_NAME` | Legacy alias |

---

## TUI

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_TUI_LOCAL_RUNTIME` | TUI uses in-process runtime vs daemon |

---

## Stasis legacy prefix

Many variables accept **`STASIS_*`** as an alias for **`MEDOUSA_*`**. New deployments should prefer `MEDOUSA_*`. `workshop_env.rs` mirrors provider keys into both prefixes when loading secrets.

---

## Changelog

| Date | Change |
|------|--------|
| 2026-06-07 | Initial catalog — grouped from codebase grep + cookbook |
