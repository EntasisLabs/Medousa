# Environment canvas (agent workflow)

Agents build persistent UI on **custom surfaces** in the environment spec. Builtin surfaces (`home`, `chat`, `settings`, `runtime`) do not render agent-owned components.

## Workflow

1. `cognition_environment_get` — read current spec, surfaces, and components.
2. `cognition_environment_propose` then `cognition_environment_apply` — add a `kind: custom` surface and include its id in the **active layout preset** `surfaces` array.
3. Publish content:
   - `cognition_ui_present` with `persist=true`, `surface_id`, `component_id`, `slot`, or
   - `cognition_component_create` with a `presentation` component.
4. `cognition_component_list` — verify placement.

## Component JSON (camelCase)

```json
{
  "id": "writing-manuscript",
  "type": "presentation",
  "surfaceId": "writing-studio",
  "slot": "main",
  "label": "Manuscript",
  "config": { "artifactId": "art-writing-demo" },
  "presentation": "inline"
}
```

Validation failures return `errors[]` from propose/create/update — fix surface preset membership and field names before retrying.

## Human docs

- [Custom views & canvas (normie)](../../cookbook/custom-views-and-canvas.md) — approval, presets, My views
- [Environment canvas (advanced)](../../cookbook/environment-canvas-advanced.md) — spec, feeds, HTTP, themes, icons

See also [agent-tools.md](./agent-tools.md).
