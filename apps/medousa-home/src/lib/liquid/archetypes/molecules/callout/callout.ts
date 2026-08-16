/** `callout` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const callout = defineArchetype({
  id: "callout",
  tier: "molecule",
  props: {
    tone: { type: "string" },
    title: { type: "string" },
    body: { type: "string", required: true },
    /** Optional collapsed technical detail (e.g. model/provider error dump). */
    detail: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
