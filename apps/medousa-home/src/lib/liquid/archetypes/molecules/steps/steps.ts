/** `steps` molecule — numbered vertical steps (paste-first from ```steps). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Steps from "./Steps.svelte";

export const steps = defineArchetype({
  id: "steps",
  tier: "molecule",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    steps: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(steps.id, Steps);
