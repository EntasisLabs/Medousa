<script lang="ts">
  /**
   * Host for markdown-hydrated Liquid embeds.
   * Mounted into placeholder divs by hydrateLiquidEmbeds.
   */
  import "$lib/liquid/archetypes";
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
  import Section from "$lib/liquid/archetypes/molecules/section/Section.svelte";
  import ChipGroup from "$lib/liquid/archetypes/molecules/chip_group/ChipGroup.svelte";
  import Media from "$lib/liquid/archetypes/atoms/media/Media.svelte";
  import type {
    LiquidActionProps,
    LiquidBriefProps,
    LiquidCalloutProps,
    LiquidCardProps,
    LiquidChipProps,
    LiquidCiteProps,
    LiquidCompareProps,
    LiquidDashboardProps,
    LiquidChartProps,
    LiquidDecisionProps,
    LiquidEmbedKind,
    LiquidMediaProps,
    LiquidPlanProps,
    LiquidSectionProps,
    LiquidShortlistProps,
    LiquidTimelineProps,
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
  }

  let { kind, payload, context = {} }: Props = $props();

  $effect(() => {
    setLiquidContext(context);
  });

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

  const chart = $derived.by(() => {
    if (kind !== "chart") return null;
    const props = payload as LiquidChartProps;
    if (!props?.categories?.length || !props?.series?.length) return null;
    return createNode({
      id: "md-chart",
      type: "chart",
      props: {
        type: props.type,
        categories: props.categories,
        series: props.series,
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
      },
      fillState: "ready",
    });
  });

  const IconComp = $derived.by(() => {
    if (kind !== "icon") return null;
    const id = typeof payload === "string" ? payload : "";
    return ICON_MAP[id] ?? null;
  });
</script>

{#if kind === "card" && card}
  <div class="liquid-md-host liquid-md-host-card liquid-md-enter">
    <Card node={card} />
  </div>
{:else if kind === "carousel" && carousel}
  <div class="liquid-md-host liquid-md-host-carousel liquid-md-enter">
    <Carousel node={carousel} />
  </div>
{:else if kind === "actions" && actions.length}
  <div class="liquid-md-host liquid-md-host-actions liquid-md-enter">
    <p class="liquid-md-actions-whisper">Suggested</p>
    {#each actions as action (action.id)}
      <ActionRow node={action} />
    {/each}
  </div>
{:else if kind === "callout" && callout}
  <div class="liquid-md-host liquid-md-host-callout liquid-md-enter">
    <Callout node={callout} />
  </div>
{:else if kind === "section" && section}
  <div class="liquid-md-host liquid-md-host-section liquid-md-enter">
    <Section node={section} />
  </div>
{:else if kind === "chips" && chips}
  <div class="liquid-md-host liquid-md-host-chips liquid-md-enter">
    <ChipGroup node={chips} />
  </div>
{:else if kind === "media" && media}
  <div class="liquid-md-host liquid-md-host-media liquid-md-enter">
    <Media node={media} />
  </div>
{:else if kind === "cite" && cite}
  <div class="liquid-md-host liquid-md-host-cite liquid-md-enter">
    <Cite node={cite} />
  </div>
{:else if kind === "compare" && compare}
  <div class="liquid-md-host liquid-md-host-compare liquid-md-enter">
    <Compare node={compare} />
  </div>
{:else if kind === "plan" && plan}
  <div class="liquid-md-host liquid-md-host-plan liquid-md-enter">
    <Plan node={plan} />
  </div>
{:else if kind === "timeline" && timeline}
  <div class="liquid-md-host liquid-md-host-timeline liquid-md-enter">
    <Timeline node={timeline} />
  </div>
{:else if kind === "shortlist" && shortlist}
  <div class="liquid-md-host liquid-md-host-shortlist liquid-md-enter">
    <Shortlist node={shortlist} />
  </div>
{:else if kind === "decision" && decision}
  <div class="liquid-md-host liquid-md-host-decision liquid-md-enter">
    <Decision node={decision} />
  </div>
{:else if kind === "brief" && brief}
  <div class="liquid-md-host liquid-md-host-brief liquid-md-enter">
    <Brief node={brief} />
  </div>
{:else if kind === "dashboard" && dashboard}
  <div class="liquid-md-host liquid-md-host-dashboard liquid-md-enter">
    <Dashboard node={dashboard} />
  </div>
{:else if kind === "chart" && chart}
  <div class="liquid-md-host liquid-md-host-chart liquid-md-enter">
    <Chart node={chart} />
  </div>
{:else if kind === "icon" && IconComp}
  <span class="liquid-md-host-icon">
    <IconComp size={14} strokeWidth={2} aria-hidden="true" />
  </span>
{/if}

<style>
  .liquid-md-host {
    margin: 1.15rem 0;
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
    .liquid-md-enter {
      animation: none;
    }
  }
</style>
