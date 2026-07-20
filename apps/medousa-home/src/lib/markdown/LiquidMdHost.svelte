<script lang="ts">
  /**
   * Host for markdown-hydrated Liquid embeds.
   * Mounted into placeholder divs by hydrateLiquidEmbeds.
   */
  import "$lib/liquid/archetypes";
  import { untrack } from "svelte";
  import { createNode } from "$lib/liquid/core";
  import {
    setLiquidContext,
    type LiquidRenderContext,
  } from "$lib/liquid/render/context";
  import Card from "$lib/liquid/archetypes/molecules/card/Card.svelte";
  import Carousel from "$lib/liquid/archetypes/molecules/carousel/Carousel.svelte";
  import ActionRow from "$lib/liquid/archetypes/molecules/action_row/ActionRow.svelte";
  import Callout from "$lib/liquid/archetypes/molecules/callout/Callout.svelte";
  import Cite from "$lib/liquid/archetypes/molecules/cite/Cite.svelte";
  import Compare from "$lib/liquid/archetypes/organisms/compare/Compare.svelte";
  import Plan from "$lib/liquid/archetypes/organisms/plan/Plan.svelte";
  import Timeline from "$lib/liquid/archetypes/organisms/timeline/Timeline.svelte";
  import Shortlist from "$lib/liquid/archetypes/organisms/shortlist/Shortlist.svelte";
  import Decision from "$lib/liquid/archetypes/organisms/decision/Decision.svelte";
  import Brief from "$lib/liquid/archetypes/organisms/brief/Brief.svelte";
  import Dashboard from "$lib/liquid/archetypes/organisms/dashboard/Dashboard.svelte";
  import Chart from "$lib/liquid/archetypes/organisms/chart/Chart.svelte";
  import Report from "$lib/liquid/archetypes/organisms/report/Report.svelte";
  import Slides from "$lib/liquid/archetypes/organisms/slides/Slides.svelte";
  import Section from "$lib/liquid/archetypes/molecules/section/Section.svelte";
  import ChipGroup from "$lib/liquid/archetypes/molecules/chip_group/ChipGroup.svelte";
  import Media from "$lib/liquid/archetypes/atoms/media/Media.svelte";
  import Tabs from "$lib/liquid/archetypes/molecules/tabs/Tabs.svelte";
  import Steps from "$lib/liquid/archetypes/molecules/steps/Steps.svelte";
  import Accordion from "$lib/liquid/archetypes/molecules/accordion/Accordion.svelte";
  import CodeEmbed from "$lib/liquid/archetypes/molecules/code/Code.svelte";
  import Tree from "$lib/liquid/archetypes/molecules/tree/Tree.svelte";
  import type {
    LiquidActionProps,
    LiquidAccordionProps,
    LiquidBriefProps,
    LiquidCalloutProps,
    LiquidCardProps,
    LiquidChipProps,
    LiquidCiteProps,
    LiquidCodeProps,
    LiquidCompareProps,
    LiquidDashboardProps,
    LiquidChartProps,
    LiquidReportProps,
    LiquidSlidesProps,
    LiquidDecisionProps,
    LiquidEmbedKind,
    LiquidMediaProps,
    LiquidPlanProps,
    LiquidSectionProps,
    LiquidShortlistProps,
    LiquidStepsProps,
    LiquidTabsProps,
    LiquidTimelineProps,
    LiquidTreeProps,
  } from "$lib/markdown/liquidEmbeds";
  import {
    AlertTriangle,
    Book,
    Brain,
    Check,
    Clock,
    Code,
    Coins,
    Compass,
    Cpu,
    FileCode,
    Globe,
    Hourglass,
    Info,
    Layers,
    Lock,
    Map,
    MessageCircle,
    Mic,
    Pencil,
    Rocket,
    Search,
    Shield,
    Sparkles,
    Star,
    Table,
    Tag,
    X,
    Zap,
    type Icon as LucideIcon,
  } from "@lucide/svelte";

  interface Props {
    kind: LiquidEmbedKind | "icon";
    payload: unknown;
    context?: LiquidRenderContext;
    /** Enter animation (default true). Skip on remounts with unchanged placeholders. */
    animate?: boolean;
  }

  let { kind, payload, context = {}, animate = true }: Props = $props();

  // Seed before child Card mounts — same pattern as SceneRenderer.
  // `$effect` runs too late; Card captures context at init for sheet expand.
  const rootContext = untrack(() => context);
  setLiquidContext(rootContext);

  const ICON_MAP: Record<string, typeof LucideIcon> = {
    sparkles: Sparkles,
    lock: Lock,
    globe: Globe,
    "message-circle": MessageCircle,
    brain: Brain,
    shield: Shield,
    code: Code,
    cpu: Cpu,
    zap: Zap,
    clock: Clock,
    hourglass: Hourglass,
    coins: Coins,
    tag: Tag,
    mic: Mic,
    pencil: Pencil,
    "file-code": FileCode,
    table: Table,
    layers: Layers,
    rocket: Rocket,
    star: Star,
    check: Check,
    x: X,
    info: Info,
    "alert-triangle": AlertTriangle,
    search: Search,
    book: Book,
    map: Map,
    compass: Compass,
  };

  function cardNode(id: string, card: LiquidCardProps) {
    return createNode({
      id,
      type: "card",
      props: {
        title: card.title,
        ...(card.subtitle ? { subtitle: card.subtitle } : {}),
        ...(card.body ? { body: card.body } : {}),
        ...(card.emoji ? { emoji: card.emoji } : {}),
        ...(card.image ? { image: card.image } : {}),
        ...(card.meta ? { meta: card.meta } : {}),
        ...(card.summary ? { summary: card.summary } : {}),
        ...(card.chips?.length ? { chips: card.chips } : {}),
        ...(card.points?.length ? { points: card.points } : {}),
        ...(card.badges?.length ? { badges: card.badges } : {}),
      },
      fillState: "ready",
    });
  }

  const card = $derived.by(() => {
    if (kind !== "card") return null;
    const props = payload as LiquidCardProps;
    if (!props?.title) return null;
    return cardNode("md-card", props);
  });

  const carousel = $derived.by(() => {
    if (kind !== "carousel") return null;
    const items = (payload as { items?: LiquidCardProps[] })?.items ?? [];
    if (items.length === 0) return null;
    return createNode({
      id: "md-carousel",
      type: "carousel",
      props: {},
      slots: {
        items: items.map((item, i) => cardNode(`md-carousel-${i}`, item)),
      },
      fillState: "ready",
    });
  });

  const actions = $derived.by(() => {
    if (kind !== "actions") return [];
    const list = (payload as { actions?: LiquidActionProps[] })?.actions ?? [];
    return list.map((action, i) =>
      createNode({
        id: `md-action-${i}`,
        type: "action_row",
        props: {
          label: action.label,
          intent: action.intent ?? action.label,
          chevron: true,
          ...(action.emoji ? { emoji: action.emoji } : {}),
        },
        fillState: "ready",
      }),
    );
  });

  const callout = $derived.by(() => {
    if (kind !== "callout") return null;
    const props = payload as LiquidCalloutProps;
    if (!props?.body) return null;
    return createNode({
      id: "md-callout",
      type: "callout",
      props: {
        body: props.body,
        ...(props.tone ? { tone: props.tone } : {}),
        ...(props.title ? { title: props.title } : {}),
      },
      fillState: "ready",
    });
  });

  const section = $derived.by(() => {
    if (kind !== "section") return null;
    const props = payload as LiquidSectionProps;
    if (!props?.title) return null;
    const slots: Record<string, ReturnType<typeof createNode>[]> = {};
    if (props.body?.trim()) {
      slots.content = [
        createNode({
          id: "md-section-body",
          type: "prose",
          props: { markdown: props.body },
          fillState: "ready",
        }),
      ];
    }
    return createNode({
      id: "md-section",
      type: "section",
      props: {
        title: props.title,
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
      },
      slots,
      fillState: "ready",
    });
  });

  const chips = $derived.by(() => {
    if (kind !== "chips") return null;
    const list = (payload as { chips?: LiquidChipProps[] })?.chips ?? [];
    if (list.length === 0) return null;
    return createNode({
      id: "md-chips",
      type: "chip_group",
      props: {},
      slots: {
        chips: list.map((item, i) =>
          createNode({
            id: `md-chip-${i}`,
            type: "chip",
            props: {
              label: item.label,
              ...(item.tone ? { tone: item.tone } : {}),
              ...(item.value ? { value: item.value } : { value: item.label }),
            },
            fillState: "ready",
          }),
        ),
      },
      fillState: "ready",
    });
  });

  const media = $derived.by(() => {
    if (kind !== "media") return null;
    const props = payload as LiquidMediaProps;
    if (!props?.src) return null;
    return createNode({
      id: "md-media",
      type: "media",
      props: {
        src: props.src,
        ...(props.alt ? { alt: props.alt } : {}),
        ...(props.caption ? { caption: props.caption } : {}),
        ...(props.ratio ? { ratio: props.ratio } : {}),
      },
      fillState: "ready",
    });
  });

  const cite = $derived.by(() => {
    if (kind !== "cite") return null;
    const props = payload as LiquidCiteProps;
    if (!props?.quote && !props?.title && !props?.url) return null;
    return createNode({
      id: "md-cite",
      type: "cite",
      props: {
        ...(props.quote ? { quote: props.quote } : {}),
        ...(props.title ? { title: props.title } : {}),
        ...(props.url ? { url: props.url } : {}),
        ...(props.source ? { source: props.source } : {}),
      },
      fillState: "ready",
    });
  });

  const compare = $derived.by(() => {
    if (kind !== "compare") return null;
    const props = payload as LiquidCompareProps;
    if (!props?.axes?.length || !props?.entities || props.entities.length < 2) return null;
    return createNode({
      id: "md-compare",
      type: "compare",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.recommendation ? { recommendation: props.recommendation } : {}),
        ...(props.mode ? { mode: props.mode } : {}),
        axes: props.axes,
        entities: props.entities,
      },
      fillState: "ready",
    });
  });

  const plan = $derived.by(() => {
    if (kind !== "plan") return null;
    const props = payload as LiquidPlanProps;
    if (!props?.segments || props.segments.length < 2) return null;
    return createNode({
      id: "md-plan",
      type: "plan",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.grouping ? { grouping: props.grouping } : {}),
        segments: props.segments,
      },
      fillState: "ready",
    });
  });

  const timeline = $derived.by(() => {
    if (kind !== "timeline") return null;
    const props = payload as LiquidTimelineProps;
    if (!props?.events || props.events.length < 2) return null;
    return createNode({
      id: "md-timeline",
      type: "timeline",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.granularity ? { granularity: props.granularity } : {}),
        events: props.events,
      },
      fillState: "ready",
    });
  });

  const shortlist = $derived.by(() => {
    if (kind !== "shortlist") return null;
    const props = payload as LiquidShortlistProps;
    if (!props?.items || props.items.length < 2) return null;
    return createNode({
      id: "md-shortlist",
      type: "shortlist",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.criteria ? { criteria: props.criteria } : {}),
        ...(props.density ? { density: props.density } : {}),
        items: props.items,
      },
      fillState: "ready",
    });
  });

  const decision = $derived.by(() => {
    if (kind !== "decision") return null;
    const props = payload as LiquidDecisionProps;
    if (!props?.options || props.options.length < 2) return null;
    return createNode({
      id: "md-decision",
      type: "decision",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.factors ? { factors: props.factors } : {}),
        ...(props.recommendation ? { recommendation: props.recommendation } : {}),
        options: props.options,
      },
      fillState: "ready",
    });
  });

  const brief = $derived.by(() => {
    if (kind !== "brief") return null;
    const props = payload as LiquidBriefProps;
    if (!props?.sections || props.sections.length < 1) return null;
    return createNode({
      id: "md-brief",
      type: "brief",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.tone ? { tone: props.tone } : {}),
        sections: props.sections,
        ...(props.sources?.length ? { sources: props.sources } : {}),
      },
      fillState: "ready",
    });
  });

  const dashboard = $derived.by(() => {
    if (kind !== "dashboard") return null;
    const props = payload as LiquidDashboardProps;
    if (!props?.tiles || props.tiles.length < 2) return null;
    return createNode({
      id: "md-dashboard",
      type: "dashboard",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.columns ? { columns: props.columns } : {}),
        tiles: props.tiles,
      },
      fillState: "ready",
    });
  });

  const report = $derived.by(() => {
    if (kind !== "report") return null;
    const props = payload as LiquidReportProps;
    if (!props?.body && !props?.title) return null;
    return createNode({
      id: "md-report",
      type: "report",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.columns ? { columns: props.columns } : {}),
        body: props.body ?? "",
      },
      fillState: "ready",
    });
  });

  const slides = $derived.by(() => {
    if (kind !== "slides") return null;
    const props = payload as LiquidSlidesProps;
    if (!props?.slides?.length) return null;
    return createNode({
      id: "md-slides",
      type: "slides",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.theme ? { theme: props.theme } : {}),
        ...(props.columns ? { columns: props.columns } : {}),
        ...(props.showAll != null ? { showAll: props.showAll } : {}),
        ...(props.exportPaper != null ? { exportPaper: props.exportPaper } : {}),
        slides: props.slides,
      },
      fillState: "ready",
    });
  });

  const chart = $derived.by(() => {
    if (kind !== "chart") return null;
    const props = payload as LiquidChartProps;
    const hasCategorySeries = Boolean(props?.categories?.length && props?.series?.length);
    const hasPoints = Boolean(props?.points?.length);
    const hasMatrix = Boolean(props?.matrix?.rows?.length && props?.matrix?.cols?.length);
    if (!hasCategorySeries && !hasPoints && !hasMatrix) return null;
    return createNode({
      id: "md-chart",
      type: "chart",
      props: {
        type: props.type,
        categories: props.categories ?? [],
        series: props.series ?? [],
        ...(props.points?.length ? { points: props.points } : {}),
        ...(props.matrix ? { matrix: props.matrix } : {}),
        ...(props.seriesMarks?.length ? { seriesMarks: props.seriesMarks } : {}),
        ...(props.title ? { title: props.title } : {}),
        ...(props.description ? { description: props.description } : {}),
        ...(props.layout ? { layout: props.layout } : {}),
        ...(props.stacked != null ? { stacked: props.stacked } : {}),
        ...(props.curve ? { curve: props.curve } : {}),
        ...(props.separator != null ? { separator: props.separator } : {}),
        ...(props.centerLabel ? { centerLabel: props.centerLabel } : {}),
        ...(props.centerValue ? { centerValue: props.centerValue } : {}),
        ...(props.trend ? { trend: props.trend } : {}),
        ...(props.trendDirection ? { trendDirection: props.trendDirection } : {}),
        ...(props.caption ? { caption: props.caption } : {}),
        ...(props.labels ? { labels: props.labels } : {}),
        ...(props.labelPosition ? { labelPosition: props.labelPosition } : {}),
        ...(props.tooltip != null ? { tooltip: props.tooltip } : {}),
        ...(props.legend != null ? { legend: props.legend } : {}),
        ...(props.interactive != null ? { interactive: props.interactive } : {}),
        ...(props.activeKey ? { activeKey: props.activeKey } : {}),
        ...(props.colors?.length ? { colors: props.colors } : {}),
        ...(props.width ? { width: props.width } : {}),
        ...(props.height ? { height: props.height } : {}),
        ...(props.surface ? { surface: props.surface } : {}),
      },
      fillState: "ready",
    });
  });

  const tabs = $derived.by(() => {
    if (kind !== "tabs") return null;
    const props = payload as LiquidTabsProps;
    if (!props?.panels || props.panels.length < 2) return null;
    return createNode({
      id: "md-tabs",
      type: "tabs",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.default ? { default: props.default } : {}),
        panels: props.panels,
      },
      fillState: "ready",
    });
  });

  const steps = $derived.by(() => {
    if (kind !== "steps") return null;
    const props = payload as LiquidStepsProps;
    if (!props?.steps || props.steps.length < 2) return null;
    return createNode({
      id: "md-steps",
      type: "steps",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        steps: props.steps,
      },
      fillState: "ready",
    });
  });

  const accordion = $derived.by(() => {
    if (kind !== "accordion") return null;
    const props = payload as LiquidAccordionProps;
    if (!props?.items || props.items.length < 1) return null;
    return createNode({
      id: "md-accordion",
      type: "accordion",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        ...(props.multiple != null ? { multiple: props.multiple } : {}),
        items: props.items,
      },
      fillState: "ready",
    });
  });

  const code = $derived.by(() => {
    if (kind !== "code") return null;
    const props = payload as LiquidCodeProps;
    if (!props?.source?.trim()) return null;
    return createNode({
      id: "md-code",
      type: "code",
      props: {
        source: props.source,
        ...(props.lang ? { lang: props.lang } : {}),
        ...(props.title ? { title: props.title } : {}),
        ...(props.diff != null ? { diff: props.diff } : {}),
        ...(props.copy != null ? { copy: props.copy } : {}),
      },
      fillState: "ready",
    });
  });

  const tree = $derived.by(() => {
    if (kind !== "tree") return null;
    const props = payload as LiquidTreeProps;
    if (!props?.nodes || props.nodes.length < 1) return null;
    return createNode({
      id: "md-tree",
      type: "tree",
      props: {
        ...(props.title ? { title: props.title } : {}),
        ...(props.subtitle ? { subtitle: props.subtitle } : {}),
        nodes: props.nodes,
      },
      fillState: "ready",
    });
  });

  const IconComp = $derived.by(() => {
    if (kind !== "icon") return null;
    const id = typeof payload === "string" ? payload : "";
    return ICON_MAP[id] ?? null;
  });

  const staggerKinds = new Set(["tabs", "steps", "accordion", "tree"]);
  const hostClass = $derived.by(() => {
    const base = animate ? "liquid-md-host liquid-md-enter" : "liquid-md-host";
    if (animate && staggerKinds.has(kind)) return `${base} liquid-md-stagger`;
    return base;
  });
