# CLI & workspace

Everything runs through the `medousa` launcher binary (installed to `~/.local/bin`).

Install first: [install-and-self-host.md](install-and-self-host.md)

---

## Core commands

```
medousa setup              Configure provider, model, backend, channels (TUI wizard)
medousa start <service>    Start engine, gateway, or channel bridge
medousa start daemon --inference   # spawns medousa_daemon + medousa_local (offline brain)
medousa tui                Terminal workspace (starts engine if needed)
medousa daemon             Foreground slim daemon (pass-through args)
medousa doctor             Health check (daemon + paths)
medousa doctor --local-engine   Also probe medousa_local on :7421
medousa models …           Local model management (power users)
medousa pair …             LAN phone pairing
```

### Start services

```bash
medousa start daemon --inference
medousa start daemon --public          # LAN bind for phone dev
medousa start mcp-gateway
medousa start discord | telegram | slack | whatsapp
medousa start all
medousa start daemon-restart --inference
```

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

Full-screen workspace: turn history, slash commands, artifact previews, settings panel, job rail beside the conversation.

Connects to Medousa Engine automatically; starts engine if not running (unless `--no-daemon`).

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
