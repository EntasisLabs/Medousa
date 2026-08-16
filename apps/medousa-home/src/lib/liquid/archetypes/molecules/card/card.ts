/** `card` molecule — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const card = defineArchetype({
  id: "card",
  tier: "molecule",
  props: {
    title: { type: "string", required: true },
    subtitle: { type: "string" },
    body: { type: "string" },
    emoji: { type: "string" },
    icon: { type: "string" },
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

