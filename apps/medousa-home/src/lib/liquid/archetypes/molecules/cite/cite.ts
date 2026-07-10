/** `cite` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Cite from "./Cite.svelte";

export const cite = defineArchetype({
  id: "cite",
  tier: "molecule",
  props: {
    title: { type: "string" },
    url: { type: "string" },
    quote: { type: "string" },
    source: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(cite.id, Cite);
