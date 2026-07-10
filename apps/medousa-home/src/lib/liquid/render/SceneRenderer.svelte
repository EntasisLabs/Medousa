<script lang="ts">
  /**
   * The dumb, fast, keyed walker. Resolves a node's archetype to its Svelte body
   * and renders it. In `skeleton` state it paints generic bones instantly. The
   * root instance seeds the render context; nested instances inherit it.
   */
  import { untrack } from "svelte";
  import type { SceneNode } from "$lib/liquid/core";
  import { resolveComponent } from "./componentRegistry";
  import { setLiquidContext, type LiquidRenderContext } from "./context";
  import SkeletonBlock from "./SkeletonBlock.svelte";

  interface Props {
    node: SceneNode;
    /** Provide once at the root; nested renderers inherit via Svelte context. */
    context?: LiquidRenderContext;
  }
  let { node, context }: Props = $props();

  // Context is bound once at init; capture the current value intentionally.
  const rootContext = untrack(() => context);
  if (rootContext) setLiquidContext(rootContext);

  const Renderer = $derived(resolveComponent(node.type));
</script>

{#if node.fillState === "skeleton"}
  <SkeletonBlock type={node.type} />
{:else if Renderer}
  <Renderer {node} />
{:else}
  <div class="liquid-unknown" role="note">
    Unknown archetype <code>{node.type}</code>
  </div>
{/if}

<style>
  .liquid-unknown {
    padding: 0.5rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px dashed color-mix(in srgb, var(--color-warning-500) 55%, transparent);
    color: rgb(var(--color-warning-200));
    font-size: 0.75rem;
  }

  .liquid-unknown code {
    font-family: ui-monospace, monospace;
  }
</style>
