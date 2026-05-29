# Medousa

Medousa is a guided AI workspace for people who want to ask questions, get useful answers, and keep a clear trail of what happened.

Core surfaces:

- `medousa`: launcher (`onboard`, `tui`, `daemon`, `discord`, `telegram`, `doctor`)
- `medousa_tui`: interactive workspace
- `medousa_cli`: one-shot client and daemon controls
- `medousa_daemon`: long-running API and scheduler
- `medousa_telegram`: Telegram ingress adapter
- `medousa_discord`: Discord ingress adapter

## Get Started in 60 Seconds

### Prerequisites

- Rust toolchain (`cargo`)
- One model provider:
  - OpenAI-compatible API key, or
  - Local Ollama instance

### One Command Setup

Install the binaries and run onboarding:

```bash
cargo install --path .
medousa setup
```

The setup wizard is guided and fast:

1. Checks your machine and suggests the best provider default
2. Captures model and API key only when needed
3. Lets you configure Discord and Telegram bot tokens in the same flow
4. Lets you set Telegram sender allowlist IDs in the same flow
5. Saves defaults so you do not repeat setup next time
6. Starts local runtime automatically (unless you opt out)
7. Can start Discord and Telegram adapters right away
8. Opens the chat workspace immediately

The interactive setup uses a ratatui wizard experience:

- `Enter`: next/confirm
- `Left`: previous step
- `Up/Down`: change selection
- `Space`: toggle yes/no options
- `Esc`: cancel without saving

Alias commands are identical:

```bash
medousa onboard
medousa init
```

If you are running from source without installing:

```bash
cargo run -p medousa --bin medousa -- setup
```

## Daily Commands

Open workspace (auto-start daemon if needed):

```bash
medousa tui
```

Check current setup, daemon reachability, and key presence:

```bash
medousa doctor
```

Run daemon in foreground:

```bash
medousa daemon
```

Run adapters through the launcher (uses stored token from setup when available):

```bash
medousa discord
medousa telegram
```

Non-interactive onboarding example:

```bash
medousa setup --yes --provider ollama --model llama3.2 --no-daemon --no-tui
```

Advanced setup options (backend + daemon URL prompts):

```bash
medousa setup --advanced
```

## Operator First-Run Checks (Recommended)

Once daemon is up, validate runtime health and report behavior:

```bash
medousa_cli daemon-first-run --daemon-url http://127.0.0.1:7419
medousa_cli daemon-report "Summarize 3 practical runtime trends with citations" --daemon-url http://127.0.0.1:7419 --poll-timeout-ms 30000
```

## Identity Continuity Workflow

Inspect current continuity context:

```bash
medousa_cli daemon-identity-inspect --daemon-url http://127.0.0.1:7419
```

Propose an update:

```bash
medousa_cli daemon-identity-update user demo-user '{"timezone":"UTC","language_variant":"en-US"}' --reason "seed continuity demo" --daemon-url http://127.0.0.1:7419
```

Review and explain recent changes:

```bash
medousa_cli daemon-identity-review user demo-user --daemon-url http://127.0.0.1:7419
medousa_cli daemon-identity-explain user demo-user --daemon-url http://127.0.0.1:7419
```

Rollback remains explicit:

```bash
medousa_cli daemon-identity-rollback user demo-user <target_version> --reason "manual continuity rollback" --approver medousa-cli --daemon-url http://127.0.0.1:7419
```

## Adapter Ingress

### Telegram

Quick path via launcher:

```bash
medousa telegram
```

Direct binary:

```bash
export MEDOUSA_TELEGRAM_BOT_TOKEN=<your-telegram-bot-token>
medousa_telegram --daemon-url http://127.0.0.1:7419 --allow-commands help,health,heartbeat,ask --max-prompt-chars 1400
```

Plain-text ingress is disabled by default unless `text` is added to `--allow-commands`.

Sender allowlist (recommended for production):

