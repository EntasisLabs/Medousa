# Install & self-host

For **downloading the Medousa app**, see the [product README](../README.md).

This guide is for running **Medousa Engine** and CLI tools on a machine you control.

---

## Install binaries

### Medousa Installer (recommended)

Download **Medousa Installer** from your release endpoint:

```
{MEDOUSA_RELEASE_BASE_URL}/stable/installer-bootstrap.json
```

The installer manages Desktop, Engine, adapters, offline brain, and model packs.

**Self-hosted / R2:** see [release-to-r2.md](release-to-r2.md) for Cloudflare R2 upload, signing, and landing-page wiring.

### CLI / headless install

One command installs the full versioned set (launcher, engine, TUI, channel adapters) to `~/.local/bin`:

```bash
export MEDOUSA_RELEASE_BASE_URL=https://releases.example.com/medousa
./scripts/install.sh
```

Self-hosted registry with pinned version:

```bash
./scripts/install.sh --registry-url https://releases.example.com/medousa --version v0.1.0
```

Air-gap / local artifact:

```bash
./scripts/install.sh --from-dist dist/medousa-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Verify install:

```bash
./scripts/install.sh --verify-only
medousa doctor
```

---

## First-time setup (TUI wizard)

Power-user configuration — provider, model, backend, channels:

```bash
medousa setup
```

Non-interactive example:

```bash
medousa setup --yes --provider ollama --model llama3.2
```

For **private Gemma brain** (same as the app’s offline path):

```bash
medousa start daemon --inference
medousa models probe
medousa models download gemma-4-e4b-it --wait
medousa models engine-load --model gemma-4-e4b-it
```

non-devs should use the **Medousa app** instead of this flow.

---

## Start services

```bash
medousa start daemon --inference   # engine + private brain (dev / server)
medousa start daemon               # engine only
medousa start mcp-gateway          # MCP broker (optional)
medousa start all                  # engine + gateway + configured adapters
medousa start daemon-restart --inference   # recover wedged engine
```

Logs: `~/.local/share/medousa/logs/<service>.log`

---

## Where data lives

| What | Where |
|------|--------|
| Database | `~/.local/share/medousa/runtime.surrealkv` |
| Settings | `~/.local/share/medousa/product_config.json` |
| Workspace prefs | `~/.local/share/medousa/tui_defaults.json` |
| History | `~/.local/share/medousa/history/` |
| Secrets | `~/.local/share/medousa/secrets/` |
| Capabilities | `~/.config/medousa/capabilities.toml` |
| MCP gateway | `~/.config/medousa/mcp-gateway.toml` |
| Specialties | `~/.config/medousa/manuscripts/` |
| Logs | `~/.local/share/medousa/logs/` |

Override DB path: `--backend surreal-kv:/path` or `MEDOUSA_SURREALKV_PATH`.

---

## Environment

See **[Configuration reference](configuration-reference.md)** for the full catalog (LLM, MCP, Locus, grapheme, pairing, channels).

Quick table — most common overrides:

| Variable | Purpose |
|----------|---------|
| `MEDOUSA_LLM_PROVIDER` | Provider name |
| `MEDOUSA_LLM_MODEL` | Model |
| `MEDOUSA_LLM_BASE_URL` | API base URL |
| `MEDOUSA_DAEMON_URL` | Engine URL for clients |
| `MEDOUSA_SURREALKV_PATH` | Database file |
| `MEDOUSA_MCP_GATEWAY_URL` | Gateway (default `http://127.0.0.1:7420`) |
| `HF_TOKEN` | Hugging Face token for Gemma downloads |
| `MEDOUSA_LOCAL_ENGINE_BIND` | Local inference bind (default `127.0.0.1:7421`) |

Provider URLs: `MEDOUSA_<PROVIDER>_BASE_URL` or `STASIS_<PROVIDER>_BASE_URL`. Ollama: `OLLAMA_HOST`.

---

## Providers

25+ LLM providers via [genai](https://github.com/jeremychone/rust-genai):

- **OpenAI** — default `gpt-4o-mini`
- **Ollama** — local, auto-detected on `127.0.0.1:11434`
- **medousa-local** — embedded Gemma via engine `:7421`
- **Custom** — any genai-supported provider

---

## Reliability & safety (operator view)

- **Stasis** — jobs survive sleep, restart, network blips; retried until done or dead-letter.
- **OpenShell** — skill scripts run sandboxed; see [openshell-handoff-setup.md](../openshell-handoff-setup.md).
- **Identity** — export with `medousa identity-export`; teach facts with `medousa identity-remember`.

Infrastructure family: [Stasis](https://github.com/EntasisLabs/stasis), [Locus](https://github.com/EntasisLabs/locus), [Resonantia](https://resonantia.me).
