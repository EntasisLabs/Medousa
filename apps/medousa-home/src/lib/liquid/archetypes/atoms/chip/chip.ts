/** `chip` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Chip from "./Chip.svelte";

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

registerComponent(chip.id, Chip);
