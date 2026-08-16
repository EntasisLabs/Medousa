/** `presentation` shell archetype — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const presentation = defineArchetype({
  id: "presentation",
  tier: "shell",
  props: { artifacts: { type: "array", required: true } },
  acceptsBindings: ["artifact:id", "inline"],
  writeCapable: false,
  slots: [],
  emits: ["navigate", "dismiss", "pin"],
  virtualization: "none",
  defaultOwner: "agent",
});
