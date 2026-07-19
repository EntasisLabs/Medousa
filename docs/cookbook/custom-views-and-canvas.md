# Custom views & canvas

Medousa can build **persistent pages** for you — dashboards, braindumps, writing studios, live polls — and pin them in the app sidebar. Think of them as your own mini spaces inside Medousa: you describe what you want, Medousa builds it, you approve the layout once, and it stays there.

This guide is for **app users** who talk to Medousa. You do not need JSON, tools, or a terminal.

---

## Custom view vs Library presentation

| | **Custom view (canvas)** | **Library / chat artifact** |
|---|--------------------------|-----------------------------|
| Where it lives | Sidebar (desktop) or **More → My views** (mobile) | Chat thread or Library tab |
| Persists? | Yes — pinned to your layout | Browse history; not a nav destination |
| Interactive state | Widgets can save data across refresh (engine-backed store) | Usually one-off or session-scoped |
| Who builds it | Medousa, from your description | Medousa, often inline in chat |

If you want something you **come back to every day**, ask for a **custom view** or **canvas page**, not just “show me HTML in chat.”

---

## How to get a custom view

Talk to Medousa in plain language. Examples:

- *“Build me a braindump view — quick thoughts I can save and see later.”*
- *“Make a writing studio with my manuscript and a notes panel.”*
- *“Create a live dashboard that checks train times every five minutes.”*
- *“Give my braindump a midnight theme with a purple accent.”*

Medousa will:

1. Add a **custom surface** (your page)
2. Put your widget(s) on it
3. Add it to your **layout preset** so it appears in nav
4. Sometimes ask you to **approve** the layout change

---

## Approving layout changes

When Medousa changes your overall layout (new page in nav, preset changes), you may see:

**Banner:** “Medousa proposed a layout change — Review in Settings → Canvas”

1. Open **Settings** (gear in the sidebar)
2. Go to **Canvas**
3. Read the summary on the pending card
4. **Apply layout** to go live, or **Dismiss** to keep the current layout

If the card lists **errors**, Medousa needs to fix the proposal before you can apply. Ask her to retry.

---

## Finding your views

### Desktop

Custom views appear as **icons in the left rail** (with Home, Chat, Work, etc.). A small dot on the icon can mean:

- **Green** — a subscribed feed recently updated (live dashboard)
- **Orange** — feed exists but nothing recent

### Mobile

Open **More** → **My views**. Custom surfaces from your active preset are listed there.

---

## Full vs Focus presets

In **Settings → Canvas** or the preset dropdown in the sidebar:

| Preset | What it does |
|--------|----------------|
| **Full** | All main areas: Home, Chat, Work, Library, Web, Workshop, etc., plus your custom views |
| **Focus** | Hides noisy areas (Web, Workshop, …). Chat, Work, Library, Settings stay. **Custom views still show if they are in the preset.** |

**Common confusion:** Medousa created your view, but you do not see it in the sidebar. Often the view exists but is **not in the active preset’s surface list**. Ask: *“Add my braindump surface to the active preset so it shows in nav.”*

---

## If a widget looks broken

1. **Settings → Canvas** — check custom surface status (components, feeds, last error)
2. Ask Medousa: *“Run the custom view doctor on [surface name] and fix any issues.”*

She can diagnose nav visibility, storage, and HTML problems without you opening browser DevTools.

**Widget does not save data?** Make sure you are on the **canvas surface** (sidebar), not only viewing the same HTML in chat. Persistent storage is tied to the pinned component on a custom surface.

---

## Your space (shell chrome)

**Settings → Canvas → Your space** controls desktop shell chrome — the left sidebar, vault chat button, activity rail, and related prefs. These live on your **environment profile** and survive reload.

The left chrome is one **master rail**: visible shows destination names (or the active view’s list); hidden is fully gone until you reopen it from the content header. Drag the rail’s right edge to resize. On Peers, Settings, Chat, Messaging, or Workspace, that same rail morphs to the view’s list — **Back** returns to destination nav; the collapse control hides the rail entirely.

This is not the same as **Edit layout** on a custom view (widget tiling). Your space hides or shows app chrome; Edit layout rearranges widgets inside a room you built.

First-run mode seeds a calm default: **Workspace** starts with the vault chat button off and the activity rail collapsed; **Workspace + AI** keeps chat and the activity rail visible. You can change any of these later in Your space.

---

## Themes and icons

Medousa can set a **canvas theme** (color palette, accent, tagline) for your environment. Widgets can use host CSS variables such as `var(--medousa-host-accent)` so they match your chosen look.

To retheme: *“Set my canvas theme to Tokyo Night with brand color #7aa2f7”* or *“Make my views match a calm goth palette.”*

Custom views can have **icons** in the sidebar (pen, sparkles, train, etc.). Ask Medousa to change them: *“Use the pen-line icon for my writing studio.”*

---

## FAQ

**Can I delete a custom view myself?**  
Not from a dedicated delete button today. Ask Medousa to remove the surface or hide it from the active preset.

**Will Focus hide my dashboard?**  
Only if that surface is removed from the Focus preset. By default, custom views in the preset still appear.

**Why is agent UI not on Home or Chat?**  
Builtin surfaces (Home, Chat, Settings, Runtime) are app chrome. Agent-built UI only renders on **custom** surfaces.

**Can I edit the HTML myself?**  
Not in the UI. Describe changes to Medousa; she revises the artifact.

---

## Related

- [Environment canvas (advanced)](environment-canvas-advanced.md) — operators & integrators
- [Artifacts & presentations](artifacts-and-presentations.md) — chat/Library HTML
- [Engine environment canvas](../engine/environment-canvas.md) — agent workflow summary
