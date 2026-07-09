/** `metadata` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Metadata from "./Metadata.svelte";

export const metadata = defineArchetype({
  id: "metadata",
  tier: "atom",
  props: { parts: { type: "array", required: true } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(metadata.id, Metadata);
