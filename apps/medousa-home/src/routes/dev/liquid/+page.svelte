<script lang="ts">
  /**
   * Liquid UI dev harness — proves the scene renderer, structure-then-fill, and
   * the Monogram molecule composition (section / carousel / card+detail / chips /
   * action_row) with a live event log. Not shipped in nav; reach it at /dev/liquid.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import type { LiquidRenderContext } from "$lib/liquid/render";
  import type { SceneEvent } from "$lib/liquid/core";
  import type { EventSink } from "$lib/liquid/ports";
  import { applyOp, createNode, createScene, type Scene, type SceneNode } from "$lib/liquid/core";

  const DEMO_IMAGE =
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      `<svg xmlns='http://www.w3.org/2000/svg' width='640' height='360'>
        <defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
          <stop offset='0' stop-color='#6d5efc'/><stop offset='1' stop-color='#22d3ee'/>
        </linearGradient></defs>
        <rect width='640' height='360' fill='url(#g)'/>
        <text x='50%' y='50%' fill='white' font-family='sans-serif' font-size='28'
          text-anchor='middle' dominant-baseline='middle'>Liquid UI</text>
      </svg>`,
    );

  // ---- Stage 1: structure-then-fill ----------------------------------------

  function buildInitial(): Scene {
    let s = createScene("dev");
    s = applyOp(s, {
      op: "plan_layout",
      surfaceId: "dev",
      rev: 1,
      root: createNode({
        id: "root",
        type: "stack",
        props: { direction: "v", gap: "lg" },
        fillState: "ready",
        slots: {
          children: [
            createNode({
              id: "pill",
              type: "status_pill",
              props: { label: "Searching the web…", state: "loading" },
              fillState: "ready",
            }),
            createNode({ id: "intro", type: "prose", fillState: "skeleton" }),
            createNode({ id: "shot", type: "media", fillState: "skeleton" }),
          ],
        },
      }),
    });
    return s;
  }

  let scene = $state<Scene>(buildInitial());

  function fill() {
    scene = applyOp(scene, {
      op: "patch_props",
      nodeId: "pill",
      props: { label: "Here's what I found", state: "ok" },
    });
    scene = applyOp(scene, {
      op: "patch_props",
      nodeId: "intro",
      props: {
        markdown:
          "## Liquid UI\n\nThe scene paints its **bones first**, then content streams into each slot in place — the node `id` keeps every instance stable across fills.\n\n- Structure before fill\n- Generate more than you show\n- Native work becomes a component",
      },
    });
    scene = applyOp(scene, { op: "set_fill_state", nodeId: "intro", state: "ready" });
    scene = applyOp(scene, {
      op: "patch_props",
      nodeId: "shot",
      props: { src: DEMO_IMAGE, alt: "Liquid UI", caption: "A composed scene, rendered node-by-node.", ratio: "16/9" },
    });
    scene = applyOp(scene, { op: "set_fill_state", nodeId: "shot", state: "ready" });
  }

  function reset() {
    scene = buildInitial();
  }

  // ---- Stage 2: Monogram molecule composition ------------------------------

  function laptopCard(id: string, emoji: string, title: string, subtitle: string, note: string): SceneNode {
    return createNode({
      id,
      type: "card",
      props: { emoji, title, subtitle, badges: ["16GB", "M-series"] },
      fillState: "ready",
      slots: {
        detail: [
          createNode({
            id: `${id}:detail`,
            type: "prose",
            props: { markdown: note },
            fillState: "ready",
          }),
        ],
      },
    });
  }

  function chipNode(id: string, label: string, tone?: string): SceneNode {
    return createNode({ id, type: "chip", props: { label, tone, value: label }, fillState: "ready" });
  }

  function actionNode(id: string, emoji: string, label: string): SceneNode {
    return createNode({ id, type: "action_row", props: { emoji, label, intent: label }, fillState: "ready" });
  }

  const monogram: SceneNode = createNode({
    id: "mono",
    type: "document",
    fillState: "ready",
    slots: {
      flow: [
        createNode({
          id: "mono:intro",
          type: "section",
          props: { title: "Laptops for video editing", subtitle: "Three saved picks, compared." },
          fillState: "ready",
          slots: {
            content: [
              createNode({
                id: "mono:prose",
                type: "prose",
                props: { markdown: "Tap a card to expose its full spec sheet — the detail renders **only when opened**." },
                fillState: "ready",
              }),
            ],
          },
        }),
        createNode({
          id: "mono:carousel",
          type: "carousel",
          fillState: "ready",
          slots: {
            items: [
              laptopCard("mono:c1", "💻", "Studio 14", "Balanced · $1,999", "Great all-rounder: quiet fans, bright display, 10h battery."),
              laptopCard("mono:c2", "🖥️", "Pro 16", "Powerhouse · $2,899", "Best raw export speed; heavier and pricier, but no thermal throttle."),
              laptopCard("mono:c3", "🪶", "Air 13", "Featherweight · $1,299", "Silent and light; fine for 1080p timelines, tight on 4K scrubbing."),
            ],
          },
        }),
        createNode({
          id: "mono:filters",
          type: "section",
          props: { title: "Narrow it down" },
          fillState: "ready",
          slots: {
            content: [
              createNode({
                id: "mono:chips",
                type: "chip_group",
                fillState: "ready",
                slots: {
                  chips: [
                    chipNode("mono:chip1", "Under $2k", "accent"),
                    chipNode("mono:chip2", "Best battery", "success"),
                    chipNode("mono:chip3", "Lightest"),
                  ],
                },
              }),
            ],
          },
        }),
        createNode({
          id: "mono:next",
          type: "section",
          props: { title: "What would you like next?" },
          fillState: "ready",
          slots: {
            content: [
              actionNode("mono:a1", "⚖️", "Compare them side by side"),
              actionNode("mono:a2", "📌", "Pin the Pro 16 to my vault"),
              actionNode("mono:a3", "🔎", "Find cheaper alternatives"),
            ],
          },
        }),
      ],
    },
  });

  let events = $state<string[]>([]);

  const sink: EventSink = {
    emit(event: SceneEvent) {
      const payload = event.payload ? ` ${JSON.stringify(event.payload)}` : "";
      events = [`${event.type} · ${event.nodeId}${payload}`, ...events].slice(0, 8);
    },
  };

  const monogramContext: LiquidRenderContext = { sink, openLinksInWeb: false };
</script>

<div class="harness">
  <header class="harness-head">
    <h1>Liquid UI — scene renderer harness</h1>
    <div class="harness-actions">
      <button type="button" onclick={fill}>Fill</button>
      <button type="button" class="ghost" onclick={reset}>Reset</button>
    </div>
  </header>

  <p class="harness-note">
    rev {scene.rev} · click <strong>Fill</strong> to stream content into the skeleton slots.
  </p>

  <section class="harness-stage">
    {#if scene.root}
      <SceneRenderer node={scene.root} context={{ openLinksInWeb: false }} />
    {/if}
  </section>

  <h2 class="harness-subhead">Monogram molecules</h2>
  <p class="harness-note">
    section · carousel · card (tap for lazy detail) · chip_group · action_row — interactions stream to the log below.
  </p>

  <section class="harness-stage">
    <SceneRenderer node={monogram} context={monogramContext} />
  </section>

  <section class="harness-log">
    <p class="harness-log-title">events</p>
    {#if events.length}
      <ul>
        {#each events as line, index (index)}
          <li>{line}</li>
        {/each}
      </ul>
    {:else}
      <p class="harness-log-empty">Tap a card, chip, or suggestion…</p>
    {/if}
  </section>
</div>

<style>
  .harness {
    max-width: 44rem;
    margin: 0 auto;
    padding: 2rem 1.25rem 4rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    color: rgb(var(--color-surface-100));
  }

  .harness-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .harness-head h1 {
    font-size: 1.05rem;
    font-weight: 600;
    margin: 0;
  }

  .harness-subhead {
    margin: 1.25rem 0 0;
    font-size: 0.95rem;
    font-weight: 600;
  }

  .harness-actions {
    display: flex;
    gap: 0.5rem;
  }

  .harness-actions button {
    padding: 0.35rem 0.85rem;
    border-radius: 0.5rem;
    font-size: 0.8rem;
    font-weight: 500;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600));
    border: 1px solid transparent;
    cursor: pointer;
  }

  .harness-actions button.ghost {
    background: transparent;
    color: rgb(var(--color-surface-200));
    border-color: color-mix(in srgb, var(--color-surface-500) 45%, transparent);
  }

  .harness-note {
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
    margin: 0;
  }

  .harness-stage {
    padding: 1.25rem;
    border-radius: 1rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 45%, transparent);
  }

  .harness-log {
    padding: 0.85rem 1rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 55%, transparent);
    font-family: ui-monospace, monospace;
    font-size: 0.7rem;
  }

  .harness-log-title {
    margin: 0 0 0.4rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: rgb(var(--color-surface-400));
  }

  .harness-log ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .harness-log li {
    color: rgb(var(--color-primary-200));
  }

  .harness-log-empty {
    margin: 0;
    color: rgb(var(--color-surface-500));
  }
</style>
