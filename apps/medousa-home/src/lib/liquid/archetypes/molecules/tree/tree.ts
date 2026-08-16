/** `tree` molecule — file/folder tree from indented list (```tree fences). */

import { defineArchetype } from "$lib/liquid/core";

export const tree = defineArchetype({
  id: "tree",
  tier: "molecule",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    nodes: { type: "array", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

