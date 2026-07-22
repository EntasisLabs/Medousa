/** `block` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Block from "./Block.svelte";

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

registerComponent(block.id, Block);
