/** `slides` organism — labeled deck frames with nested figure grid bodies. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Slides from "./Slides.svelte";

export const slides = defineArchetype({
  id: "slides",
  tier: "organism",
  props: {
    title: { type: "string" },
    theme: { type: "string" },
    columns: { type: "string" },
    active: { type: "string" },
    showAll: { type: "boolean" },
    exportPaper: { type: "boolean" },
    slides: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(slides.id, Slides);
