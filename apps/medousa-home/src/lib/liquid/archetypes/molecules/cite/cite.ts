/** `cite` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const cite = defineArchetype({
  id: "cite",
  tier: "molecule",
  props: {
    title: { type: "string" },
    url: { type: "string" },
    quote: { type: "string" },
    source: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
