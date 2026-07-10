<script lang="ts">
  /**
   * Host for markdown-hydrated Liquid embeds (card / carousel / actions / icon).
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
  import type {
    LiquidActionProps,
    LiquidCardProps,
    LiquidEmbedKind,
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

  const IconComp = $derived.by(() => {
    if (kind !== "icon") return null;
    const id = typeof payload === "string" ? payload : "";
    return ICON_MAP[id] ?? null;
  });
</script>

{#if kind === "card" && card}
  <div class="liquid-md-host liquid-md-host-card">
    <Card node={card} />
  </div>
{:else if kind === "carousel" && carousel}
  <div class="liquid-md-host liquid-md-host-carousel">
    <Carousel node={carousel} />
  </div>
{:else if kind === "actions" && actions.length}
  <div class="liquid-md-host liquid-md-host-actions">
    {#each actions as action (action.id)}
      <ActionRow node={action} />
    {/each}
  </div>
{:else if kind === "icon" && IconComp}
  <span class="liquid-md-host-icon">
    <IconComp size={14} strokeWidth={2} aria-hidden="true" />
  </span>
{/if}

<style>
  .liquid-md-host {
    margin: 0.75rem 0;
    min-width: 0;
  }

  .liquid-md-host-actions {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .liquid-md-host-icon {
    display: inline-flex;
    vertical-align: -0.15em;
    margin-right: 0.2rem;
    color: rgb(var(--color-surface-400));
  }
</style>
