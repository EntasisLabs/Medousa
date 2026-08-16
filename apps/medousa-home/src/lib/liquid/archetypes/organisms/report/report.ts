/** `report` organism — narrative + nested chart figures in a column grid. */

import { defineArchetype } from "$lib/liquid/core";

export const report = defineArchetype({
  id: "report",
  tier: "organism",
  props: {
    title: { type: "string" },
    subtitle: { type: "string" },
    columns: { type: "string" },
    body: { type: "string", required: true },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
