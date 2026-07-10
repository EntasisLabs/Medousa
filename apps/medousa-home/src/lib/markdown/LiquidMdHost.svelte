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
  import Section from "$lib/liquid/archetypes/molecules/section/Section.svelte";
  import ChipGroup from "$lib/liquid/archetypes/molecules/chip_group/ChipGroup.svelte";
  import Media from "$lib/liquid/archetypes/atoms/media/Media.svelte";
  import type {
    LiquidActionProps,
    LiquidCalloutProps,
    LiquidCardProps,
    LiquidChipProps,
    LiquidCiteProps,
    LiquidEmbedKind,
    LiquidMediaProps,
    LiquidSectionProps,
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

  setLiquidContext(context);

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
