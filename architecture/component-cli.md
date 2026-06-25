# Component: CLI surfaces

## Dual-surface product model

| Surface | Audience | Job |
|---------|----------|-----|
| **Medousa Home** | Normie operators | Felt experience — chat, vault, settings without terminal |
| **`medousa` CLI** | Power users / headless | Run engine, diagnose, automate, script |
| **`medousa-cli`** | HTTP/script helpers | One-shot daemon API calls |

CLI is operator-first: honest framing, plain language, scriptable (`--json`). Curious normies are welcome — no gatekeeping — but the terminal is not the primary onboarding path.

## Entry points

- **`medousa`** — `src/bin/medousa.rs` — lifecycle, diagnose, workshop, configure, channels
- **`medousa-cli`** — `src/bin/medousa_cli.rs` — `daemon-ask`, `daemon-health`, etc.
- **`medousa_tui`** — terminal workspace (optional; not required for headless)

## Command taxonomy (`medousa --help`)

1. **Lifecycle** — `start`, `stop`, `status`, `daemon`
2. **Diagnose** — `doctor [--config] [--json]`, `models probe`
3. **Workshop** — `workspace`, `vault`, `pair`, `iroh`
4. **Configure** — `setup --yes`, `doctor --config`
5. **Extend** — identity, manuscripts, skills
6. **Channels** — discord, telegram, slack, whatsapp (secondary)

## Headless operator path

```bash
./scripts/install.sh --profile headless-server --from-source
medousa setup --yes   # non-interactive when flags provided
medousa start daemon
medousa doctor --config --json
medousa status
```

Docker: `Dockerfile` + `docker-compose.yml`. systemd: `contrib/systemd/medousa-engine.service`.

## Related

- [cli-and-workspace.md](../docs/cookbook/cli-and-workspace.md)
- [road-to-production-plan.md](../architecture/road-to-production-plan.md)
