/** `plan` organism — phased itinerary / trip flow (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";

export const plan = defineArchetype({
  id: "plan",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    grouping: { type: "string" },
    segments: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

