/** `accordion` molecule — collapsible sections (paste-first from ```accordion). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Accordion from "./Accordion.svelte";

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

registerComponent(accordion.id, Accordion);
