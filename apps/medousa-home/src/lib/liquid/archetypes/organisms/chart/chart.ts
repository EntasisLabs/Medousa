/** `chart` organism — paste-first plots from ```chart markdown fences. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Chart from "./Chart.svelte";

export const chart = defineArchetype({
  id: "chart",
  tier: "organism",
  props: {
    type: { type: "string", required: true },
    title: { type: "string" },
    description: { type: "string" },
    categories: { type: "array", required: true },
    series: { type: "array", required: true },
    layout: { type: "string" },
    stacked: { type: "boolean" },
    curve: { type: "string" },
    separator: { type: "boolean" },
    centerLabel: { type: "string" },
    centerValue: { type: "string" },
    trend: { type: "string" },
    trendDirection: { type: "string" },
    caption: { type: "string" },
    labels: { type: "string" },
    labelPosition: { type: "string" },
    tooltip: { type: "boolean" },
    legend: { type: "string" },
    interactive: { type: "boolean" },
    activeKey: { type: "string" },
    colors: { type: "array" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: ["select"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(chart.id, Chart);
