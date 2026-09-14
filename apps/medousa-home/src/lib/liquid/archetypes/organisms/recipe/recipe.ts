import { defineArchetype } from "$lib/liquid/core";

export const recipe = defineArchetype({
  id: "recipe",
  tier: "organism",
  props: {
    title: { type: "string", required: true },
    subtitle: { type: "string" },
    yield: { type: "string" },
    ingredients: { type: "array" },
    resources: { type: "array" },
    steps: { type: "array", required: true },
    notes: { type: "string" },
    actions: { type: "array" },
  },
  acceptsBindings: ["inline"],
  writeCapable: true,
  slots: [],
  emits: ["edit", "submit"],
  virtualization: "none",
  defaultOwner: "agent",
});
