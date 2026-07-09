/** `button` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Button from "./Button.svelte";

export const button = defineArchetype({
  id: "button",
  tier: "atom",
  props: {
    label: { type: "string", required: true },
    action: { type: "string", required: true },
    tone: { type: "string" },
    payload: { type: "object" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["run"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(button.id, Button);
