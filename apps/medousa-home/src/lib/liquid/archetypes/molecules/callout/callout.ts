/** `callout` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Callout from "./Callout.svelte";

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

registerComponent(callout.id, Callout);
