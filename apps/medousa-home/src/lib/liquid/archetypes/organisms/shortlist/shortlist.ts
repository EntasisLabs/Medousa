/** `shortlist` organism — ranked candidates / find-me options (sacred seven). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Shortlist from "./Shortlist.svelte";

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

registerComponent(shortlist.id, Shortlist);
