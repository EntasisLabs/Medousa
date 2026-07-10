<script lang="ts">
  import LayoutWidgetTile from "$lib/components/environment/LayoutWidgetTile.svelte";
  import type { ComponentDef } from "$lib/types/environment";

  interface Props {
    components: ComponentDef[];
    sessionId: string;
    profileId?: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
  }

  let {
    components,
    sessionId,
    profileId = null,
    feedStateForComponent,
  }: Props = $props();

  /** One widget → edge-to-edge fill above the tab bar (not a short rounded card). */
  const fill = $derived(components.length === 1);
</script>

<div
  class="mobile-stack-layout"
  class:mobile-stack-layout-fill={fill}
  role="list"
>
  {#each components as component (component.id)}
    <article class="mobile-stack-card" role="listitem">
      <LayoutWidgetTile
        {component}
        {sessionId}
        {profileId}
        feedState={feedStateForComponent(component.id)}
        editing={false}
        draggable={false}
      />
    </article>
  {/each}
</div>

<style>
  .mobile-stack-layout {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.15rem 0 1.25rem;
    min-height: 0;
  }

  .mobile-stack-layout-fill {
    flex: 1 1 auto;
    height: 100%;
    gap: 0;
    padding: 0;
  }

  .mobile-stack-card {
    display: flex;
    flex-direction: column;
    min-height: min(52vh, 28rem);
    max-height: none;
    border-radius: 0.85rem;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 70%, transparent);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--color-surface-950) 45%, transparent);
  }

  .mobile-stack-layout-fill .mobile-stack-card {
    flex: 1 1 auto;
    min-height: 0;
    border-radius: 0;
    border: 0;
    box-shadow: none;
    background: transparent;
    overflow: hidden;
  }

  .mobile-stack-card :global(.layout-widget-tile) {
    flex: 1 1 auto;
    min-height: min(52vh, 28rem);
    min-width: 0;
    border-radius: 0;
    box-shadow: none;
  }

  .mobile-stack-layout-fill .mobile-stack-card :global(.layout-widget-tile) {
    min-height: 0;
    height: 100%;
    background: transparent;
  }

  /* Let inner carousels/chips own horizontal scroll — don't create a competing scroller. */
  .mobile-stack-card :global(.layout-widget-tile-body),
  .mobile-stack-card :global(.layout-widget-body) {
    min-width: 0;
    min-height: 0;
  }
</style>
