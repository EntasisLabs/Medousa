
<div align="center" >
  
<img src="assets/medousa-blk.png" alt="Medousa Logo" width="300">

</div>

<h1 style="text-align: center;">Medousa</h1>

**Turn chaotic life into stone.**

Medousa is a permanent AI workspace that lives on your computer. It remembers everything you tell it, verifies what it tells you, and keeps working even when you close the window.

One brain. Your terminal, your chats, your scheduled work, your memory — not four apps pretending to be one.

You talk to it from the workspace, from Discord, Telegram, Slack, or WhatsApp. It runs in the background, processes your requests, and surfaces answers that carry proof — not just words.

 **No cloud, no subscription, no hallucinated answers that you cannot trace.** 
  
## How it works

Medousa runs two processes. A background engine that never stops, and an interface that you talk to. The engine holds your history, your memory, and your recurring tasks. The interface connects to it over your local network and gives you a workspace for conversations, commands, and automations.

Every channel hits that same engine. Message from your phone, your team chat, or the terminal — same assistant, same history, same rules. Until you say `/new`.

## What you can do with it

These are the things Medousa does out of the box:

| You need to... | So you... |
|---|---|
| Remember where you left off | Ask Medousa. It keeps your history — and who you are — across days. |
| Stop re-introducing yourself | Tell it once. Your preferences, your people, your rhythms. It builds a picture of you that gets sharper over time. |
| Wear a different hat on command | Switch to a **specialty** — or import your existing `SKILL.md` libraries from Cursor, Hermes, or OpenClaw. No rewrite required. |
| Get answers you can trust | For live facts it goes out, gathers sources, and shows you the trail. No flat strings. |
| Automate what actually matters | Schedule a check-in, a report, or a full working session. Medousa runs it while you sleep. The answer finds you. |
| Hand off the heavy lifting | Send the big job to the background. One clear answer comes back — not a pile of half-finished threads. |
| Plug in what you already use | Connect the tools and services you rely on. Medousa learns what they can do — and asks before it acts. |
| Reach it from anywhere | Discord, Telegram, Slack, WhatsApp. Text `/brief` on Telegram and your morning summary starts. |
| Get checked in on | Turn on proactive nudges. Medousa reaches out on a rhythm you choose — with reasoning, not noise. |
| Run it fully offline | Point it at a local model. Your machine. Your rules. No network required. |
| Know it's healthy | Run `medousa doctor`. One pass. Model, engine, bridges — checked. |
| Start fresh on a new machine | Run `medousa setup`. About sixty seconds. You're back. |


## Powered by Stasis, Locus, and Resonantia

