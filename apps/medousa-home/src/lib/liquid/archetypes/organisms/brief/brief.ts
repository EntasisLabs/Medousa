/** `brief` organism — structured answer with sources (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Brief from "./Brief.svelte";

export const brief = defineArchetype({
  id: "brief",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    tone: { type: "string" },
    sections: { type: "array", required: true },
    sources: { type: "array" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select", "navigate"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(brief.id, Brief);
