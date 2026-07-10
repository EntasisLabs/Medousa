<script lang="ts">
  /**
   * Renders a durable Liquid scene component pinned to a custom surface.
   *
   * The daemon stores `config.scene` as an opaque payload (`{ ops: [...] }`, the
   * same op JSON as `cognition_ui_scene`) and never interprets it. Here we decode
   * those ops into a typed `Scene` and render it through the shared PR2
   * `SceneRenderer` — the same renderer the chat surface uses.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import type { LiquidRenderContext } from "$lib/liquid/render";
  import { applyOps, createScene, type SceneEvent } from "$lib/liquid/core";
  import { decodeSceneOps } from "$lib/liquid/surfaces/chat/sceneStream";
  import type { EventSink } from "$lib/liquid/ports";

  interface Props {
    config: Record<string, unknown>;
    componentId: string;
    sessionId: string;
    label?: string | null;
    compact?: boolean;
    mobile?: boolean;
  }

  let {
    config,
    componentId,
    sessionId,
    label = null,
    compact = false,
    mobile = false,
  }: Props = $props();

  const surfaceId = $derived(`env:${componentId}`);

  /** Opaque scene payload → ordered wire ops (defensive: tolerate shapes). */
  const wireOps = $derived.by<unknown[]>(() => {
    const scene = config?.scene as Record<string, unknown> | undefined;
    if (!scene || typeof scene !== "object") return [];
    const ops = scene.ops;
    return Array.isArray(ops) ? ops : [];
  });

  const scene = $derived.by(() => {
    const ops = decodeSceneOps(wireOps, surfaceId);
    if (ops.length === 0) return null;
    return applyOps(createScene(surfaceId), ops);
  });

  // Environment scenes are read-only in v1: user events are recorded but do not
  // yet spawn turns (chat owns the bidirectional loop). Wiring is a follow-up.
  const sink: EventSink = {
    emit(_event: SceneEvent) {
      /* no-op for durable environment scenes (v1) */
    },
  };

  const context = $derived<LiquidRenderContext>({
    sink,
    sessionId,
    compact,
    mobile,
    openLinksInWeb: true,
  });
</script>

{#if scene?.root}
  <div class="environment-scene-view" role="group" aria-label={label ?? "Scene"}>
    <SceneRenderer node={scene.root} {context} />
  </div>
{:else}
  <div class="environment-scene-view environment-scene-view-empty">
    <p>Scene has no content yet.</p>
  </div>
{/if}

<style>
  .environment-scene-view {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    height: 100%;
    overflow: auto;
    padding: 0.85rem;
  }

  .environment-scene-view-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgb(var(--color-surface-400));
    font-size: 0.8125rem;
  }
</style>
