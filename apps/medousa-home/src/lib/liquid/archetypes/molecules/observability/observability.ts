/** `observability` molecule — thinking + tool lineage behind one disclosure. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Observability from "./Observability.svelte";

export const observability = defineArchetype({
  id: "observability",
  tier: "molecule",
  props: {
    summary: { type: "string" },
    collapsed: { type: "boolean" },
    streaming: { type: "boolean" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["detail"],
  emits: [],
  virtualization: "none",
  defaultOwner: "app",
});

registerComponent(observability.id, Observability);
