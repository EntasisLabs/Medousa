/** `card` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Card from "./Card.svelte";

export const card = defineArchetype({
  id: "card",
  tier: "molecule",
  props: {
    title: { type: "string", required: true },
    subtitle: { type: "string" },
    body: { type: "string" },
    emoji: { type: "string" },
    image: { type: "string" },
    badges: { type: "array" },
    meta: { type: "string" },
    summary: { type: "string" },
    chips: { type: "array" },
    points: { type: "array" },
  },
  acceptsBindings: ["work:card", "vault:path", "inline"],
  writeCapable: false,
  slots: ["detail"],
  emits: ["select", "expand", "collapse", "pin"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(card.id, Card);
