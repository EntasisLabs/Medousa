/** `presentation` shell archetype — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Presentation from "./Presentation.svelte";

export const presentation = defineArchetype({
  id: "presentation",
  tier: "shell",
  props: { artifacts: { type: "array", required: true } },
  acceptsBindings: ["artifact:id", "inline"],
  writeCapable: false,
  slots: [],
  emits: ["navigate", "dismiss", "pin"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(presentation.id, Presentation);
