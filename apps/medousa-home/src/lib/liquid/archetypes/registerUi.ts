/** Explicit, idempotent installation of lazy Liquid renderer factories. */
import type { ArchetypeId } from "$lib/liquid/core";
import {
  registerComponentLoader,
  type ArchetypeComponentModule,
} from "$lib/liquid/render/componentRegistry";

const factories: ReadonlyArray<[
  ArchetypeId,
  () => Promise<ArchetypeComponentModule>,
]> = [
  ["button", () => import("./atoms/button/Button.svelte")],
  ["chip", () => import("./atoms/chip/Chip.svelte")],
  ["media", () => import("./atoms/media/Media.svelte")],
  ["metadata", () => import("./atoms/metadata/Metadata.svelte")],
  ["prose", () => import("./atoms/prose/Prose.svelte")],
  ["status_pill", () => import("./atoms/status_pill/StatusPill.svelte")],
  ["whisper", () => import("./atoms/whisper/Whisper.svelte")],
  ["stack", () => import("./layout/stack/Stack.svelte")],
  ["accordion", () => import("./molecules/accordion/Accordion.svelte")],
  ["action_row", () => import("./molecules/action_row/ActionRow.svelte")],
  ["block", () => import("./molecules/block/Block.svelte")],
  ["callout", () => import("./molecules/callout/Callout.svelte")],
  ["card", () => import("./molecules/card/Card.svelte")],
  ["carousel", () => import("./molecules/carousel/Carousel.svelte")],
  ["chip_group", () => import("./molecules/chip_group/ChipGroup.svelte")],
  ["cite", () => import("./molecules/cite/Cite.svelte")],
  ["code", () => import("./molecules/code/Code.svelte")],
  ["observability", () => import("./molecules/observability/Observability.svelte")],
  ["section", () => import("./molecules/section/Section.svelte")],
  ["steps", () => import("./molecules/steps/Steps.svelte")],
  ["tabs", () => import("./molecules/tabs/Tabs.svelte")],
  ["tree", () => import("./molecules/tree/Tree.svelte")],
  ["brief", () => import("./organisms/brief/Brief.svelte")],
  ["chart", () => import("./organisms/chart/Chart.svelte")],
  ["compare", () => import("./organisms/compare/Compare.svelte")],
  ["dashboard", () => import("./organisms/dashboard/Dashboard.svelte")],
  ["decision", () => import("./organisms/decision/Decision.svelte")],
  ["document", () => import("./organisms/document/Document.svelte")],
  ["feed", () => import("./organisms/feed/Feed.svelte")],
  ["plan", () => import("./organisms/plan/Plan.svelte")],
  ["report", () => import("./organisms/report/Report.svelte")],
  ["shortlist", () => import("./organisms/shortlist/Shortlist.svelte")],
  ["slides", () => import("./organisms/slides/Slides.svelte")],
  ["timeline", () => import("./organisms/timeline/Timeline.svelte")],
  ["chat_media", () => import("./shell/chat_media/ChatMedia.svelte")],
  ["presentation", () => import("./shell/presentation/Presentation.svelte")],
  ["thinking", () => import("./shell/thinking/Thinking.svelte")],
  ["tool_trace", () => import("./shell/tool_trace/ToolTrace.svelte")],
];

let installed = false;
let styles: Promise<unknown> | undefined;

export function registerLiquidUiFactories(): void {
  if (!installed) {
    installed = true;
    for (const [id, factory] of factories) registerComponentLoader(id, factory);
  }
}

export function installLiquidUi(): Promise<unknown> {
  registerLiquidUiFactories();
  styles ??= import("$lib/liquid/styles/liquidOverflow.css");
  return styles;
}
