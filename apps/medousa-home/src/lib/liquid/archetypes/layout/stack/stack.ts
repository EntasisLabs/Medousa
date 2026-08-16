/** `stack` layout primitive — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const stack = defineArchetype({
  id: "stack",
  tier: "layout",
  props: {
    direction: { type: "string" },
    gap: { type: "string" },
    align: { type: "string" },
  },
  acceptsBindings: [],
  writeCapable: false,
  slots: ["children"],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

