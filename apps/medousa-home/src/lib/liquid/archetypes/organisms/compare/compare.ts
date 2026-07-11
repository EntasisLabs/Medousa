/** `compare` organism — side-by-side judgment matrix (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Compare from "./Compare.svelte";

export const compare = defineArchetype({
  id: "compare",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    recommendation: { type: "string" },
    axes: { type: "array", required: true },
    entities: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(compare.id, Compare);
