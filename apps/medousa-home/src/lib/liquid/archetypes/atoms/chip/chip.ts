/** `chip` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const chip = defineArchetype({
  id: "chip",
  tier: "atom",
  props: {
    label: { type: "string", required: true },
    tone: { type: "string" },
    value: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});
