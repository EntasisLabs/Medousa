/** `chip_group` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import ChipGroup from "./ChipGroup.svelte";

export const chipGroup = defineArchetype({
  id: "chip_group",
  tier: "molecule",
  props: {},
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["chips"],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(chipGroup.id, ChipGroup);
