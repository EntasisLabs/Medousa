# Themes, accessibility, and quiet chrome

Medousa should feel like *your* bench. Customize carefully — loud chrome fights flow.

Related: [Settings reference](guide:settings-reference) · [Keyboard and flow](guide:keyboard-flow) · [Platform matrix](guide:platform-matrix)

## Light / dark vs named theme

| Control | Where | Scope |
|---------|-------|--------|
| **Light / dark** | Preferences header toggle | This device (`document` class + localStorage) |
| **Named color theme** | Preferences → **Look** | **Active layout preset** — switching presets can change theme |

Pick one theme per desk. Thrashing themes mid-task costs more than it gives.

## Shell chrome (Look)

Room / shell options under Look can show or hide:

- Left rail
- Vault chat FAB
- Vault sidebar
- Mobile Home tab behavior
- Layout preset advanced controls

Layout presets also carry destination order — [Navigation](guide:navigation-surfaces) and [Views](guide:views-environments).

## Zoom

| Kind | How | Notes |
|------|-----|--------|
| **Content zoom** | Spotlight Zoom in/out/reset, or ⌘+/−/0 (desktop) | Scales whole UI like a browser (about 70–160%); no-op on web/phone |
| **Pane zoom** | ⌘; z / Spotlight **Zoom pane** | Maximizes one pane, not UI scale |

## Everyday and display prefs

Preferences → **Everyday** / **More display**:

- Work-done alerts, workshop guidance, open Web when the agent browses
- Mobile: **Remote push**, **Live Activity**
- Technical activity, engine details in chat, model picker visibility, **Liquid chat**

Work card retention (hide / wipe) is under Preferences → **Work cards** — [Data and recovery](guide:data-lifecycle).

## Accessibility

| Concern | Behavior |
|---------|----------|
| **Reduced motion** | Honors OS `prefers-reduced-motion` — wizard, chat, shell, Liquid. No in-app toggle. |
| **Keyboard** | Spotlight + pane prefix chords — [Commands reference](guide:commands-reference). No remapping UI. |
| **Focus** | Prefer keyboard for pane geometry; mouse for precise editing. |
| **Contrast / theme** | Choose a named theme that reads clearly; pair with light/dark. |
| **Drag alternatives** | Many drag actions have Spotlight or menu equivalents (cancel Work via drop zone still pointer-first — use card inspector when needed). |

## What not to customize first

| Resist | Why |
|--------|-----|
| Per-message model hopping | Breaks continuity; prefer favorites |
| Maximal panes on day one | Learn one desk, then split |
| Turning every badge on | Status noise trains you to ignore status |
| Shell tools on with empty allowlists | Security — [Permissions](guide:permissions-budgets) |

When something feels wrong visually, check zoom and theme before filing a bug.

Next: [Commands reference](guide:commands-reference) · [Settings reference](guide:settings-reference).
