# Specialist agents

**Agents** (Automations → Agents) are importable specialists — `SKILL.md` packs you can run, schedule, and drop into flows. They are not the same as your user **profile** ([Profiles and Locus](guide:profiles-locus)).

## Open Agents

- Automations explorer → **Agents**
- Mobile More → **Agents**
- Spotlight: Agents

Copy: “Specialist agents — import, tune tools, schedule.”

## Import

**Bring your skills** wizard (desktop for folder pick):

1. **Choose folder** containing `SKILL.md`, or pick a library: **Hermes**, **OpenClaw**, **Cursor** (skills under `~/.hermes/skills`, `~/.openclaw/skills`, `~/.cursor/skills`).
2. **Import options** — scope **User** / **Project**; optional **Replace existing specialists with the same id**.
3. **Imported** confirmation.

Filters: **All / Runnable / Sandbox / Imported**. Badges may show **sandbox** or **scripts**.

## Tune and run

| Action | Meaning |
|--------|---------|
| **Run** | Sends `/skill {id}` into chat |
| **Tools** popover | Toggle tools this agent can use; **OpenShell sandbox**; allow OpenShell on scheduled runs |
| Editor | Specialist name, role, display name, tone, when invoked, **Open YAML** |
| **Schedule…** | Cron / execution / delivery readiness |
| **Use in automation** | Attach into Flows |

Treat the Tools palette as the trust surface — there is no separate “trust policy” label. Prefer sandbox when the skill ships scripts.

```callout
tone: tip
title: Profile vs specialist
body: You / Profiles is who you are. Agents are optional skills with their own tools. Do not import a skill to “fix” identity memory — teach from You instead.
```

Next: [Grapheme and automations](guide:grapheme-automations) · [Permissions, budgets, and tool safety](guide:permissions-budgets).