The reliability is not hand-waved. Medousa is built on the same infrastructure family as **[Resonantia](https://resonantia.me)** — not a chat wrapper pretending to be durable.

**[Stasis](https://github.com/EntasisLabs/stasis)** — the engine underneath. When you send something, schedule something, or ask for something later, Stasis makes it real work — saved, retried, finished, delivered back to you. Your laptop sleeps. The network drops. The app restarts. The work does not vanish.

**[Locus](https://github.com/EntasisLabs/locus)** — the memory layer. What matters becomes structured, timestamped, retrievable memory — not a scrollback dump. Medousa writes it, recalls it across turns, and stops treating every conversation like day one.

**[Resonantia](https://resonantia.me)** — the sibling on the same protocol. Where Medousa is the workspace that runs and remembers on your machine, Resonantia is the terrain you navigate — your mind, made visible. Same foundation. Different surface. If you want to *see* what this memory looks like when it becomes a map, start there.

Medousa is the brain. Stasis makes work finish. Locus makes memory stick. Resonantia shows what that memory can become.


## Install and run

**Install** — one command. Pulls the full binary set (launcher, daemon, TUI, and channel adapters) and installs to `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install.sh | bash
```

Pin a release if you prefer:

```bash
curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install.sh | bash -s -- --version v0.1.0
```

**Set up** — first-time configuration. The wizard detects your local Ollama installation, walks through provider configuration, backend selection, and channel setup, then starts the daemon and opens the chat interface. About sixty seconds:

```bash
medousa setup
```

**Verify** — one pass over the stack:

```bash
medousa doctor
```

If you prefer non-interactive setup:

```bash
medousa setup --yes --provider ollama --model llama3.2
```

**From source** (developers):

```bash
git clone https://github.com/EntasisLabs/Medousa.git
cd Medousa
./scripts/install.sh --from-source
medousa setup
```

## The commands

Everything runs through one binary:

```
medousa setup              Configure provider, model, backend, and channels
medousa start <service>    Start the engine, gateway, or a channel bridge
medousa tui                Open the workspace (starts the engine for you)
medousa daemon             Start the background engine
medousa discord            Connect Discord
medousa telegram           Connect Telegram
medousa slack              Connect Slack
medousa whatsapp           Connect WhatsApp
medousa doctor             Health check — everything that matters

medousa identity-export    Export who Medousa knows you to be
medousa identity-remember  Teach it a fact, from the terminal
medousa manuscript-list    See your installed specialties
medousa manuscript-validate <id>
medousa manuscript-install <path> [--project]
medousa skill-import <path> [--force]
medousa skill-import --from-hermes|--from-openclaw|--from-cursor
medousa openshell-probe [<manuscript-id>]   # H6/H7 sandbox validation
```

**Skill learning tools** (worker lane; host can discover + propose):

| Tool | Lane | Role |
|------|------|------|
| `cognition_skill_discover` | host + worker | Inventory scripts + risk before import |
| `cognition_skill_propose` | host + worker | Policy level gate (observe → sandbox) |
| `cognition_skill_probe` | worker | Run skill script in OpenShell sandbox |
| `cognition_openshell_sandbox_run` | worker | Ad-hoc sandbox command or `skill_script` |

Run `medousa <command> --help` for the rest.

### From your phone or chat

Same engine, same rules:

| Say this | Get this |
|---|---|
| `/new` | A fresh start |
| `/brief` | Your morning brief — add a note after if you want |
| `/skills` | List imported skill specialties with runnable scripts |
| `/skill <id> [script] [extra]` | Run a skill in OpenShell sandbox (same flow Medousa uses) |
| `/ask …` | A direct question |
| `/regen` | Try the last answer again |
| `/stop` | Cancel what's running |
| `/history` | Pick up an older conversation |
| `/model`, `/depth`, `/name` | Tune how it responds |
| `/health`, `/heartbeat` | Is everything alive? |

Or just type. No slash required.

## The workspace

The workspace is a terminal interface with everything you need in one place. Turn history, slash commands, artifact previews, and a settings panel. When work is running, you see it — jobs, schedules, script output — beside the conversation, not buried in a log file.

It is fast. It connects to the background engine automatically. If the engine is not running, the workspace starts it.

## What makes it reliable

You are not watching Medousa when it works. That is the point.

When you send a message or schedule a check-in, Medousa turns it into work that cannot be lost. If your laptop goes to sleep, if the network drops, if the daemon restarts — that work waits. It retries. It picks up where it left off.

You never have to wonder whether something finished. If Medousa accepted it, it ran.

## What makes it safe

When Medousa runs a script — processing a spreadsheet, fetching a page, transforming a file — it runs inside a sealed environment. That script cannot touch your documents, your passwords, or your other applications unless you explicitly say it can.

When it reaches outside — sending a message, calling an external service — it can ask you first.

You do not have to trust the script. You only have to trust the seal.

## What makes it remember

Most assistants amnesia every time you open them. Medousa doesn't.

It remembers **what happened** — the texture of your weeks, compressed and searchable.

It remembers **who you are** — how you take your coffee, who Mario is, what you care about this quarter. The essentials surface at the start of every turn. Ask for more when you need it.

You can export that picture as markdown. Edit it. Hand it back. Or teach Medousa one fact at a time from the terminal.

This is not a gimmick. It is the entire point.

## Specialties

Some jobs deserve their own voice.

A **manuscript** is a specialty pack — morning brief, research deep-dive, memory ritual — with its own tone, its own boundaries, its own schedule if you want one. Install one. Name it. Run it from the workspace, delegate it to the background, or schedule it to land in Telegram every morning.

Shipped with the repo: **morning-brief**. One command on Telegram. Your day, summarized.

### Bring your skills with you

You already did the work. Medousa doesn't make you start over.

If you're coming from **Hermes**, **OpenClaw**, or **Cursor**, you already have skills — folders with a `SKILL.md` file and optional scripts, references, and templates. Same format. Same muscle memory. Medousa speaks it natively.

One command imports a skill (or your whole library) as a Medousa specialty. Your `SKILL.md` stays the source of truth — scripts and reference files come along for the ride. Medousa wraps it with scheduling, delivery, memory, and tool policy when you want more than markdown alone.

```bash
# One skill folder or SKILL.md file
medousa skill-import ~/.hermes/skills/research/web-research

# Your whole Hermes library
medousa skill-import --from-hermes

# OpenClaw + .agents skills
medousa skill-import --from-openclaw

# Cursor personal skills + this project's .cursor/skills
medousa skill-import --from-cursor
```

Imported specialties land in `~/.config/medousa/manuscripts/` — a small YAML stub plus your skill folder. List them with `medousa manuscript-list`. Run one with `manuscript_id` on a turn or worker, same as anything built in-house.

Native YAML manuscripts and imported skills coexist. Write new specialties in YAML when you want schedules, delivery, and tool boundaries baked in. Keep authoring `SKILL.md` when that's how you think. Medousa handles both — one runtime, one catalog, zero re-explaining yourself.

Power users can extend a base specialty (`--extends base-researcher` is the default), attach longer prose, and tune tool allowlists after import. The machinery stays invisible. The experience doesn't.


## Where everything lives

Your data stays on your machine:

| What | Where |
|---|---|
| The database | `~/.local/share/medousa/runtime.surrealkv` |
| Your settings | `~/.local/share/medousa/product_config.json` |
| Workspace preferences | `~/.local/share/medousa/tui_defaults.json` |
| Conversation history | `~/.local/share/medousa/history/` |
| Keys and tokens | `~/.local/share/medousa/secrets/` |
| Tool & service bindings | `~/.config/medousa/capabilities.toml` |
| Connected app servers | `~/.config/medousa/mcp-gateway.toml` |
| Your specialties (YAML + imported skills) | `~/.config/medousa/manuscripts/` |
| Logs | `~/.local/share/medousa/logs/` |

**Settings** (`product_config.json`) — who can message you on each channel, heartbeat rhythms, how hard the engine works. Written by setup; yours to tune.

**Workspace prefs** (`tui_defaults.json`) — model, depth, how many tool rounds. The settings panel keeps this current.

**Capabilities** (`capabilities.toml`) — optional. Map what you mean ("check my calendar") to what runs, without touching code.

**Gateway** (`mcp-gateway.toml`) — optional. Which external app servers Medousa can talk to locally. Details in [docs/mcp-gateway-setup.md](docs/mcp-gateway-setup.md).

Override the database path with `--backend` or `MEDOUSA_SURREALKV_PATH`.

## Providers

Medousa supports 25+ LLM providers via [genai](https://github.com/jeremychone/rust-genai). Switch at setup or in the workspace:

- **OpenAI** (default) — `gpt-4o-mini`
- **Ollama** (local) — `llama3.2`, auto-detected on `127.0.0.1:11434`
- **Custom** — any provider genai supports

Or set `MEDOUSA_LLM_PROVIDER`, `MEDOUSA_LLM_MODEL`, and `MEDOUSA_LLM_BASE_URL` and skip the wizard.

## Environment

| Variable | Purpose |
|---|---|
| `MEDOUSA_LLM_PROVIDER` | Provider name |
| `MEDOUSA_LLM_MODEL` | Model |
| `MEDOUSA_LLM_BASE_URL` | API base URL |
| `MEDOUSA_SURREALKV_PATH` | Database file |
| `MEDOUSA_DAEMON_URL` | Engine URL for bridges and workspace |
| `MEDOUSA_MCP_GATEWAY_URL` | Local gateway (default `http://127.0.0.1:7420`) |

Provider-specific URLs: `MEDOUSA_<PROVIDER>_BASE_URL` or `STASIS_<PROVIDER>_BASE_URL`. Ollama honours `OLLAMA_HOST`.

---

### Chaos is not a personality trait. It is a failure of tools.

Every piece of software that forgets who you are, loses your work, or answers without proof is not your fault. It is a broken tool.

Medousa is built to be the opposite.

It remembers. It verifies. It finishes what it starts. It runs where you live — on your machine, in your chat, across your rooms. It does not guess. It does not forget. It does not leave you wondering whether something worked.

Chaotic life turns to stone when the tools around you stop adding to the noise.

That is what Medousa is for.