```bash
export MEDOUSA_TELEGRAM_ALLOW_USER_IDS="123456789,987654321"
medousa telegram --allow-user-ids 123456789,987654321
```

The adapter will ignore messages from users not in this allowlist.
When configured in `medousa setup`, the launcher applies it automatically for `medousa telegram`.

Optional safety overrides:

```bash
export MEDOUSA_TELEGRAM_MAX_PROMPT_CHARS_BY_CHAT="-1001234567890:1000,123456789:700"
```

Optional proactive heartbeat nudges:

```bash
export MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED=true
export MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS="-1001234567890,123456789"
export MEDOUSA_TELEGRAM_HEARTBEAT_MIN_SIGNIFICANCE=0.75
```

### Discord

Quick path via launcher:

```bash
medousa discord
```

Direct binary:

```bash
export MEDOUSA_DISCORD_BOT_TOKEN=<your-discord-bot-token>
medousa_discord --daemon-url http://127.0.0.1:7419 --command-prefix ! --allow-commands help,health,heartbeat,ask --max-prompt-chars 1400
```

Plain-text ingress is disabled by default unless `text` is added to `--allow-commands`.

Optional safety overrides:

```bash
export MEDOUSA_DISCORD_MAX_PROMPT_CHARS_BY_CHANNEL="123456789012345678:1000,234567890123456789:700"
```

Optional proactive heartbeat nudges:

```bash
export MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED=true
export MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS="123456789012345678,234567890123456789"
export MEDOUSA_DISCORD_HEARTBEAT_MIN_SIGNIFICANCE=0.75
```

## Runtime Configuration

Environment examples:

```bash
export STASIS_LLM_PROVIDER=openai
export STASIS_LLM_MODEL=gpt-4o-mini
```

Ollama example:

```bash
export STASIS_LLM_PROVIDER=ollama
export STASIS_LLM_MODEL=llama3.2
export MEDOUSA_OLLAMA_BASE_URL=http://localhost:11434/v1/
```

## Typical Flows

Flow A: interactive investigation loop

1. Start in chat and ask a question.
2. Inspect tool/runtime behavior in observability output.
3. Use artifact and verification commands to inspect trust signals.
4. Adjust routing/settings/depth as needed.

Flow B: script execution and validation

1. Open editor (`/edit` or `/open`).
2. Run script (`/run` or `/run-current`).
3. Review runtime diagnostics and job outcomes.
4. Persist or export relevant output.

Flow C: service operation

1. Run daemon for continuous scheduling.
2. Enqueue ask/prompt/recurring work via API or CLI.
3. Track health and outcomes through API and logs.

## Persistence and Data Locations

Local data is stored under:

- `~/.local/share/medousa/history/`
- `~/.local/share/medousa/tui_defaults.json`
- `~/.local/share/medousa/last_session`
- `~/.local/share/medousa/onboard_profile.json`
- `~/.local/share/medousa/logs/daemon.log`
- `~/.local/share/medousa/logs/discord.log`
- `~/.local/share/medousa/logs/telegram.log`
- `~/.local/share/medousa/secrets/api_key` (file fallback when keyring is unavailable)
- `~/.local/share/medousa/secrets/discord_bot_token` (file fallback when keyring is unavailable)
- `~/.local/share/medousa/secrets/telegram_bot_token` (file fallback when keyring is unavailable)

Secrets (API key and bot tokens) use OS keyring when available.

## Architecture References

- [architecture/README.md](architecture/README.md)
- [architecture/enterprise-architecture-and-flow-guide.md](architecture/enterprise-architecture-and-flow-guide.md)
- [architecture/system-overview.md](architecture/system-overview.md)
- [architecture/component-tui.md](architecture/component-tui.md)
- [architecture/component-cli.md](architecture/component-cli.md)
- [architecture/component-daemon.md](architecture/component-daemon.md)
- [architecture/interaction-and-state-model.md](architecture/interaction-and-state-model.md)