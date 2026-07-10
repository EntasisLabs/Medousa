/** `tool_trace` shell archetype — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import ToolTrace from "./ToolTrace.svelte";

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

registerComponent(toolTrace.id, ToolTrace);