</script>

{#if kind === "card" && card}
  <div class="{hostClass} liquid-md-host-card">
    <Card node={card} />
  </div>
{:else if kind === "carousel" && carousel}
  <div class="{hostClass} liquid-md-host-carousel">
    <Carousel node={carousel} />
  </div>
{:else if kind === "actions" && actions.length}
  <div class="{hostClass} liquid-md-host-actions">
    <p class="liquid-md-actions-whisper">Suggested</p>
    {#each actions as action (action.id)}
      <ActionRow node={action} />
    {/each}
  </div>
{:else if kind === "callout" && callout}
  <div class="{hostClass} liquid-md-host-callout">
    <Callout node={callout} />
  </div>
{:else if kind === "section" && section}
  <div class="{hostClass} liquid-md-host-section">
    <Section node={section} />
  </div>
{:else if kind === "chips" && chips}
  <div class="{hostClass} liquid-md-host-chips">
    <ChipGroup node={chips} />
  </div>
{:else if kind === "media" && media}
  <div class="{hostClass} liquid-md-host-media">
    <Media node={media} />
  </div>
{:else if kind === "cite" && cite}
  <div class="{hostClass} liquid-md-host-cite">
    <Cite node={cite} />
  </div>
{:else if kind === "compare" && compare}
  <div class="{hostClass} liquid-md-host-compare">
    <Compare node={compare} />
  </div>
{:else if kind === "plan" && plan}
  <div class="{hostClass} liquid-md-host-plan">
    <Plan node={plan} />
  </div>
{:else if kind === "timeline" && timeline}
  <div class="{hostClass} liquid-md-host-timeline">
    <Timeline node={timeline} />
  </div>
{:else if kind === "shortlist" && shortlist}
  <div class="{hostClass} liquid-md-host-shortlist">
    <Shortlist node={shortlist} />
  </div>
{:else if kind === "decision" && decision}
  <div class="{hostClass} liquid-md-host-decision">
    <Decision node={decision} />
  </div>
{:else if kind === "brief" && brief}
  <div class="{hostClass} liquid-md-host-brief">
    <Brief node={brief} />
  </div>
{:else if kind === "dashboard" && dashboard}
  <div class="{hostClass} liquid-md-host-dashboard">
    <Dashboard node={dashboard} />
  </div>
{:else if kind === "report" && report}
  <div class="{hostClass} liquid-md-host-report">
    <Report node={report} />
  </div>
{:else if kind === "slides" && slides}
  <div class="{hostClass} liquid-md-host-slides">
    <Slides node={slides} />
  </div>
{:else if kind === "chart" && chart}
  <div class="{hostClass} liquid-md-host-chart">
    <Chart node={chart} />
  </div>
{:else if kind === "tabs" && tabs}
  <div class="{hostClass} liquid-md-host-tabs">
    <Tabs node={tabs} />
  </div>
{:else if kind === "steps" && steps}
  <div class="{hostClass} liquid-md-host-steps">
    <Steps node={steps} />
  </div>
{:else if kind === "accordion" && accordion}
  <div class="{hostClass} liquid-md-host-accordion">
    <Accordion node={accordion} />
  </div>
{:else if kind === "code" && code}
  <div class="{hostClass} liquid-md-host-code">
    <CodeEmbed node={code} />
  </div>
{:else if kind === "tree" && tree}
  <div class="{hostClass} liquid-md-host-tree">
    <Tree node={tree} />
  </div>
{:else if kind === "icon" && IconComp}
  <span class="liquid-md-host-icon">
    <IconComp size={14} strokeWidth={2} aria-hidden="true" />
  </span>
{/if}

<style>
  .liquid-md-host {
    min-width: 0;
  }

  .liquid-md-host-actions {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .liquid-md-actions-whisper {
    margin: 0 0 0.15rem;
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .liquid-md-host-icon {
    display: inline-flex;
    vertical-align: -0.15em;
    margin-right: 0.2rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-md-enter {
    animation: liquid-md-enter 0.32s ease-out both;
  }

  .liquid-md-stagger :global(.liquid-tabs-tab),
  .liquid-md-stagger :global(.liquid-tabs-panel),
  .liquid-md-stagger :global(.liquid-steps-item),
  .liquid-md-stagger :global(.liquid-accordion-item),
  .liquid-md-stagger :global(.liquid-tree-node) {
    animation: liquid-md-enter 0.36s ease-out both;
    animation-delay: calc(var(--stagger, 0) * 45ms);
  }

  @keyframes liquid-md-enter {
    from {
      opacity: 0;
      transform: translateY(0.35rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-md-enter,
    .liquid-md-stagger :global(.liquid-tabs-tab),
    .liquid-md-stagger :global(.liquid-tabs-panel),
    .liquid-md-stagger :global(.liquid-steps-item),
    .liquid-md-stagger :global(.liquid-accordion-item),
    .liquid-md-stagger :global(.liquid-tree-node) {
      animation: none;
    }
  }
</style>
