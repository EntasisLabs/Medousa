/** `chip_group` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

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
