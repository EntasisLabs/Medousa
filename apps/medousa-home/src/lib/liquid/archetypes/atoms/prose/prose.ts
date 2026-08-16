/** `prose` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const prose = defineArchetype({
  id: "prose",
  tier: "atom",
  props: {
    markdown: { type: "string", required: true },
    plain: { type: "boolean" },
  },
  acceptsBindings: ["inline", "vault:path", "vault:query"],
  writeCapable: false,
  slots: [],
  emits: ["navigate"],
  virtualization: "none",
  defaultOwner: "agent",
});

