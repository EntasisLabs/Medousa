# Workshop & Automations

**Audience:** Medousa app users who want scripts, flows, and scheduled work —
not just chat.

Medousa can run durable jobs, specialist packs, and scheduled check-ins. On
desktop you’ll see **Workshop** and **Automations** in the nav (wording may
evolve as Scripts Workbench lands).

---

## Mental model

| Idea | Meaning |
|------|---------|
| **Workshop** | Browse modules, scripts, connections, and specialists for the active engine |
| **Automations** | Flows, schedules, run history — work that can finish without you watching |
| **Specialty** | A pack with its own voice and boundaries (morning brief, research, …) |
| **Background / Work** | Heavy jobs you send off chat; results return on the work board |

Chat is for conversation. Automations are for **repeatable** or **long-running**
work that should survive sleep and reconnects.

---

## Common things to do

### Run or schedule a flow

1. Open **Automations** (or Workshop → Flows, depending on your build).
2. Pick a flow or ask Medousa in chat to *create a flow that…*
3. Run it now, or attach a **schedule** so it lands while you’re away.
4. Delivery can go to chat, a channel, or vault — pick what you configured.

### Import or edit a specialty

1. Open Workshop / Automations → specialists or scripts.
2. Import a `SKILL.md` (Cursor / Hermes / OpenClaw style) or start from a
   template.
3. Preview allowlists before you trust a pack with tools.

Cookbook depth: [Skills & specialties](../cookbook/skills-and-specialties.md).

### Send heavy work to the background

In chat, ask Medousa to run something in the background or use the Work surface.
Accepted jobs retry and resume; you don’t babysit the window.

---

## History and honesty

Automations keep **run history** so you can see what fired, what failed, and
what was delivered. If something looks stuck, open history before restarting the
engine.

---

## Related

- [Getting started](getting-started.md)
- [Channels](channels.md) — deliver briefs to Telegram / Discord / …
- [Custom views & canvas](../cookbook/custom-views-and-canvas.md)
- Contributor plans: [workshop-and-automations-plan.md](../../architecture/workshop-and-automations-plan.md)
  (architecture — not required for using the app)
