/** `dashboard` organism — at-a-glance metric tile grid (sacred seven, paste-first). */

import { defineArchetype } from "$lib/liquid/core";

export const dashboard = defineArchetype({
  id: "dashboard",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    columns: { type: "string" },
    tiles: { type: "array", required: true },
  },
  acceptsBindings: ["inline", "feed:id"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

