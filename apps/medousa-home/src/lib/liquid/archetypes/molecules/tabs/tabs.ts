/** `tabs` molecule — multi-panel switcher (paste-first from ```tabs). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Tabs from "./Tabs.svelte";

export const tabs = defineArchetype({
  id: "tabs",
  tier: "molecule",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    default: { type: "string" },
    panels: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(tabs.id, Tabs);
