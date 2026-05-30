# Medousa

**Turn chaotic life into stone.**

Medousa is a permanent AI workspace that lives on your computer. It remembers everything you tell it, verifies what it tells you, and keeps working even when you close the window. No cloud, no subscription, no hallucinated answers that you cannot trace.

You talk to it through a terminal interface, through Discord, or through Telegram. It runs in the background, processes your requests, and surfaces answers that carry proof — not just words.

## What it does

Everything in Medousa starts with a conversation. You ask something. It thinks. It writes back. But the difference is what happens underneath.

When you ask a question, Medousa does not just generate a reply. It runs tool loops. It looks up stored context from previous sessions. It captures each step as an immutable record. The result is a turn that carries its own lineage — every chunk of text is tagged as verified or provisional alongside the source it came from. You never guess whether the answer is real.

It works when the terminal is closed. The background daemon processes recurring prompts, keeps session history, and listens for incoming messages from your messaging bridges. You can message it from across the room and get the same answers you would at the keyboard.

## What you can do with it

These are the things Medousa does out of the box:

| You need to... | So you... |
|---|---|
| Remember where you left off | Ask Medousa. It keeps session history and identity memory across days. |
| Get answers you can trust | Every turn shows you the verification trail. No flat strings. |
| Automate a weekly check-in | Set a recurring prompt. The daemon runs it, and the result waits for you. |
| Reach it from another room | Connect Discord or Telegram. Message it from your phone. |
| Run it fully offline | Use the Ollama provider with a local model. No network required. |
| Check if everything is healthy | Run `medousa doctor`. It tests the model, the daemon, and your bridges in one pass. |
| Start over on a fresh machine | Run `medousa setup`. The wizard walks through provider, model, backend, adapters, and daemon start in one flow. |

## How it works

Medousa is two layers running together.

The **daemon** is the background engine. It owns the runtime state, session history, identity memory, and verification store. It processes turns, runs recurring prompts, and serves the chat interface. It binds to `127.0.0.1:7419` by default and keeps working whether you are looking at the terminal or not.

The **TUI** is the interface you interact with. It connects to the daemon and gives you a rich terminal workspace with commands, slash commands, artifact previews, turn history, and a markdown renderer. It is fast — built on a rendering loop that targets 60 fps on a 1-second refresh budget.

The **bridges** (Discord and Telegram) connect Medousa to your messaging apps. Each adapter is a separate binary that authenticates with your bot token and relays messages through the daemon. You can run them in the background alongside the daemon.

The **backend** stores your data. Choose between three options:

- **In-memory** — data lives while the daemon runs. Fastest, ephemeral.
- **SurrealKV** — data persisted to a local file. Survives restarts. No external database needed.
- **SurrealWS** — data persisted to a remote SurrealDB instance. For multi-machine setups or shared state.

## Quick start

```
medousa setup
```

The wizard detects your local Ollama installation, walks through provider configuration, backend selection, and adapter setup, then starts the daemon and opens the chat interface. It takes about sixty seconds.

If you prefer non-interactive:

```
medousa setup --yes --provider ollama --model llama3.2
```

## The commands

Everything is controlled through the `medousa` binary:

```
medousa setup      Configure provider, model, backend, and adapters in one pass
medousa tui        Open the chat workspace (starts daemon automatically)
medousa daemon     Start the background engine explicitly
medousa discord    Connect the Discord bridge
medousa telegram   Connect the Telegram bridge
medousa doctor     Run diagnostics on the full stack
```

Each command has its own flags. Run `medousa <command> --help` for details.

## The workspace

When you open the TUI, you enter a terminal workspace with:

- **Turn history** — every past exchange is stored and searchable by session
- **Slash commands** — quick actions for settings, artifacts, and stage routing
- **Artifact preview** — chunked outputs from tool calls rendered inline
- **Agent runtime** — multi-step reasoning loops that collect evidence before replying
- **Settings UI** — configure provider, model, backend, theme, and key bindings on the fly
- **Markdown rendering** — formatted output with syntax highlighting

The workspace connects to the daemon through an HTTP API. If the daemon is not running, the TUI starts it automatically.

## Storage

By default, Medousa keeps data in your local data directory:

| What | Where |
|---|---|
| Runtime database (SurrealKV) | `~/.local/share/medousa/runtime.surrealkv` |
| Daemon log | `~/.local/share/medousa/logs/daemon.log` |
| Discord log | `~/.local/share/medousa/logs/discord.log` |
| Telegram log | `~/.local/share/medousa/logs/telegram.log` |
| Onboarding profile | `~/.local/share/medousa/onboard_profile.json` |

You can override the database path with the `--backend` flag or the `MEDOUSA_SURREALKV_PATH` environment variable.

## Providers

Medousa supports any OpenAI-compatible chat provider. You can switch at setup time or later in the TUI settings:

- **OpenAI** (default) — `gpt-4o-mini`
- **Ollama** (local) — `llama3.2`, auto-detected on `127.0.0.1:11434`
- **Custom** — any provider with an OpenAI-compatible endpoint

Set the `MEDOUSA_LLM_PROVIDER`, `MEDOUSA_LLM_MODEL`, and `MEDOUSA_LLM_BASE_URL` environment variables to configure without the wizard.

## Environment

| Variable | Purpose |
|---|---|
| `MEDOUSA_LLM_PROVIDER` | Provider name (openai, ollama, custom) |
| `MEDOUSA_LLM_MODEL` | Model identifier |
| `MEDOUSA_LLM_BASE_URL` | Base URL for the provider API |
| `MEDOUSA_SURREALKV_PATH` | Path for the SurrealKV database file |
| `MEDOUSA_SURREAL_NAMESPACE` | SurrealDB namespace (default: medousa) |
| `MEDOUSA_SURREAL_DATABASE` | SurrealDB database (default: runtime) |

Provider-specific base URLs can also be set with `MEDOUSA_<PROVIDER>_BASE_URL` or `STASIS_<PROVIDER>_BASE_URL`. For Ollama, `OLLAMA_HOST` is honoured automatically.
