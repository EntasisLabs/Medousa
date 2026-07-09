<script lang="ts">
  /**
   * Liquid UI dev harness — proves the scene renderer + reuse atoms and the
   * structure-then-fill choreography (bones paint first, content streams in).
   * Not shipped in nav; reach it at /dev/liquid.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import { applyOp, createNode, createScene, type Scene } from "$lib/liquid/core";

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
</style>
