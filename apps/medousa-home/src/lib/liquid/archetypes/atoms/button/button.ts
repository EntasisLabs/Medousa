/** `button` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const button = defineArchetype({
  id: "button",
  tier: "atom",
  props: {
    label: { type: "string", required: true },
    action: { type: "string", required: true },
    tone: { type: "string" },
    payload: { type: "object" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["run"],
  virtualization: "none",
  defaultOwner: "agent",
});

