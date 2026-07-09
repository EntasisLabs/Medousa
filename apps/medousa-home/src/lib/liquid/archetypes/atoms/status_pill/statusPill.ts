/** `status_pill` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import StatusPill from "./StatusPill.svelte";

export const statusPill = defineArchetype({
  id: "status_pill",
  tier: "atom",
  props: {
    label: { type: "string", required: true },
    state: { type: "string" },
  },
  acceptsBindings: ["inline", "feed:id"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(statusPill.id, StatusPill);
