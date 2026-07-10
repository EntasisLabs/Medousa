/** `action_row` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import ActionRow from "./ActionRow.svelte";

export const actionRow = defineArchetype({
  id: "action_row",
  tier: "molecule",
  props: {
    label: { type: "string", required: true },
    emoji: { type: "string" },
    chevron: { type: "boolean" },
    intent: { type: "string" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["submit"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(actionRow.id, ActionRow);
