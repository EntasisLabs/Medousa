/** `block` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const block = defineArchetype({
  id: "block",
  tier: "molecule",
  props: {
    id: { type: "string" },
    font: { type: "string" },
    size: { type: "string" },
    align: { type: "string" },
    spacing: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["content"],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
