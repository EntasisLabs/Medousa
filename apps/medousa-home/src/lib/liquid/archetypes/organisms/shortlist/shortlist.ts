/** `shortlist` organism — ranked candidates / find-me options (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";

export const shortlist = defineArchetype({
  id: "shortlist",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    criteria: { type: "string" },
    density: { type: "string" },
    items: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

