/** `accordion` molecule — collapsible sections (paste-first from ```accordion). */

import { defineArchetype } from "$lib/liquid/core";

export const accordion = defineArchetype({
  id: "accordion",
  tier: "molecule",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    multiple: { type: "boolean" },
    items: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});
