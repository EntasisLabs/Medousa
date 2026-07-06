# Edit canvas layout (operators)

Rearrange widgets on any **custom view** without asking Medousa — layout changes save to your environment spec like HTML edits.

Normie guide: [Custom views & canvas](custom-views-and-canvas.md) · Advanced spec: [Environment canvas (advanced)](environment-canvas-advanced.md)

---

## Where to edit

1. Open a **custom view** from the nav (not Home, Chat, or Settings).
2. Tap **Edit layout** at the top of the surface, or open a widget’s **⋯** menu → **Move in layout**.

Custom surfaces only — built-in surfaces cannot enter layout edit mode.

---

## Desktop

While editing:

| Action | How |
|--------|-----|
| Reorder | Drag a widget onto another widget |
| Split side-by-side | Select a widget → **Split ↔** |
| Split stacked | Select a widget → **Split ↕** |
| Add empty zone | **Add zone** — dashed drop target for a future widget |
| Reset | **Reset** — back to automatic vertical stack |
| Save | **Done** — writes layout to your spec |
| Discard | **Cancel** — revert unsaved changes |

Empty zones are **hidden** in normal view. They only appear as dashed targets while editing.

---

## Mobile (tap gestures)

Mobile does not use drag-and-drop. A hint bar shows:

**Tap select · Long-press pick up · Double-tap drop**

| Gesture | Action |
|---------|--------|
| Single tap | Select widget or empty zone |
| Long press | Pick up selected widget (lift shadow) |
| Double tap | Drop onto highlighted zone or widget |
| Single tap canvas (while moving) | Cancel move |

Use **Add zone**, **Split ↔**, **Split ↕**, **Reset**, **Done**, and **Cancel** in the edit toolbar — same as desktop.

---

## Persistence

**Done** calls the same spec save path as editing widget HTML (`PUT /v1/environment/spec`). Reload the app to confirm layout sticks.

Medousa can still change layout via agent tools; your manual edits and agent edits share one `layoutRoot` tree.

---

## Settings

Settings → **Canvas** shows preset and surface status. Edit layout on any custom view from the surface toolbar or widget menu — no separate settings screen required.
