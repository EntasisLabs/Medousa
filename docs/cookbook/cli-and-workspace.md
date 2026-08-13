# CLI & workspace

Power-user operator surface. Everyday chat → open Medousa. This doc is for engine lifecycle, scripting, and troubleshooting.

Install: [install-and-self-host.md](install-and-self-host.md) · Headless: `./scripts/install.sh --profile headless-server`

---

## Core commands

```
medousa status             Engine bind, health, data dir
medousa stop [--local-engine]
medousa doctor --config [--json]
medousa setup --yes        Non-interactive bootstrap (flags/env)
medousa start daemon --inference
medousa tui                Terminal workspace
medousa pull <name>        Install CDN package into data dir (engine, mcp-gateway, telegram, …)
medousa update [<name>]    Update installed packages when newer
medousa packages status    Local vs remote package versions
medousa pair …             LAN / QR pairing
medousa iroh …             Relay smoke / tickets
```
Desktop Settings now read/write **per-engine** `tui_defaults.json` via `GET/PUT /v1/runtime/tui-defaults` (not host-global file).

### Start services

```bash
medousa start daemon --inference
medousa start daemon --public          # UNSAFE full LAN API bind; isolated development only
medousa start mcp-gateway
medousa start discord | telegram | slack | whatsapp
medousa start all
medousa start daemon-restart --inference
```

`--public` does not currently make pairing credentials mandatory across the
complete daemon router. Do not use it on the internet, guest Wi-Fi, or an
untrusted LAN. Prefer Iroh; see [Mobile & LAN](mobile-and-lan.md) and
[hardening H01](../../architecture/hardening/README.md).

### Local models (`medousa models`)

```
medousa models probe
medousa models catalog
medousa models list
medousa models download <model-id> [--wait]
medousa models remove <model-id>
medousa models engine-status
medousa models engine-load [--model <id>]
```

### Identity & specialties

```
medousa identity-export [--user-id <id>] [--dir <path>]
medousa identity-remember --kind preference|person|note --subject … --statement …
medousa manuscript-list
medousa manuscript-install <path-to.yaml> [--project]
medousa skill-import <path> [--from-hermes|--from-openclaw|--from-cursor]
medousa openshell-probe [<manuscript-id>]
medousa workspace …
medousa vault …
```

Run `medousa <command> --help` for flags.

---

## medousa-cli (HTTP helpers)

```
medousa-cli daemon-health [--daemon-url <url>]
medousa-cli daemon-ask <prompt>
medousa-cli daemon-report <query>
medousa-cli daemon-watch-add <cron> <prompt>
medousa-cli daemon-identity-context …
```

Useful for scripts and CI calling the engine without the TUI.

---

## Terminal workspace (`medousa tui`)

Full-screen workspace: turn history, slash commands, artifact previews, settings panel, job rail beside the conversation. Home and the TUI are sibling workshop shells over the same daemon — panes for chat, notes, code, review, and terminal (see [tui-home-workspace-parity-plan](../../architecture/tui-home-workspace-parity-plan.md)).

Artifacts: [artifacts-and-presentations.md](artifacts-and-presentations.md) · Vault: [vault-and-library.md](vault-and-library.md)

Connects to Medousa Engine automatically; starts engine if not running (unless `--no-daemon`).

### Connection (workshop switch)

Settings label in Home is **Connection**. In the TUI:

| Action | How |
|--------|-----|
| Open picker | `Ctrl+; w` · `/connection` · command palette |
| Switch URL | Enter on a row, or `/connection http://host:7419` |
| Browse LAN | `l` in the picker (mDNS; works without a reachable daemon) |
| Paste URL | `u` in the picker |

The picker lists **Local**, workshops from Home’s `{dataDir}/workshops.json` (read-only), recent daemons, and LAN discoveries after browse. Pane layout is scoped per workshop under `tui_workspaces/{scope}/` (same idea as Home’s `medousa-home-workspace-session-v4:{workshopId}`).

### Chat handoff with Home

Chat turns live on the **daemon** (`/v1/sessions/{id}/history`). Point TUI and Home at the **same workshop URL** and use the **same `session_id`** (`--session` / focused chat tab / Home’s last session) — there is no fork. Pane layouts stay shell-local (TUI JSON vs Home localStorage).

### Forge, notes, terminal

| Action | Keys |
|--------|------|
| Seal code lease → Review | `Ctrl+E` / `/seal` (saves dirty buffer first) |
| Review dispositions | `a` approve · `f` finish · `u` restore |
| Note tree / buffer / links | `Tab` cycle · Enter open from tree or links |
| Note save conflict | `Ctrl+R` reload from vault · `Ctrl+Y` keep mine (omit If-Match) |
| Terminal scrollback | `PgUp` / `PgDn` · `Esc` jump to live bottom |

---

## Skill learning tools (worker lane)

| Tool | Lane | Role |
|------|------|------|
| `cognition_skill_discover` | host + worker | Inventory scripts + risk before import |
| `cognition_skill_propose` | host + worker | Policy level gate |
| `cognition_skill_probe` | worker | Run skill in OpenShell sandbox |
| `cognition_openshell_sandbox_run` | worker | Ad-hoc sandbox command |

See [skills-and-specialties.md](skills-and-specialties.md).

---

## From source

```bash
git clone https://github.com/EntasisLabs/Medousa.git
cd Medousa
./scripts/install.sh --from-source
medousa setup
```

Details: [build-from-source.md](build-from-source.md)
