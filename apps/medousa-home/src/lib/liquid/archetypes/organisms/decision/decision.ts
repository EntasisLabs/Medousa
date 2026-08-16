/** `decision` organism — options → tradeoffs → recommendation (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";

export const decision = defineArchetype({
  id: "decision",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    factors: { type: "string" },
    recommendation: { type: "string" },
    options: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

