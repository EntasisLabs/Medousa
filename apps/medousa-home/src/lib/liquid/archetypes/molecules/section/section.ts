/** `section` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const section = defineArchetype({
  id: "section",
  tier: "molecule",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["content"],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
