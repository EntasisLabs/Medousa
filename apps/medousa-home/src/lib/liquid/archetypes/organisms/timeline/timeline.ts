/** `timeline` organism — chronological event rail (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";

export const timeline = defineArchetype({
  id: "timeline",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    granularity: { type: "string" },
    layout: { type: "string" },
    events: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});
