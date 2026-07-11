/** `dashboard` organism — at-a-glance metric tile grid (sacred seven, paste-first). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Dashboard from "./Dashboard.svelte";

export const dashboard = defineArchetype({
  id: "dashboard",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    columns: { type: "string" },
    tiles: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(dashboard.id, Dashboard);
