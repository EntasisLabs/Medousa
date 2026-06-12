# Skills & specialties

Some jobs deserve their own voice — morning brief, research deep-dive, memory ritual.

A **manuscript** (specialty) is a pack with tone, boundaries, optional schedule, and tool policy.

---

## Shipped example

**morning-brief** — one command on Telegram; your day, summarized.

List installed:

```bash
medousa manuscript-list
```

---

## Import from Hermes, OpenClaw, or Cursor

Same `SKILL.md` format you already use:

```bash
# One skill folder
medousa skill-import ~/.hermes/skills/research/web-research

# Whole libraries
medousa skill-import --from-hermes
medousa skill-import --from-openclaw
medousa skill-import --from-cursor
```

Imported skills land in `~/.config/medousa/manuscripts/` (YAML stub + your folder).

Validate / install YAML manuscripts:

```bash
medousa manuscript-validate <id>
medousa manuscript-install <path-to.yaml> [--project]
```

---

## OpenShell sandbox

Skills with scripts run sealed — see [openshell-handoff-setup.md](../openshell-handoff-setup.md).

Probe before production:

```bash
medousa openshell-probe [<manuscript-id>]
```

---

## Extend & compose

- `--extends base-researcher` on import
- Native YAML manuscripts for schedules + delivery baked in
- Imported `SKILL.md` when that's how you think

One runtime, one catalog.
