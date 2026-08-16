/** `status_pill` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const statusPill = defineArchetype({
  id: "status_pill",
  tier: "atom",
  props: {
    label: { type: "string", required: true },
    state: { type: "string" },
  },
  acceptsBindings: ["inline", "feed:id"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
