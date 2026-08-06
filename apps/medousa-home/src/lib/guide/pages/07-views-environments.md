# Views and environments

An **environment** describes how Home presents surfaces: which exist, layout presets, theme, and shell chrome. **Custom views** are surfaces you create and pin — canvases with widgets, not chat transcripts.

Related: [Navigation and surfaces](guide:navigation-surfaces) · [Liquid reference](guide:liquid-reference) · [Sharing and phone](guide:sharing-phone)

## Layout presets

Status bar layout switcher (when more than one preset exists). Built-ins include **Default** and **Focus**. Switching can also apply that preset’s theme and chrome. **Edit destinations** shows/hides and reorders rail surfaces (Settings and Runtime stay available).

## Create a custom view

**+ New view** (canvas / environment UI):

| Field | Notes |
|-------|--------|
| **Name** | Label in nav |
| **View id** | Stable id |
| **Nav icon** | Rail glyph |
| **Layout** | Dashboard / Single column / Split |
| **Nav position** | Where it appears |
| | **Create view** |

Views appear like built-in surfaces. Pop one out via view pop-out for a second monitor.

## Widgets and tiling

**Widget catalog** — target a view; tabs such as **Artifacts**, **Spotify / Apple**, **Vault notes**. **Add widget** opens the picker.

**Edit layout** toolbar: Cancel / Done · Split / Stack / Merge · Add widget · Remove. Desktop: drag by handle. Mobile: long-press to move (editor may not auto-open on phone).

Prefer one composition per desk — see product design restraint in polish plans; avoid dashboard sprawl.

## Feeds

Custom views can show **Live feed** / **Stale feed** badges when bound to recurring Liquid `feed` output (last-good). Empty feed: “No feed output yet.” Wire content via automations + Liquid, not a separate Automations “Feeds” tab.

## Backup and import

Settings → Sharing / LAN → **Canvas backup & send**:

- **Include views**
- **If names collide:** **Rename** / **Skip** / **Overwrite**
- **Export** / **Import**

Use Rename when merging workshops so you do not clobber a careful layout.

## View vs note

| Use a view when… | Use a note when… |
|------------------|------------------|
| You need persistent spatial layout | Prose and links are enough |
| Multiple widgets must stay visible | You're drafting or journaling |
| The surface is interactive chrome | The agent should edit markdown |

Next: [Vault and notes](guide:vault-notes) · [Liquid reference](guide:liquid-reference).
