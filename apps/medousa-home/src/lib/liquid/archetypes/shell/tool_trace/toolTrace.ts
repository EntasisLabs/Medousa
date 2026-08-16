/** `tool_trace` shell archetype — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const toolTrace = defineArchetype({
  id: "tool_trace",
  tier: "molecule",
  props: {
    runs: { type: "array", required: true },
    turnIndex: { type: "number" },
    streaming: { type: "boolean" },
  },
  acceptsBindings: ["inline", "work:lineage"],
  writeCapable: false,
  slots: [],
  emits: ["select", "run"],
  virtualization: "none",
  defaultOwner: "agent",
});

