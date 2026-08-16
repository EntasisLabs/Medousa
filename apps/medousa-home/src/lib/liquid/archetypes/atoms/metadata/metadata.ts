/** `metadata` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const metadata = defineArchetype({
  id: "metadata",
  tier: "atom",
  props: { parts: { type: "array", required: true } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
