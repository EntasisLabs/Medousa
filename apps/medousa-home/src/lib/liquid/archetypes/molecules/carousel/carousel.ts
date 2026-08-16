/** `carousel` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const carousel = defineArchetype({
  id: "carousel",
  tier: "molecule",
  props: {},
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["items"],
  emits: ["select", "scroll_end"],
  virtualization: "window",
  defaultOwner: "agent",
});
