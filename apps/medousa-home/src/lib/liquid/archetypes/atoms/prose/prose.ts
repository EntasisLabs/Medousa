/** `prose` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Prose from "./Prose.svelte";

export const prose = defineArchetype({
  id: "prose",
  tier: "atom",
  props: {
    markdown: { type: "string", required: true },
    plain: { type: "boolean" },
  },
  acceptsBindings: ["inline", "vault:path", "vault:query"],
  writeCapable: false,
  slots: [],
  emits: ["navigate"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(prose.id, Prose);
