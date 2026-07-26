# Browser and web research

**Web** is a real browser surface inside Home — tabs, address bar, bookmarks, and library saves — plus human↔agent handoff when Medousa needs the page.

Related: [Permissions, budgets, and tool safety](guide:permissions-budgets) · [Chat](guide:chat)

## Chrome

| Control | Notes |
|---------|--------|
| Tab bar | New tab ⌘T · close ⌘W · reopen closed ⇧⌘T |
| Back / Forward / Reload | ⌘[ · ⌘] · ⌘R (Stop while loading) |
| URL bar | Focus ⌘L · history suggestions on desktop |
| Find in page | ⌘F |
| Bookmark / Save | Star page; **Save to Library** |
| Bookmarks sheet | ⇧⌘B — History, Quick bookmarks, Library saves |
| Page actions | Copy link, open in default browser, bookmark, saved pages |

Pop Web into its own window from the desktop toolbar when research should sit beside Chat.

## Agent handoff

```timeline
title: Who has the wheel?
subtitle: Banner order
granularity: day

---
ts: 1
label: Medousa is exploring
detail: Agent controls the page — **Take control** when you need the wheel.
icon: sparkles
---
ts: 2
label: You are browsing
detail: You own the scoped session — **Hand back** when done.
icon: users
---
ts: 3
label: Verification needed
detail: CAPTCHA or similar — complete in Web, then **Continue agent**.
icon: shield
```

| Banner | Meaning | Action |
|--------|---------|--------|
| **Medousa is exploring** | Agent controls the page | **Take control** when you need the wheel |
| **You are browsing** | You own the scoped session | **Hand back** when done |
| **Verification needed** / CAPTCHA | Agent paused on a check | Complete in Web → **Continue agent** |

Same verification flow can appear from chat (“Medousa needs help with a verification”). Do not paste site passwords into the composer — use this surface.

## Research habits

1. Save durable sources to the **Library** (Save / Library saves).
2. Keep Chat open in a pane for cited briefs.
3. Prefer **Take control** for logins and payments; hand back only when the agent should resume.

```callout
tone: tip
title: External browser
body: Use Open in default browser when a site needs a full system browser or a profile Medousa does not have.
```

Next: [Chat](guide:chat) · [Vault and notes](guide:vault-notes).
