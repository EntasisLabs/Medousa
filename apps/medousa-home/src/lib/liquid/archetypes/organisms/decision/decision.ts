/** `decision` organism — options → tradeoffs → recommendation (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Decision from "./Decision.svelte";

export const decision = defineArchetype({
  id: "decision",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    factors: { type: "string" },
    recommendation: { type: "string" },
    options: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(decision.id, Decision);
