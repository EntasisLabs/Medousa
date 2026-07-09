/** `thinking` shell archetype — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Thinking from "./Thinking.svelte";

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

registerComponent(thinking.id, Thinking);
