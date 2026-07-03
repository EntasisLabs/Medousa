# Environment canvas (advanced)

Operators, contributors, and HTTP integrators: how Medousa’s **environment spec** powers custom views, presets, widgets, feeds, and themes.

Normie guide: [Custom views & canvas](custom-views-and-canvas.md)

---

## Mental model

```text
EnvironmentSpec
├── surfaces[]        — nav destinations (builtin + custom)
├── layoutPresets[]   — which surfaces appear in nav (one active)
├── components[]      — persistent UI on surfaces (presentation, chrome_action, …)
├── shellChrome       — mobile home tab, ask FAB, tab bar density
└── theme             — canvas palette + brand (environment-first; widgets inherit)
```

**Frame / Chrome / Content:**

- **Frame** — `surfaces`, `layoutPresets`, `shellChrome`
- **Chrome** — builtin panels, `chrome_action` components (open ask, activity)
- **Content** — agent `presentation` components on **`kind: custom`** surfaces only

Builtin surfaces (`home`, `chat`, `settings`, `runtime`, …) do **not** render agent-owned presentation components.

---

## Surfaces

| Field | Notes |
|-------|--------|
| `id` | kebab-case slug |
| `label` | Nav label |
| `icon` | Lucide kebab name from [allowed icon catalog](#nav-icons) |
| `kind` | `builtin` \| `custom` |
| `layout` | `single` \| `split` \| `dashboard` |
| `layoutRoot` | Optional vstack/hstack/grid tree |

Custom surfaces are created via `cognition_environment_patch` (`add_custom_surface`) or `cognition_custom_view_compose`.

---

## Layout presets

Shipped presets:

| ID | Label | Behavior |
|----|-------|----------|
| `default` | Full | Full nav including web, workshop, custom views |
| `focus` | Focus | Hides noisy surfaces; custom views remain if listed |

A custom surface **exists** in `spec.surfaces` but is **hidden from nav** until its `id` is in the **active** preset’s `surfaces` array.

Activate: `cognition_environment_activate_preset` or Settings / nav preset dropdown.

---

## Components

```json
{
  "id": "braindump-capture",
  "type": "presentation",
  "surfaceId": "braindump",
  "slot": "main",
  "label": "Quick Thought",
  "config": { "artifactId": "art:…:ui:…" },
  "presentation": "inline",
  "feeds": ["braindump.pulse"]
}
```

| Type | Use |
|------|-----|
| `presentation` | HTML artifact in `PresentationFrame` |
| `chrome_action` | Header/FAB actions (`open_ask`, `open_activity`) |
| `builtin_panel` | Host panel id on builtin surfaces |

Slots: `main`, `header`, `sidebar`, `fab`, `inline`.

---

## Publishing paths

| Goal | Tool |
|------|------|
| One-shot surface + HTML + feeds + recurring | `cognition_custom_view_compose` |
| First HTML publish | `cognition_ui_present` (`persist=true` + `surface_id` + `component_id` + `slot`) |
| Revise HTML | `cognition_artifact_write` (`artifact_id`) |
| Incremental spec edits | `cognition_environment_patch` (agent tool only — SDK uses `environment().put_spec`) |
| Layout tree only | `cognition_layout_apply` (immediate, no approval) |
| Full spec replace | `cognition_environment_propose` → operator **Apply** in Settings → Canvas |

**Hybrid approval:** `rewrite_active_preset_surfaces` requires operator approval. Most other patch ops apply immediately.

### Patch ops (summary)

| Op | Effect |
|----|--------|
| `add_custom_surface` | New custom surface + optional `add_to_active_preset` |
| `add_to_active_preset` | Show surface in nav |
| `add_component` | Upsert presentation/chrome component |
| `set_component_feeds` | Bind feed ids |
| `update_surface` | Change `label` and/or `icon` on existing surface |
| `set_environment_theme` | Set `spec.theme` (`colorThemeId`, `brandColor`, `tagline`) |
| `remove_custom_surface` | Remove a custom surface from spec (does not delete artifact files) |
| `remove_component` | Remove a presentation component from spec |
| `rewrite_active_preset_surfaces` | Replace active preset surface list (gated) |

Patch ops run via `cognition_environment_patch` during agent turns. External integrators replace the full spec with `PUT /v1/environment/spec` (`environment().put_spec` in the SDK).

---

## Layout grammar

`layoutRoot` nodes:

- `vstack` / `hstack` — `spacing`, `align`, `distribution`, `children`
- `grid` — `columns`, `spacing`, `children`
- `component` — `{ type: "component", id, flex? }`

Validate with `cognition_environment_wiki(topic=layout_schema)`.

---

## Feeds & recurring

1. `cognition_feed_subscribe` — bind `feed_ids` on component
2. `cognition_runtime_recurring_register` — cron + poll → feed events
3. Artifact reads `window.__MEDOUSA_FEED__.feeds['feed.id'].lastPatch`

Personal-app recipe: wiki topics `feed_client`, `example_trip_poll`.

---

## MedousaStore & artifact runtime

- **Never** `localStorage` in sandboxed presentation HTML
- `MedousaStore.get/set/delete` return **Promises** — always `await`
- Guard reads: `Array.isArray(raw) ? raw : []`
- Doctor: `cognition_custom_view_doctor(surface_id, include_static_lint=true)`

Host CSS tokens (environment-first theme):

```css
color: var(--medousa-host-fg);
background: var(--medousa-host-surface);
border-color: var(--medousa-host-accent);
accent: var(--medousa-host-brand);
```

Wiki: `cognition_environment_wiki(topic=artifact_runtime)` and `environment_theme`.

---

## Environment theme

`spec.theme` (primary):

```json
{
  "colorThemeId": "tokyo-night",
  "brandColor": "#7aa2f7",
  "tagline": "Night desk"
}
```

Falls back to active workshop Room theme + workshop brand when fields are omitted.

Set via `set_environment_theme` patch op or ask Medousa to retheme your views.

Allowed `colorThemeId` values match Settings → Room palettes (`medousa`, `tokyo-night`, `dracula`, …).

---

## Nav icons

Icons are **closed allowlist** Lucide kebab names (not arbitrary uploads).

Examples: `pen-line`, `sparkles`, `brain`, `train-front`, `layout-grid`, `heart`, `coffee`

Set at `add_custom_surface` or `update_surface`. Invalid icons fail validation at propose/apply.

---

## Diagnostics

```text
cognition_custom_view_doctor(
  surface_id,
  include_runtime=true,
  include_static_lint=true,
  probe=true   // optional; needs Home on that surface
)
```

Settings → Canvas mirrors `GET /v1/environment/status?include_runtime=true`.

Common issue codes: `STATIC_LOCALSTORAGE`, `STATIC_STORE_SYNC_USAGE`, `STATIC_SLICE_WITHOUT_GUARD`, `STORE_WRONG_TYPE`, `RUNTIME_LOG`.

---

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/environment/spec` | Current spec + revision |
| PUT | `/v1/environment/spec` | Replace spec (validate first) |
| POST | `/v1/environment/spec/propose` | Stage pending proposal |
| GET | `/v1/environment/spec/pending` | Pending proposal |
| POST | `/v1/environment/spec/pending/apply` | Operator apply |
| DELETE | `/v1/environment/spec/pending` | Dismiss |
| GET | `/v1/environment/status` | Doctor-shaped status |
| GET | `/v1/environment/spec/stream` | SSE spec + feed patches + probe requests |
| GET/PUT | `/v1/components/{id}/store/*` | MedousaStore HTTP |
| POST/GET | `/v1/components/{id}/runtime/events` | Widget console bridge |

---

## Worked example: writing studio

1. `cognition_environment_wiki(topic=example_writing_studio)`
2. `cognition_custom_view_compose` with `surface_id`, `component_id`, `html`, optional `layout_root`
3. Operator approves if preset rewrite pending
4. Open surface from nav; revise HTML with `cognition_artifact_write`
5. `cognition_custom_view_doctor` until `issues` empty

---

## Tool routing

| Goal | Tool |
|------|------|
| Read layout | `cognition_environment_get` |
| Policy / schema | `cognition_environment_wiki` |
| Incremental edit | `cognition_environment_patch` |
| Diagnose | `cognition_custom_view_doctor` |
| HTML revise | `cognition_artifact_write` |

---

## Related

- [Engine environment canvas](../engine/environment-canvas.md) — short agent checklist
- [Agent tools](../engine/agent-tools.md) — cognition tool index
- [Medousa Home app](../apps/medousa-home.md) — UI stores & Settings Canvas
