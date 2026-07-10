/** `stack` layout primitive — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Stack from "./Stack.svelte";

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

registerComponent(stack.id, Stack);
