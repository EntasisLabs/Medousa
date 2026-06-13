# MCP gateway setup

The **MCP Client gateway** (`medousa_mcp_gateway`) is a separate process that runs MCP servers and exposes them to the Medousa daemon over HTTP. The main agent uses `cognition.mcp.*` tools, which call the gateway — not MCP directly.

## Quick start

```bash
# 1) Install starter config (if missing)
medousa setup   # wizard: enable "Start MCP gateway", or use --yes

# 2) Or start services manually in the background
medousa start daemon
medousa start mcp-gateway

# 3) Verify
medousa doctor
```

Default URLs:

| Service | URL |
|---------|-----|
| Daemon | `http://127.0.0.1:7419` |
| MCP gateway | `http://127.0.0.1:7420` |

Logs: `~/.local/share/medousa/logs/mcp-gateway.log` and `daemon.log`.

## Config file

Path: **`~/.config/medousa/mcp-gateway.toml`**

The setup wizard writes a starter file when none exists. To reset from the template in the repo, delete the file and run:

```bash
medousa setup --yes
# or copy from: medousa::mcp_gateway::STARTER_MCP_GATEWAY_TOML (see src/mcp_gateway/starter_config.rs)
```

### `[gateway]` section

| Field | Purpose |
|-------|---------|
| `bind` | Listen address (default `127.0.0.1:7420`) |
| `daemon_policy_url` | Daemon policy endpoint for invoke approval |
| `use_mock_fallback` | Synthetic tools when a server is down or `use_mock = true` |

### `[[servers]]` entries

Each server is one MCP connection the gateway manages.

| Field | Purpose |
|-------|---------|
| `id` | Stable id used in `cognition.mcp.invoke` (`server_id`) |
| `title` | Human label |
| `enabled` | `false` skips registration |
| `transport` | `stdio` (local command), `http` / `streamable` (POST JSON-RPC), or `sse` (legacy SSE + POST endpoint) |
| `command` / `args` | Stdio MCP server launch line |
| `url` | Remote MCP endpoint (`http://` or `https://`) |
| `bearer_token` | Optional bearer token for remote MCP auth |
| `use_mock` | `true` = mock tools only (no subprocess) |
| `allowed_lanes` | `interactive`, `scheduled`, … |
| `allowed_effect_classes` | Policy hints: `external_read`, `external_write`, `external_side_effect` |

## Example servers

### Mock (wizard default)

Good for local smoke tests without installing Node/Python MCP packages:

```toml
[[servers]]
id = "notion"
title = "Notion MCP (mock)"
enabled = true
transport = "stdio"
use_mock = true
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write", "external_side_effect"]
```

### Filesystem (real stdio)

```toml
[[servers]]
id = "filesystem"
title = "Filesystem MCP"
enabled = true
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/you/projects"]
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write"]
```

### Fetch (read-only web)

```toml
[[servers]]
id = "fetch"
title = "Fetch MCP"
enabled = true
transport = "stdio"
command = "uvx"
args = ["mcp-server-fetch"]
allowed_lanes = ["interactive"]
allowed_effect_classes = ["external_read"]
```

### Remote HTTP (hosted MCP gateway)

```toml
[[servers]]
id = "hosted"
title = "Hosted MCP"
enabled = true
transport = "http"
url = "https://mcp.example.com/mcp"
bearer_token = "your-token-if-required"
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write"]
```

Use `transport = "sse"` for legacy MCP servers that expose an SSE stream plus a separate message POST endpoint (common in older reference servers).

```toml
[[servers]]
id = "legacy"
title = "Legacy SSE MCP"
enabled = true
transport = "sse"
url = "https://mcp.example.com/sse"
allowed_lanes = ["interactive"]
allowed_effect_classes = ["external_read"]
```

After editing config, restart the gateway:

```bash
medousa start mcp-gateway
```

## Environment variables

| Variable | Used by |
|----------|---------|
| `MEDOUSA_MCP_GATEWAY_URL` | Daemon / TUI (default `http://127.0.0.1:7420`) |
| `MEDOUSA_MCP_GATEWAY_TOKEN` | Bearer on daemon → gateway requests |
| `MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN` | Admin routes (catalog refresh, invoke kill-switch) |
| `MEDOUSA_MCP_POLICY_TOKEN` | Gateway → daemon policy callback |
| `MEDOUSA_MCP_TURN_TOKEN_SECRET` | Turn-scoped invoke tokens (required for real invokes) |
| `MEDOUSA_MCP_GATEWAY_BIN` | Override path to `medousa_mcp_gateway` binary |

Set the same secrets on **daemon** and **gateway** when you enable auth.

## Background services (`medousa start`)

Foreground commands (`medousa daemon`, `medousa mcp` via binary) block the shell. For production-style local runs:

```bash
medousa start daemon [--backend surreal-mem] [--bind 127.0.0.1:7419]
medousa start mcp-gateway
medousa start discord|telegram|slack|whatsapp   # needs tokens in keyring / setup
medousa start all    # daemon + mcp-gateway + enabled adapters
```

Processes detach into a new session (no `nohup` required) and append logs under `~/.local/share/medousa/logs/`.

## Setup wizard notes

- **Install MCP gateway config** — writes `mcp-gateway.toml` only if the file is missing.
- **Start MCP gateway** — independent of install; if config is missing, setup creates it before start.
- If you already have a config file, leave "Install" off and keep **Start** on.

## Troubleshooting

| Symptom | Check |
|---------|--------|
| `cognition.mcp.*` fails | `medousa doctor` — gateway reachable? |
| Wizard says started but not reachable | `tail -f ~/.local/share/medousa/logs/mcp-gateway.log` |
| Spawn uses `cargo run` and dies | Install release binaries next to `medousa`, or set `MEDOUSA_MCP_GATEWAY_BIN` |
| No servers in catalog | `enabled = true` in TOML; mock servers need `use_mock = true` |

Architecture: [architecture/component-mcp-gateway.md](../architecture/component-mcp-gateway.md).
