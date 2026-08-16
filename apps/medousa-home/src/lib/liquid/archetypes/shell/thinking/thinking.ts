/** `thinking` shell archetype — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const thinking = defineArchetype({
  id: "thinking",
  tier: "molecule",
  props: {
    reasoning: { type: "string", required: true },
    streaming: { type: "boolean" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
