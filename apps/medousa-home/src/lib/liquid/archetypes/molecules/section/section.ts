/** `section` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Section from "./Section.svelte";

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

registerComponent(section.id, Section);
