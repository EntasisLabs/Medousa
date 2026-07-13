/** `report` organism — narrative + nested chart figures in a column grid. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Report from "./Report.svelte";

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

registerComponent(report.id, Report);
