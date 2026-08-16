/** `action_row` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const actionRow = defineArchetype({
  id: "action_row",
  tier: "molecule",
  props: {
    label: { type: "string", required: true },
    emoji: { type: "string" },
    icon: { type: "string" },
    chevron: { type: "boolean" },
    intent: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["submit"],
  virtualization: "none",
  defaultOwner: "agent",
});
