/** `document` organism — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const document = defineArchetype({
  id: "document",
  tier: "organism",
  props: { scroll: { type: "string" } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["flow"],
  emits: ["navigate", "pin", "scroll_end"],
  virtualization: "none",
  defaultOwner: "agent",
});

