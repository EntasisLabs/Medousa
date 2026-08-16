/** `compare` organism — side-by-side judgment matrix (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";

export const compare = defineArchetype({
  id: "compare",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    recommendation: { type: "string" },
    mode: { type: "string" },
    axes: { type: "array", required: true },
    entities: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});
