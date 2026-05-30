# Medousa

**Turn chaotic life into stone.**

Medousa is a permanent AI workspace that lives on your computer. It remembers everything you tell it, verifies what it tells you, and keeps working even when you close the window. No cloud, no subscription, no hallucinated answers that you cannot trace.

You talk to it through a terminal interface, through Discord, or through Telegram. It runs in the background, processes your requests, and surfaces answers that carry proof — not just words.

## What it does

Medousa runs on your computer, not in a cloud. It remembers what you tell it, verifies what it tells you, and keeps working when you close the lid. You talk to it through a terminal, through Discord, or through Telegram. It runs scripts inside a sealed environment. It picks up where you left off, even days later. Every answer carries its own proof.

That is it. Everything else is infrastructure.

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

Medousa runs two processes. A background engine that never stops, and an interface that you talk to. The engine owns your history, your memory, and your recurring tasks. The interface connects to it over your local network and gives you a workspace for conversations, commands, and automations. You can also connect Discord or Telegram and message Medousa from across the room.

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

The workspace is a terminal interface with everything you need in one place. Turn history, slash commands, artifact previews, and a settings panel. It is fast. It connects to the background engine automatically. If the engine is not running, the workspace starts it.

## What makes it reliable

You are not watching Medousa when it works. That is the point.

When you send a message or schedule a check-in, Medousa converts it into a unit of work that cannot be lost. If your laptop goes to sleep, if the network drops, if the daemon restarts — that work waits. It retries. It picks up at the exact step that was interrupted, not from the beginning.

You never have to wonder whether something finished. If Medousa accepted it, it ran.

## What makes it safe

When Medousa runs a script — processing a spreadsheet, fetching a page, transforming a file — it runs inside a sealed environment. That script cannot touch your documents, your passwords, or your other applications unless you explicitly say it can.

You do not have to trust the script. You only have to trust the seal.

## What makes it remember

Medousa does not treat every conversation as a blank page. It builds a picture of how you work — not just what you say, but how you approach things. The questions you ask. The patterns you repeat. The context you keep coming back to.

When you return after a week away, Medousa does not ask "who are you?" It picks up where you left off. Not by scrolling through chat logs. By understanding what was relevant, what was resolved, and what was still in motion.

This is not a gimmick. It is the entire point.

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


---

### Chaos is not a personality trait. It is a failure of tools.

Every piece of software that forgets who you are, loses your work, or answers without proof is not your fault. It is a broken tool.

Medousa is built to be the opposite.

It remembers. It verifies. It finishes what it starts. It runs where you live — on your machine, in your chat, across your rooms. It does not guess. It does not forget. It does not leave you wondering whether something worked.

Chaotic life turns to stone when the tools around you stop adding to the noise.

That is what Medousa is for.